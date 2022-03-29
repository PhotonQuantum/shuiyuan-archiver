#![windows_subsystem = "windows"]
#![allow(clippy::module_name_repetitions)]

use std::path::{Path, PathBuf};
use std::sync::Arc;

use chrono::Local;
use eyre::Result;
use once_cell::sync::Lazy;
use parking_lot::Mutex;
use regex::Regex;
use reqwest_middleware::ClientWithMiddleware;
use rfd::AsyncFileDialog;
use rsa::pkcs1::ToRsaPublicKey;
use rsa::{RsaPrivateKey, RsaPublicKey};
use slint::Weak;
use tokio::runtime::{Handle, Runtime};
use tokio::sync::Mutex as AsyncMutex;

use crate::archiver::Archiver;
use crate::client::{create_client, create_client_with_token};
use crate::rate_limit::RateLimitWatcher;
use crate::store::Store;

mod archiver;
mod client;
mod future_queue;
mod middleware;
mod models;
mod rate_limit;
mod store;
mod url_scheme;

slint::include_modules!();

#[derive(Clone)]
struct State {
    rt: Handle,
    client: Arc<Mutex<Option<ClientWithMiddleware>>>,
    ui: Weak<MainWindow>,
    key: RsaPrivateKey,
    store: Arc<Store>,
    rate_limit_watcher: RateLimitWatcher,
}

fn browser_auth(key: &RsaPublicKey) -> Result<()> {
    let query = &[
        ("application_name", "Shuiyuan Archiver"),
        ("client_id", "fbLF9rqADqQ%3AAPA91bGewJA-kSC7OEZVFoEWGdNwhVvQEu4BwKuqR53gvFRN9kxAHX5cv7Q7KZDPtJQ9WgK8QbfVRFZtRrG5oOudbPpV7gBMtQON0C-Fz8djlFCoXARE25DvDxfnlQ4HuZjOOeD2qdyb"),
        ("scopes", "session_info,read"),
        ("nonce", "1"),
        ("public_key", &key.to_pkcs1_pem().unwrap()),
    ];
    let parsed_query = serde_urlencoded::to_string(query)?;
    let url = format!(
        "https://shuiyuan.sjtu.edu.cn/user-api-key/new?{}",
        parsed_query
    );
    Ok(webbrowser::open(&url)?)
}

fn main() -> Result<()> {
    let rt = Runtime::new()?;
    let store = Store::new()?;

    let ui = MainWindow::new();
    let state = State {
        rt: rt.handle().clone(),
        client: Arc::new(Mutex::new(None)),
        ui: ui.as_weak(),
        key: RsaPrivateKey::new(&mut rand::thread_rng(), 2048)?,
        store: Arc::new(store),
        rate_limit_watcher: RateLimitWatcher::new(ui.as_weak()),
    };

    // check if we have a cached key and whether it's still valid
    let state_ = state.clone();
    rt.spawn(async move {
        state_.ui.upgrade_in_event_loop(|ui| {
            ui.set_login_disabled(true);
        });
        if let Some(cached_token) = state_.store.get_token() {
            if let Ok(client) =
                create_client_with_token(&cached_token, state_.rate_limit_watcher).await
            {
                *state_.client.lock() = Some(client);
                state_.ui.upgrade_in_event_loop(|ui| {
                    ui.set_state("fetch".into());
                });
                return;
            }
        }

        // no cached token, or it's expired
        state_.store.delete_token();
        browser_auth(&state_.key.to_public_key()).expect("No browser found");
        state_.ui.upgrade_in_event_loop(|ui| {
            ui.set_login_disabled(false);
        });
    });

    let state_ = state.clone();
    ui.on_login_cb(move || {
        let state = state_.clone();
        let payload = state.ui.unwrap().get_login_token().to_string();
        state.rt.clone().spawn(async move {
            let state = state.clone();
            state
                .ui
                .upgrade_in_event_loop(|handle| handle.set_login_disabled(true));

            let new_client = create_client(&payload, &state.key, state.rate_limit_watcher).await;
            match new_client {
                Ok((token, new_client)) => {
                    *state.client.lock() = Some(new_client);
                    let _ = state.store.set_token(&token);
                    state.ui.upgrade_in_event_loop(|handle| {
                        handle.set_state("fetch".into());
                    });
                }
                Err(e) => {
                    state.ui.upgrade_in_event_loop(move |handle| {
                        println!("{:?}", e);
                        handle.set_login_disabled(false);
                        handle.set_login_error(format!("登录失败\n{}", e).into());
                    });
                }
            }
        });
    });

    ui.on_parse_cb(|url| {
        static RE: Lazy<Regex> =
            Lazy::new(|| Regex::new(r#"https://shuiyuan.sjtu.edu.cn/t/topic/(\d+)"#).unwrap());
        let topic_str = RE
            .captures(&url)
            .and_then(|c| Some(c.get(1)?.as_str()))
            .unwrap_or_default();
        topic_str.parse().unwrap_or_default()
    });

    let state_ = state.clone();
    ui.on_browse_cb(move || {
        let state = state_.clone();
        state.rt.clone().spawn(async move {
            let path = AsyncFileDialog::new().pick_folder().await;
            if let Some(path) = path {
                state.ui.upgrade_in_event_loop(move |ui| {
                    ui.set_fetch_output(path.path().to_string_lossy().to_string().into())
                });
            }
        });
    });

    ui.on_fetch_cb(move |topic, output, anonymous| {
        let state = state.clone();
        let client = state.client.lock().as_ref().unwrap().clone();
        let output = PathBuf::from(output.to_string());
        state.rt.clone().spawn(async move {
            let state = state.clone();
            state
                .ui
                .upgrade_in_event_loop(|handle| handle.set_fetch_disabled(true));
            let locked_ui = AsyncMutex::new(state.ui.clone());
            let output = find_available_path(&*output, topic);
            let res = Archiver::new(client, topic as usize, output, anonymous, locked_ui)
                .download()
                .await;
            state.ui.upgrade_in_event_loop(move |handle| {
                handle.set_fetch_disabled(false);
                if let Err(e) = res {
                    handle.set_fetch_msg(format!("下载错误：{}", e).into());
                } else {
                    handle.set_fetch_msg("下载完成".into());
                }
            });
        });
    });

    ui.run();
    Ok(())
}

fn find_available_path(path: &Path, topic: i32) -> PathBuf {
    let new_path = path.join(format!("水源存档_{}_{}", topic, get_current_time()));
    new_path
}

fn get_current_time() -> String {
    Local::now().format("%Y-%m-%d_%H-%M-%S").to_string()
}
