use serde::{Serialize, Serializer};

#[derive(Debug, thiserror::Error)]
pub enum AppError {
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),

    #[error("database error: {0}")]
    Db(#[from] rusqlite::Error),

    #[error("json error: {0}")]
    Json(#[from] serde_json::Error),

    #[error("project not found")]
    NotFound,

    #[error("invalid project name: {0}")]
    InvalidName(String),

    #[error("name already in use: {0}")]
    NameTaken(String),

    #[error("port range exhausted for service")]
    PortsExhausted,

    #[error("docker not available: {0}")]
    DockerUnavailable(String),

    #[error("scaffold failed: {0}")]
    Scaffold(String),

    #[error("sail command failed: {0}")]
    Sail(String),

    #[error("{0}")]
    Other(String),
}

impl Serialize for AppError {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(&self.to_string())
    }
}

pub type AppResult<T> = Result<T, AppError>;
