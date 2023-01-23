use std::io;

use futures_retry_policies::ShouldRetry;
use tempfile::PersistError;
use thiserror::Error;
use tokio::sync::mpsc;
use tracing::warn;

pub type Result<T, E = Error> = std::result::Result<T, E>;

#[derive(Debug, Error)]
pub enum Error {
    #[error("reqwest_middleware error: {0}")]
    ReqwestMiddleware(#[from] reqwest_middleware::Error),
    #[error("reqwest error: {0}")]
    Reqwest(#[from] reqwest::Error),
    #[error("channel closed")]
    Sender,
    #[error("io error: {0}")]
    IO(#[from] io::Error),
    #[error("json error: {0}")]
    Json(#[from] serde_json::Error),
    #[error("join error: {0}")]
    Join(#[from] tokio::task::JoinError),
    #[error("handlebars render error: {0}")]
    Handlebars(#[from] handlebars::RenderError),
    #[error("base64 decode error: {0}")]
    Base64Decode(#[from] base64::DecodeError),
    #[error("rsa error: {0}")]
    Rsa(#[from] rsa::errors::Error),
    #[error("bytes stream stuck")]
    StreamStuck,
    #[error("atomic file poisoned")]
    AtomicFilePoisoned,
    #[error("atomic file write error: {0}")]
    AtomicFileWrite(#[from] PersistError),
}

impl ShouldRetry for Error {
    fn should_retry(&self, attempts: u32) -> bool {
        let retry = matches!(
            self,
            Self::Reqwest(_) | Self::ReqwestMiddleware(_) | Self::IO(_) | Self::StreamStuck
        );
        warn!(attempts, retry, e=?self, "Error occurred, retrying");
        retry
    }
}

impl<T> From<mpsc::error::SendError<T>> for Error {
    fn from(_value: mpsc::error::SendError<T>) -> Self {
        Self::Sender
    }
}
