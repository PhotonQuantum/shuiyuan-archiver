#![cfg_attr(
    all(not(debug_assertions), target_os = "windows"),
    windows_subsystem = "windows"
)]
#![allow(clippy::module_name_repetitions)]

use std::error::Error as StdError;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::str::FromStr;
use std::sync::Mutex;

use chrono::Local;
use reqwest::Client;
use rsa::pkcs1::ToRsaPublicKey;
use rsa::RsaPrivateKey;
use tauri::{AboutMetadata, Menu, MenuItem, Submenu, Wry};
use uuid::Uuid;

use crate::archiver::Archiver;
use crate::client::{create_client_with_token, decrypt_payload};
use crate::rate_limit::RateLimitWatcher;
use crate::store::Store;

mod archiver;
mod client;
mod error;
mod future_queue;
mod middleware;
mod models;
mod rate_limit;
mod store;
mod url_scheme;

type Result<T, E = Box<dyn StdError + Send + Sync>> = std::result::Result<T, E>;

#[tauri::command]
fn open_browser(key: tauri::State<RsaPrivateKey>) {
    let query = &[
        ("application_name", "Shuiyuan Archiver"),
        ("client_id", &generate_client_id()),
        ("scopes", "session_info,read"),
        ("nonce", "1"),
        ("public_key", &key.to_public_key().to_pkcs1_pem().unwrap()),
    ];
    let parsed_query = serde_urlencoded::to_string(query).expect("failed to encode query");
    let url = format!(
        "https://shuiyuan.sjtu.edu.cn/user-api-key/new?{}",
        parsed_query
    );
    webbrowser::open(&url).expect("no browser");
}

#[tauri::command]
fn token_from_oauth(oauth_key: String, key: tauri::State<RsaPrivateKey>) -> String {
    decrypt_payload(&oauth_key, &key).unwrap_or_default()
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
    let client = Client::new();
    let resp = client
        .get("https://shuiyuan.sjtu.edu.cn/session/current.json")
        .header("user-api-key", dbg!(token))
        .send()
        .await;
    match resp {
        Ok(resp) => dbg!(resp.status()).is_success(),
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
    let watcher = RateLimitWatcher::new(window.clone());
    match create_client_with_token(&token, watcher).await {
        Ok(client) => {
            let path = find_available_path(&*PathBuf::from(save_at), topic as i32);
            *saved_folder.lock().unwrap() = Some(path.clone());
            let archiver = Archiver::new(client, topic, path, mask_user, window);
            if let Err(e) = archiver.download().await {
                sentry::capture_error(&*e);
                return Err(e.to_string());
            }
            Ok(())
        }
        Err(e) => {
            sentry::capture_error(&*e);
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

    let store = Store::new().expect("failed to initialize store");
    let key = RsaPrivateKey::new(&mut rand::thread_rng(), 2048).unwrap();
    let saved_folder: Mutex<Option<PathBuf>> = Mutex::new(None);

    let builder = if cfg!(target_os = "macos") {
        let app_menu = Menu::new()
            .add_native_item(MenuItem::About(
                String::from("水源存档工具"),
                AboutMetadata::new(),
            ))
            .add_native_item(MenuItem::Copy)
            .add_native_item(MenuItem::Cut)
            .add_native_item(MenuItem::Paste)
            .add_native_item(MenuItem::SelectAll)
            .add_native_item(MenuItem::Undo)
            .add_native_item(MenuItem::Redo);
        let menu = Menu::new().add_submenu(Submenu::new("File", app_menu));
        tauri::Builder::default().menu(menu)
    } else {
        tauri::Builder::default()
    };
    builder
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

fn find_available_path(path: &Path, topic: i32) -> PathBuf {
    let new_path = path.join(format!("水源存档_{}_{}", topic, get_current_time()));
    new_path
}

fn get_current_time() -> String {
    Local::now().format("%Y-%m-%d_%H-%M-%S").to_string()
}

fn generate_client_id() -> String {
    let base_uuid = Uuid::from_str("1bf328bf-239b-46ed-9696-92fdcb51f2b1").unwrap();
    let mac = mac_address::get_mac_address()
        .unwrap()
        .expect("No mac address found");
    let client_id = Uuid::new_v5(&base_uuid, &mac.bytes());
    client_id.to_string()
}
