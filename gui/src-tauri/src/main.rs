#![cfg_attr(
    all(not(debug_assertions), target_os = "windows"),
    windows_subsystem = "windows"
)]
#![allow(clippy::module_name_repetitions)]

use std::error::Error as StdError;
use std::path::PathBuf;
use std::process::Command;
use std::str::FromStr;
use std::sync::Mutex;

use once_cell::sync::Lazy;
use serde_json::json;
use tauri::Wry;
use tokio::sync::mpsc::channel;
use tracing_subscriber::EnvFilter;

use sa_core::archiver;
use sa_core::archiver::DownloadEvent;
use sa_core::client::{create_client_with_token, oauth_url, token_from_payload};
use sa_core::re_exports::reqwest;
use sa_core::re_exports::rsa;
use sa_core::re_exports::uuid::Uuid;

use crate::store::Store;

mod store;
mod url_scheme;

type BoxedError = Box<dyn StdError + Send + Sync>;
type Result<T, E = BoxedError> = std::result::Result<T, E>;

const APP_ID: Lazy<Uuid> =
    Lazy::new(|| Uuid::from_str("1bf328bf-239b-46ed-9696-92fdcb51f2b1").unwrap());

#[tauri::command]
fn open_browser(key: tauri::State<rsa::RsaPrivateKey>) {
    webbrowser::open(&oauth_url(&APP_ID, &key)).expect("no browser");
}

#[tauri::command]
fn token_from_oauth(payload: String, key: tauri::State<rsa::RsaPrivateKey>) -> String {
    token_from_payload(&payload, &key).unwrap_or_default()
}

#[tauri::command]
fn set_token(token: String, state: tauri::State<Store>) {
    if let Err(e) = state.set_token(&token) {
        sentry::capture_error(&*e);
    }
}

#[tauri::command]
fn get_token(state: tauri::State<Store>) -> String {
    state.get_token().unwrap_or_default()
}

#[tauri::command]
fn del_token(state: tauri::State<Store>) {
    state.delete_token();
}

#[tauri::command]
async fn validate_token(token: String) -> bool {
    let client = reqwest::Client::new();
    let resp = client
        .get("https://shuiyuan.sjtu.edu.cn/session/current.json")
        .header("user-api-key", token)
        .send()
        .await;
    match resp {
        Ok(resp) => resp.status().is_success(),
        Err(_) => false,
    }
}

#[tauri::command]
async fn select_folder() -> String {
    tauri::api::dialog::blocking::FileDialogBuilder::default()
        .pick_folder()
        .unwrap_or_default()
        .to_string_lossy()
        .to_string()
}

#[tauri::command]
async fn archive(
    token: String,
    topic: usize,
    save_at: String,
    mask_user: bool,
    window: tauri::Window<Wry>,
    saved_folder: tauri::State<'_, Mutex<Option<PathBuf>>>,
) -> Result<(), String> {
    let rate_limit_callback = {
        let window = window.clone();
        move |t| {
            window.emit("rate-limit-event", t).unwrap();
        }
    };
    match create_client_with_token(&token, rate_limit_callback).await {
        Ok(client) => {
            let path = PathBuf::from(save_at);
            *saved_folder.lock().unwrap() = Some(path.clone());
            let (tx, mut rx) = channel(8);
            tokio::spawn(async move {
                while let Some(ev) = rx.recv().await {
                    match ev {
                        DownloadEvent::FetchingMeta => {
                            window
                                .emit("progress-event", json!({"kind": "fetch-meta"}))
                                .unwrap();
                        }
                        DownloadEvent::PostChunksTotal(n) => {
                            window
                                .emit(
                                    "progress-event",
                                    json!({"kind": "chunks-total", "value": n}),
                                )
                                .unwrap();
                        }
                        DownloadEvent::PostChunksDownloadedInc => {
                            window
                                .emit("progress-event", json!({"kind": "chunks-inc"}))
                                .unwrap();
                        }
                        DownloadEvent::ResourceTotalInc => {
                            window
                                .emit("progress-event", json!({"kind": "resources-total-inc"}))
                                .unwrap();
                        }
                        DownloadEvent::ResourceDownloadedInc => {
                            window
                                .emit("progress-event", json!({"kind": "resources-inc"}))
                                .unwrap();
                        }
                    }
                }
            });
            if let Err(e) = archiver::archive(&client, topic, &path, mask_user, tx).await {
                sentry::capture_error(&e);
                return Err(e.to_string());
            }
            Ok(())
        }
        Err(e) => {
            sentry::capture_error(&e);
            Err(e.to_string())
        }
    }
}

#[tauri::command]
fn open_saved_folder(saved_folder: tauri::State<Mutex<Option<PathBuf>>>) {
    if let Some(path) = saved_folder.inner().lock().unwrap().clone() {
        let command;
        #[cfg(target_os = "macos")]
        {
            command = "open";
        }
        #[cfg(target_os = "windows")]
        {
            command = "explorer";
        }
        #[cfg(target_os = "linux")]
        {
            command = "xdg-open";
        }

        drop(Command::new(command).arg(path).spawn());
    }
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

    let store = Store::new().expect("failed to initialize store");
    let key = rsa::RsaPrivateKey::new(&mut rand::thread_rng(), 2048).unwrap();
    let saved_folder: Mutex<Option<PathBuf>> = Mutex::new(None);

    tauri::Builder::default()
        .manage(store)
        .manage(key)
        .manage(saved_folder)
        .invoke_handler(tauri::generate_handler![
            set_token,
            get_token,
            del_token,
            validate_token,
            open_browser,
            token_from_oauth,
            select_folder,
            archive,
            open_saved_folder
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
