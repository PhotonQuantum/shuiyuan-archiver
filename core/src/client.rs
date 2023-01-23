use std::fmt::Debug;
use std::future::Future;
use std::ops::Deref;
use std::sync::Arc;
use std::time::Duration;
use std::{io, iter};

use base64::prelude::BASE64_STANDARD;
use base64::Engine;
use futures::StreamExt;
use futures_retry_policies::retry_policies::RetryPolicies;
use leaky_bucket::RateLimiter;
use reqwest::header::{HeaderName, HeaderValue};
use reqwest::{ClientBuilder, Response};
use reqwest_middleware::{
    ClientBuilder as ClientBuilderWithMiddleware, ClientWithMiddleware, RequestBuilder,
};
use reqwest_retry::policies::ExponentialBackoff;
use rsa::pkcs1::EncodeRsaPublicKey;
use rsa::{Pkcs1v15Encrypt, RsaPrivateKey, RsaPublicKey};
use serde::de::DeserializeOwned;
use serde::Deserialize;
use tokio::sync::Semaphore;
use uuid::Uuid;

use crate::error::{Error, Result};
use crate::middleware::{BypassThrottle, RetryMiddleware};

pub const MAX_CONN: usize = 4;
pub const LOOSE_MAX_CONN: usize = 64;
pub const MAX_THROTTLE_WEIGHT: usize = 4;

const DEFAULT_BACKOFF: ExponentialBackoff = ExponentialBackoff {
    max_n_retries: 3,
    min_retry_interval: Duration::from_secs(1),
    max_retry_interval: Duration::from_secs(30 * 60),
    backoff_exponent: 3,
};

#[derive(Debug, Deserialize)]
struct Payload {
    key: String,
}

fn generate_client_id(app_id: &Uuid) -> String {
    let mac = mac_address::get_mac_address()
        .unwrap()
        .expect("No mac address found");
    let client_id = Uuid::new_v5(app_id, &mac.bytes());
    client_id.to_string()
}

/// Generate the OAuth URL from given app ID and public key.
#[must_use]
pub fn oauth_url(app_id: &Uuid, key: &RsaPublicKey) -> String {
    let query = &[
        ("application_name", "Shuiyuan Archiver"),
        ("client_id", &generate_client_id(app_id)),
        ("scopes", "session_info,read"),
        ("nonce", "1"),
        (
            "public_key",
            &key.to_pkcs1_pem(Default::default())
                .expect("failed to encode key"),
        ),
    ];
    let parsed_query = serde_urlencoded::to_string(query).expect("failed to encode query");
    format!("https://shuiyuan.sjtu.edu.cn/user-api-key/new?{parsed_query}")
}

/// Unpack the OAuth token from the given payload.
///
/// # Errors
///
/// This function will return an error if the payload is invalid.
pub fn token_from_payload(payload: &str, key: &RsaPrivateKey) -> Result<String> {
    let ciphertext = BASE64_STANDARD.decode(payload.replace(' ', "").trim())?;

    let decrypted = key.decrypt(Pkcs1v15Encrypt, &ciphertext)?;

    Ok(serde_json::from_slice::<Payload>(&decrypted)?.key)
}

pub struct RequestBuilderWrapped {
    req: RequestBuilder,
    sem_weight: u32,
    throttle_weight: usize,
    bypass_max_conn: bool,
    bypass_throttle: bool,
}

pub trait RequestBuilderExt {
    fn with_conn_weight(self, weight: u32) -> RequestBuilderWrapped;
    fn with_throttle_weight(self, weight: usize) -> RequestBuilderWrapped;
    fn bypass_max_conn(self) -> RequestBuilderWrapped;
    fn bypass_throttle(self) -> RequestBuilderWrapped;
}

impl RequestBuilderExt for RequestBuilderWrapped {
    fn with_conn_weight(self, weight: u32) -> RequestBuilderWrapped {
        Self {
            sem_weight: weight,
            ..self
        }
    }

    fn with_throttle_weight(self, weight: usize) -> RequestBuilderWrapped {
        Self {
            throttle_weight: weight,
            ..self
        }
    }

    fn bypass_max_conn(self) -> RequestBuilderWrapped {
        Self {
            bypass_max_conn: true,
            ..self
        }
    }

    fn bypass_throttle(self) -> RequestBuilderWrapped {
        Self {
            bypass_throttle: true,
            ..self
        }
    }
}

impl RequestBuilderExt for RequestBuilder {
    fn with_conn_weight(self, weight: u32) -> RequestBuilderWrapped {
        RequestBuilderWrapped {
            sem_weight: weight,
            ..self.into_request_builder_wrapped()
        }
    }

    fn with_throttle_weight(self, weight: usize) -> RequestBuilderWrapped {
        RequestBuilderWrapped {
            throttle_weight: weight,
            ..self.into_request_builder_wrapped()
        }
    }

    fn bypass_max_conn(self) -> RequestBuilderWrapped {
        RequestBuilderWrapped {
            bypass_max_conn: true,
            ..self.into_request_builder_wrapped()
        }
    }

    fn bypass_throttle(self) -> RequestBuilderWrapped {
        RequestBuilderWrapped {
            bypass_throttle: true,
            ..self.into_request_builder_wrapped()
        }
    }
}

pub trait IntoRequestBuilderWrapped: 'static + Send + Sync {
    fn into_request_builder_wrapped(self) -> RequestBuilderWrapped;
}

impl IntoRequestBuilderWrapped for RequestBuilderWrapped {
    fn into_request_builder_wrapped(self) -> RequestBuilderWrapped {
        self
    }
}

impl IntoRequestBuilderWrapped for RequestBuilder {
    fn into_request_builder_wrapped(self) -> RequestBuilderWrapped {
        RequestBuilderWrapped {
            req: self,
            sem_weight: 1,
            throttle_weight: 1,
            bypass_max_conn: false,
            bypass_throttle: false,
        }
    }
}

#[derive(Clone)]
pub struct Client {
    client: ClientWithMiddleware,
    loose_sem: Arc<Semaphore>,
    sem: Arc<Semaphore>,
    bucket: Arc<RateLimiter>,
}

impl Deref for Client {
    type Target = ClientWithMiddleware;

    fn deref(&self) -> &Self::Target {
        &self.client
    }
}

impl Client {
    /// Send a request and return the json response.
    ///
    /// This method applies rate limiting and connection limiting, and retries on failure.
    ///
    /// # Errors
    ///
    /// Returns an error if the request failed after retrying.
    pub async fn send_json<T: DeserializeOwned + Send>(
        &self,
        req: impl IntoRequestBuilderWrapped,
    ) -> Result<T> {
        self.with(req, |req| async move {
            Ok(req
                .timeout(Duration::from_secs(10))
                .send()
                .await?
                .json()
                .await?)
        })
        .await
    }
    /// Execute given function with given request.
    ///
    /// This method applies rate limiting and connection limiting, and retries on failure.
    ///
    /// # Errors
    ///
    /// Returns an error if the request failed after retrying.
    pub async fn with<F, Fut, T>(&self, req: impl IntoRequestBuilderWrapped, f: F) -> Result<T>
    where
        F: Fn(RequestBuilder) -> Fut + Clone + Send + Sync,
        Fut: Future<Output = Result<T>> + Send,
    {
        let RequestBuilderWrapped {
            req,
            sem_weight,
            throttle_weight,
            bypass_max_conn,
            bypass_throttle,
        } = req.into_request_builder_wrapped();
        futures_retry_policies::retry(
            RetryPolicies::new(DEFAULT_BACKOFF),
            tokio::time::sleep,
            move || {
                let sem = if bypass_max_conn {
                    self.loose_sem.clone()
                } else {
                    self.sem.clone()
                };
                let bucket = self.bucket.clone();
                let req = req
                    .try_clone()
                    .expect("clone request")
                    .with_extension(BypassThrottle(bypass_throttle));
                let f = f.clone();
                async move {
                    let _guard = sem
                        .acquire_many(sem_weight)
                        .await
                        .expect("acquire semaphore");
                    if !bypass_throttle {
                        bucket.acquire(throttle_weight).await;
                    }
                    f(req).await
                }
            },
        )
        .await
    }
}

/// Create a client with given token.
///
/// # Errors
///
/// Errors if an http client can't be created, or the token is illegal.
pub async fn create_client_with_token(
    token: &str,
    rate_limit_callback: impl 'static + Fn(u64) + Send + Sync,
) -> Result<Client> {
    let client = ClientBuilder::new()
        .connect_timeout(Duration::from_secs(10))
        .default_headers(
            iter::once((
                HeaderName::from_static("user-api-key"),
                HeaderValue::from_str(token).expect("illegal token"),
            ))
            .collect(),
        )
        .build()?;

    let client = ClientBuilderWithMiddleware::new(client)
        .with(RetryMiddleware::new(rate_limit_callback))
        .build();

    client
        .get("https://shuiyuan.sjtu.edu.cn/session/current.json")
        .send()
        .await?
        .error_for_status()?;
    Ok(Client {
        client,
        loose_sem: Arc::new(Semaphore::new(LOOSE_MAX_CONN)),
        sem: Arc::new(Semaphore::new(MAX_CONN)),
        bucket: Arc::new(
            RateLimiter::builder()
                .interval(Duration::from_millis(200))
                .max(MAX_THROTTLE_WEIGHT)
                .build(),
        ),
    })
}

#[async_trait::async_trait]
pub trait ResponseExt {
    async fn bytes_to_writer(self, writer: impl io::Write + Send + Sync) -> Result<()>;
}

#[async_trait::async_trait]
impl ResponseExt for Response {
    async fn bytes_to_writer(self, mut writer: impl io::Write + Send + Sync) -> Result<()> {
        let mut stream = self.bytes_stream();
        loop {
            break match tokio::time::timeout(Duration::from_secs(10), stream.next()).await {
                Ok(Some(Ok(bytes))) => {
                    writer.write_all(&bytes)?;
                    continue;
                }
                Ok(Some(Err(e))) => Err(e.into()),
                Ok(None) => Ok(()),
                Err(_) => Err(Error::StreamStuck),
            };
        }
    }
}
