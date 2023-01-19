use std::iter;

use base64::prelude::BASE64_STANDARD;
use base64::Engine;
use reqwest::header::{HeaderName, HeaderValue};
use reqwest::ClientBuilder;
use reqwest_middleware::{ClientBuilder as ClientBuilderWithMiddleware, ClientWithMiddleware};
use reqwest_retry::policies::ExponentialBackoffBuilder;
use reqwest_retry::RetryTransientMiddleware;
use rsa::pkcs1::EncodeRsaPublicKey;
use rsa::{Pkcs1v15Encrypt, RsaPrivateKey, RsaPublicKey};
use serde::Deserialize;
use uuid::Uuid;

use crate::error::Result;
use crate::middleware::{MaxConnMiddleware, RetryMiddleware};

const MAX_CONN: usize = 8;

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

#[must_use]
pub fn oauth_url(app_id: &Uuid, key: &RsaPublicKey) -> String {
    let query = &[
        ("application_name", "Shuiyuan Archiver"),
        ("client_id", &generate_client_id(app_id)),
        ("scopes", "session_info,read"),
        ("nonce", "1"),
        ("public_key", &key.to_pkcs1_pem(Default::default()).unwrap()),
    ];
    let parsed_query = serde_urlencoded::to_string(query).expect("failed to encode query");
    format!("https://shuiyuan.sjtu.edu.cn/user-api-key/new?{parsed_query}")
}

pub fn token_from_payload(payload: &str, key: &RsaPrivateKey) -> Result<String> {
    let ciphertext = BASE64_STANDARD.decode(payload.replace(' ', "").trim())?;

    let decrypted = key.decrypt(Pkcs1v15Encrypt, &ciphertext)?;

    Ok(serde_json::from_slice::<Payload>(&decrypted)?.key)
}

pub async fn create_client_with_token(
    token: &str,
    rate_limit_callback: impl 'static + Fn(u64) + Send + Sync,
) -> Result<ClientWithMiddleware> {
    let client = ClientBuilder::new()
        .default_headers(
            iter::once((
                HeaderName::from_static("user-api-key"),
                HeaderValue::from_str(token)?,
            ))
            .collect(),
        )
        .build()?;

    let client = ClientBuilderWithMiddleware::new(client)
        .with(RetryTransientMiddleware::new_with_policy(
            ExponentialBackoffBuilder::default().build_with_max_retries(3),
        ))
        .with(RetryMiddleware::new(rate_limit_callback))
        .with(MaxConnMiddleware::new(MAX_CONN))
        .build();

    client
        .get("https://shuiyuan.sjtu.edu.cn/session/current.json")
        .send()
        .await?
        .error_for_status()?;
    Ok(client)
}
