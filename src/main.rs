#![windows_subsystem = "windows"]
#![allow(clippy::module_name_repetitions)]

use std::path::{Path, PathBuf};
use std::sync::Arc;

use eyre::Result;
use once_cell::sync::Lazy;
use parking_lot::Mutex;
use regex::Regex;
use rfd::FileDialog;
use slint::Weak;
use tokio::runtime::{Handle, Runtime};
use tokio::sync::Mutex as AsyncMutex;

use crate::archiver::download;
use crate::client::SJTUClient;

mod archiver;
mod client;

slint::include_modules!();

#[derive(Clone)]
struct State {
    rt: Handle,
    client: Arc<Mutex<Option<SJTUClient>>>,
    ui: Weak<MainWindow>,
}

fn main() -> Result<()> {
    let rt = Runtime::new()?;
    let client: Arc<Mutex<Option<SJTUClient>>> = Arc::new(Mutex::new(None));

    let ui = MainWindow::new();
    let state = State {
        rt: rt.handle().clone(),
        client,
        ui: ui.as_weak(),
    };

    let state_ = state.clone();
    ui.on_login_cb(move || {
        let state = state_.clone();
        let user = state.ui.unwrap().get_username().to_string();
        let pass = state.ui.unwrap().get_password().to_string();
        state.rt.clone().spawn(async move {
            let state = state.clone();
            state
                .ui
                .upgrade_in_event_loop(|handle| handle.set_login_disabled(true));

            let new_client = SJTUClient::new(&user, &pass).await;
            match new_client {
                Ok(new_client) => {
                    *state.client.lock() = Some(new_client);
                    state.ui.upgrade_in_event_loop(|handle| {
                        handle.set_state("fetch".into());
                    });
                }
                Err(e) => {
                    state.ui.upgrade_in_event_loop(move |handle| {
                        println!("{:?}", e);
                        handle.set_login_disabled(false);
                        handle.set_login_error(
                            format!("用户名密码错误或验证码识别失败，可多试几次\n{}", e).into(),
                        );
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

    ui.on_browse_cb(|old| {
        let path = FileDialog::new().pick_folder();
        if let Some(path) = path {
            path.to_string_lossy().to_string().into()
        } else {
            old
        }
    });

    ui.on_fetch_cb(move |topic| {
        let state = state.clone();
        let client = state.client.lock().as_ref().unwrap().clone();
        let output = PathBuf::from(state.ui.unwrap().get_fetch_output().to_string());
        state.rt.clone().spawn(async move {
            let state = state.clone();
            state
                .ui
                .upgrade_in_event_loop(|handle| handle.set_fetch_disabled(true));
            let locked_ui = AsyncMutex::new(state.ui.clone());
            let output = find_available_path(&*output);
            let res = download(&*client, topic as u64, &*output, locked_ui).await;
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

fn find_available_path(path: &Path) -> PathBuf {
    if !path.exists() {
        return path.to_path_buf();
    }

    let files = path.read_dir().map(|iter| iter.count()).unwrap_or(1);
    if files == 0 {
        return path.to_path_buf();
    }

    let base = path.join("水源存档");
    if base.exists() {
        for i in 1..100 {
            let new_path = path.join(format!("水源存档 ({})", i));
            if !new_path.exists() {
                return new_path;
            }
        }
    }

    base
}
