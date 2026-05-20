use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::{RwLock, mpsc};
use tokio::time::{Duration, interval};

use crate::error::EvseApiError;
use crate::session::Session;

pub struct SessionManager {
    cmd_tx: mpsc::UnboundedSender<ManagerCommand>,
}

struct ManagedSession {
    session: Session,
    api_tx: mpsc::UnboundedSender<String>,
}

enum ManagerCommand {
    AddSession {
        id: String,
        session: Session,
        api_tx: mpsc::UnboundedSender<String>,
        done_tx: tokio::sync::oneshot::Sender<Result<(), EvseApiError>>,
    },
    PushEvent {
        id: String,
        event_json: String,
    },
    #[allow(dead_code)]
    Shutdown,
}

impl Default for SessionManager {
    fn default() -> Self {
        Self::new()
    }
}

impl SessionManager {
    pub fn new() -> Self {
        let sessions: Arc<RwLock<HashMap<String, ManagedSession>>> =
            Arc::new(RwLock::new(HashMap::new()));
        let (cmd_tx, mut cmd_rx) = mpsc::unbounded_channel();

        let sessions_clone = sessions.clone();
        tokio::spawn(async move {
            let mut tick = interval(Duration::from_millis(50));
            loop {
                tokio::select! {
                    Some(cmd) = cmd_rx.recv() => {
                        match cmd {
                            ManagerCommand::AddSession { id, session, api_tx, done_tx } => {
                                sessions_clone.write().await.insert(
                                    id,
                                    ManagedSession { session, api_tx },
                                );
                                let _ = done_tx.send(Ok(()));
                            }
                            ManagerCommand::PushEvent { id, event_json } => {
                                if let Some(s) = sessions_clone.read().await.get(&id) {
                                    s.session.push_event(&event_json);
                                }
                            }
                            ManagerCommand::Shutdown => break,
                        }
                    }
                    _ = tick.tick() => {
                        let mut to_remove = Vec::new();
                        let sessions = sessions_clone.read().await;
                        for (id, managed) in sessions.iter() {
                            if managed.session.poll().is_none() {
                                to_remove.push(id.clone());
                            }
                        }
                        drop(sessions);
                        for id in to_remove {
                            if let Some(managed) = sessions_clone.write().await.remove(&id) {
                                let _ = managed.api_tx.send(serde_json::json!({
                                    "type": "session_closed",
                                    "session_id": id,
                                    "reason": "finished"
                                }).to_string());
                            }
                        }
                    }
                }
            }
        });

        SessionManager { cmd_tx }
    }

    pub async fn add_session(
        &self,
        id: String,
        session: Session,
        api_tx: mpsc::UnboundedSender<String>,
    ) -> Result<(), EvseApiError> {
        let (done_tx, done_rx) = tokio::sync::oneshot::channel();
        let _ = self.cmd_tx.send(ManagerCommand::AddSession {
            id,
            session,
            api_tx,
            done_tx,
        });
        done_rx.await.map_err(|_| EvseApiError::ChannelClosed)?
    }

    pub fn push_event(&self, id: &str, event_json: &str) {
        let _ = self.cmd_tx.send(ManagerCommand::PushEvent {
            id: id.to_string(),
            event_json: event_json.to_string(),
        });
    }
}
