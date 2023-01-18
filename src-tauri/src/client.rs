use reqwest::header::{HeaderName, HeaderValue};
use reqwest::ClientBuilder;
use reqwest_middleware::{ClientBuilder as ClientBuilderWithMiddleware, ClientWithMiddleware};
use reqwest_retry::policies::ExponentialBackoffBuilder;
use reqwest_retry::RetryTransientMiddleware;
use rsa::{PaddingScheme, RsaPrivateKey};
use serde::Deserialize;

use crate::middleware::{MaxConnMiddleware, RetryMiddleware};
use crate::RateLimitWatcher;
use crate::Result;

const MAX_CONN: usize = 8;

#[derive(Debug, Deserialize)]
struct Payload {
    key: String,
}

pub fn decrypt_payload(payload: &str, key: &RsaPrivateKey) -> Result<String> {
    let ciphertext = base64::decode(payload.replace(' ', "").trim())?;

    let decrypted = key.decrypt(PaddingScheme::PKCS1v15Encrypt, &ciphertext)?;

    Ok(serde_json::from_slice::<Payload>(&decrypted)?.key)
}

pub async fn create_client_with_token(
    token: &str,
    rate_limit_watcher: RateLimitWatcher,
) -> Result<ClientWithMiddleware> {
    let client = ClientBuilder::new()
        .default_headers(
            [(
                HeaderName::from_static("user-api-key"),
                HeaderValue::from_str(token)?,
            )]
            .into_iter()
            .collect(),
        )
        .build()?;

    let client = ClientBuilderWithMiddleware::new(client)
        .with(RetryMiddleware::new(rate_limit_watcher))
        .with(RetryTransientMiddleware::new_with_policy(
            ExponentialBackoffBuilder::default().build_with_max_retries(3),
        ))
        .with(MaxConnMiddleware::new(MAX_CONN))
        .build();

    client
        .get("https://shuiyuan.sjtu.edu.cn/session/current.json")
        .send()
        .await?
        .error_for_status()?;
    Ok(client)
}
