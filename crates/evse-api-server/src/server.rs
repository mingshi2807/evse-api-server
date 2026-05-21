use axum::{
    Router,
    extract::State,
    extract::ws::{Message, WebSocket, WebSocketUpgrade},
    response::Json,
    routing::get,
};
use evse_api_core::{manager::SessionManager, protocol::Command, session::Session};
use serde_json::json;
use std::sync::Arc;
use std::time::Instant;
use tokio::sync::mpsc;

pub struct AppState {
    pub manager: Arc<SessionManager>,
    pub start_time: Instant,
}

pub fn build_router(manager: Arc<SessionManager>) -> Router {
    let state = Arc::new(AppState {
        manager,
        start_time: Instant::now(),
    });
    Router::new()
        .route("/ws", get(ws_handler))
        .route("/api/v1/health", get(health_handler))
        .route("/api/v1/status", get(status_handler))
        .with_state(state)
}

async fn health_handler() -> Json<serde_json::Value> {
    Json(json!({"status": "ok"}))
}

async fn status_handler(State(state): State<Arc<AppState>>) -> Json<serde_json::Value> {
    Json(json!({
        "status": "running",
        "uptime_secs": state.start_time.elapsed().as_secs(),
        "sessions": 0,
        "version": env!("CARGO_PKG_VERSION"),
    }))
}

async fn ws_handler(
    ws: WebSocketUpgrade,
    State(state): State<Arc<AppState>>,
) -> axum::response::Response {
    ws.on_upgrade(move |socket| handle_ws(socket, state))
}

async fn handle_ws(mut socket: WebSocket, state: Arc<AppState>) {
    let session_id = uuid::Uuid::new_v4().to_string();
    let (api_tx, mut api_rx) = mpsc::unbounded_channel::<String>();

    let cfg_json = r#"{"evse_id":"default","interface":"lo","energy_services":["DC"],"auth_services":["EIM"],"control_mode":"Dynamic","mobility_mode":"ProvidedByEvcc","dc_limits":{"max_voltage":900,"max_current":250,"max_power":50000,"min_power":0}}"#;

    let (session, mut event_rx) = match Session::new(cfg_json) {
        Ok(s) => s,
        Err(e) => {
            let _ = socket.send(Message::Text(
                json!({"type":"error","session_id":session_id,"code":"INIT_FAILED","message":e.to_string()}).to_string().into()
            )).await;
            return;
        }
    };

    if let Err(e) = state
        .manager
        .add_session(session_id.clone(), session, api_tx)
        .await
    {
        let _ = socket.send(Message::Text(
            json!({"type":"error","session_id":session_id,"code":"ADD_FAILED","message":e.to_string()}).to_string().into()
        )).await;
        return;
    }

    let _ = socket
        .send(Message::Text(
            json!({"type":"status","message":"connected","session_id":session_id})
                .to_string()
                .into(),
        ))
        .await;

    let sid = session_id.clone();
    loop {
        tokio::select! {
            Some(event) = event_rx.recv() => {
                if socket.send(Message::Text(event.into())).await.is_err() {
                    break;
                }
            }
            Some(event) = api_rx.recv() => {
                if socket.send(Message::Text(event.into())).await.is_err() {
                    break;
                }
            }
            msg = socket.recv() => {
                match msg {
                    Some(Ok(Message::Text(text))) => {
                        if let Ok(Command::ControlEvent { event, .. }) = serde_json::from_str::<Command>(&text) {
                            let event_json = serde_json::to_string(&event).unwrap_or_default();
                            state.manager.push_event(&sid, &event_json);
                        }
                    }
                    _ => break,
                }
            }
        }
    }
}
