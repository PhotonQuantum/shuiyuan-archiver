#![cfg_attr(
    all(not(debug_assertions), target_os = "windows"),
    windows_subsystem = "windows"
)]
#![allow(clippy::module_name_repetitions)]

use std::error::Error as StdError;
use std::path::PathBuf;
use std::str::FromStr;
use std::sync::Mutex;

use once_cell::sync::Lazy;
use tap::Tap;
use tauri::async_runtime::channel;
use tauri::Wry;
use tracing_subscriber::EnvFilter;

use sa_core::archiver;
use sa_core::archiver::{fetch_topic_meta, TopicMeta};
use sa_core::client::{create_client_with_token, oauth_url, token_from_payload, Client};
use sa_core::re_exports::rsa;
use sa_core::re_exports::uuid::Uuid;

mod url_scheme;

type BoxedError = Box<dyn StdError + Send + Sync>;
type Result<T, E = BoxedError> = std::result::Result<T, E>;

static APP_ID: Lazy<Uuid> =
    Lazy::new(|| Uuid::from_str("1bf328bf-239b-46ed-9696-92fdcb51f2b1").unwrap());

#[tauri::command]
fn sanitize(s: String) -> String {
    sanitize_filename::sanitize(&s)
}

#[tauri::command]
fn open_browser(key: tauri::State<rsa::RsaPrivateKey>) {
    webbrowser::open(&oauth_url(&APP_ID, &key)).expect("no browser");
}

#[tauri::command]
fn token_from_oauth(payload: String, key: tauri::State<rsa::RsaPrivateKey>) -> String {
    token_from_payload(&payload, &key).unwrap_or_default()
}

#[tauri::command]
async fn login_with_token(
    token: String,
    client: tauri::State<'_, Mutex<Option<Client>>>,
    window: tauri::Window<Wry>,
) -> Result<(), String> {
    let rate_limit_callback = {
        let window = window.clone();
        move |t| {
            eprintln!("rate limit: {}", t);
            window.emit("rate-limit-event", t).unwrap();
        }
    };

    let new_client = create_client_with_token(&token, rate_limit_callback)
        .await
        .map_err(|e| {
            sentry::capture_error(&e);
            e.to_string()
        })?;
    *client.lock().unwrap() = Some(new_client);
    Ok(())
}

#[tauri::command]
async fn fetch_meta(
    topic_id: u32,
    client: tauri::State<'_, Mutex<Option<Client>>>,
) -> Result<TopicMeta, String> {
    eprintln!("fetch meta {}", topic_id);
    let client = client.lock().unwrap().clone().expect("client");
    fetch_topic_meta(&client, topic_id)
        .await
        .tap(|_| {
            eprintln!("fetch meta done");
        })
        .map_err(|e| {
            eprintln!("fetch meta error: {}", e);
            sentry::capture_error(&e);
            e.to_string()
        })
}

#[tauri::command]
async fn archive(
    topic_meta: TopicMeta,
    save_to: String,
    mask_user: bool,
    window: tauri::Window<Wry>,
    client: tauri::State<'_, Mutex<Option<Client>>>,
) -> Result<(), String> {
    let client = client.lock().unwrap().clone().expect("client");
    let path = PathBuf::from(save_to);
    let (tx, mut rx) = channel(8);
    tauri::async_runtime::spawn(async move {
        while let Some(ev) = rx.recv().await {
            window.emit("progress-event", ev).unwrap();
        }
    });
    if let Err(e) = archiver::archive(&client, topic_meta, &path, mask_user, tx).await {
        sentry::capture_error(&e);
        return Err(e.to_string());
    }
    Ok(())
}

fn main() {
    let _guard = option_env!("SENTRY_DSN").map(|dsn| {
        sentry::init((
            dsn,
            sentry::ClientOptions {
                release: sentry::release_name!(),
                ..Default::default()
            },
        ))
    });
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::from_default_env())
        .init();

    let key = rsa::RsaPrivateKey::new(&mut rand::thread_rng(), 2048).unwrap();
    let client: Mutex<Option<Client>> = Mutex::new(None);

    tauri::Builder::default()
        .plugin(tauri_plugin_store::Builder::default().build())
        .manage(key)
        .manage(client)
        .invoke_handler(tauri::generate_handler![
            sanitize,
            login_with_token,
            open_browser,
            token_from_oauth,
            fetch_meta,
            archive,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
