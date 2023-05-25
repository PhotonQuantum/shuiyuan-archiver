use std::io;

use futures_retry_policies::ShouldRetry;
use lol_html::errors::RewritingError;
use reqwest::StatusCode;
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
    #[error("rewriting error: {0}")]
    Rewriting(#[from] RewritingError),
}

fn classify_reqwest_error(e: &reqwest::Error) -> bool {
    e.is_timeout()
        || e.is_connect()
        || e.is_request()
        || e.status()
            .map(|status| {
                status.is_server_error()
                    || !status.is_client_error()
                    || status == StatusCode::REQUEST_TIMEOUT
                    || status == StatusCode::TOO_MANY_REQUESTS
            })
            .unwrap_or_default()
}

impl ShouldRetry for Error {
    fn should_retry(&self, attempts: u32) -> bool {
        let retry = match self {
            Self::ReqwestMiddleware(reqwest_middleware::Error::Reqwest(e)) | Self::Reqwest(e) => {
                classify_reqwest_error(e)
            }
            Self::IO(_) | Self::StreamStuck => true,
            _ => false,
        };
        warn!(attempts, retry, e=?self, "ShouldRetry: Error occurred");
        retry
    }
}

impl<T> From<mpsc::error::SendError<T>> for Error {
    fn from(_value: mpsc::error::SendError<T>) -> Self {
        Self::Sender
    }
}
