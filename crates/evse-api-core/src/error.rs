use thiserror::Error;

#[derive(Error, Debug)]
pub enum EvseApiError {
    #[error("libiso15118: {0}")]
    Iso15118(String),

    #[error("JSON: {0}")]
    Json(#[from] serde_json::Error),

    #[error("session not found: {0}")]
    SessionNotFound(String),

    #[error("config error: {0}")]
    Config(String),

    #[error("channel closed")]
    ChannelClosed,
}
