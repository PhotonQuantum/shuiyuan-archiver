use eyre::{Context, Result};
use reqwest::ClientBuilder;
use reqwest::header::{HeaderName, HeaderValue};
use reqwest_middleware::{ClientBuilder as ClientBuilderWithMiddleware, ClientWithMiddleware};
use reqwest_retry::policies::ExponentialBackoffBuilder;
use reqwest_retry::RetryTransientMiddleware;
use rsa::{PaddingScheme, RsaPrivateKey};
use serde::Deserialize;

use crate::middleware::RetryMiddleware;
use crate::RateLimitWatcher;

#[derive(Debug, Deserialize)]
struct Payload {
    key: String,
}

pub fn decrypt_payload(payload: &str, key: &RsaPrivateKey) -> Result<String> {
    let ciphertext = base64::decode(&payload.replace(' ', "").trim())?;

    let decrypted = key
        .decrypt(PaddingScheme::PKCS1v15Encrypt, &ciphertext)?;

    Ok(serde_json::from_slice::<Payload>(&decrypted)?.key)
}

pub async fn create_client(payload: &str, key: &RsaPrivateKey, rate_limit_watcher: RateLimitWatcher) -> Result<(String, ClientWithMiddleware)> {
    let token = decrypt_payload(payload, key).context("Failed to decrypt payload")?;
    Ok((token.clone(), create_client_with_token(&token, rate_limit_watcher).await?))
}

pub async fn create_client_with_token(token: &str, rate_limit_watcher: RateLimitWatcher) -> Result<ClientWithMiddleware> {
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
        // .with(RateLimitMiddleware::new(Quota::per_second(
        //     NonZeroU32::new(5).unwrap(),
        // )))
        .with(RetryMiddleware::new(rate_limit_watcher))
        .with(RetryTransientMiddleware::new_with_policy(
            ExponentialBackoffBuilder::default().build_with_max_retries(3),
        ))
        .build();

    client
        .get("https://shuiyuan.sjtu.edu.cn/session/current.json")
        .send()
        .await?
        .error_for_status()
        .wrap_err("invalid credential")?;
    Ok(client)
}