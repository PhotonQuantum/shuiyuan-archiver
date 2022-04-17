use std::cmp::max;
use std::sync::Arc;
use std::sync::Mutex;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use tauri::{Window, Wry};
use tokio::sync::oneshot::{channel, Sender};
use tokio::time::interval;

#[derive(Clone)]
pub struct RateLimitWatcher {
    wait_until: Arc<Mutex<Option<SystemTime>>>,
    _stop_tx: Arc<Sender<()>>,
}

impl RateLimitWatcher {
    pub fn new(window: Window<Wry>) -> Self {
        let wait_until: Arc<Mutex<Option<SystemTime>>> = Arc::new(Mutex::new(None));
        let (stop_tx, mut stop_rx) = channel();
        {
            let wait_until = wait_until.clone();
            tauri::async_runtime::spawn(async move {
                let mut interval = interval(Duration::from_millis(500));

                loop {
                    tokio::select! {
                        _ = interval.tick() => {
                            if let Some(wait_until) = wait_until.lock().unwrap().as_ref() {
                                window.emit("rate-limit-event",
                                    wait_until
                                        .duration_since(SystemTime::now())
                                        .unwrap_or_default()
                                        .as_secs() as i32,
                                    ).expect("failed to emit rateLimit event");
                            } else {
                                window.emit("rate-limit-event", 0).expect("failed to emit rateLimit event");
                            }
                        }
                        _ = &mut stop_rx => {
                            break;
                        }
                    }
                }
            });
        }
        Self {
            wait_until,
            _stop_tx: Arc::new(stop_tx),
        }
    }
    pub fn register_limit(&self, delay: u64) {
        let mut wait_until = self.wait_until.lock().unwrap();
        let new_until = SystemTime::now() + Duration::from_secs(delay);
        *wait_until = Some(max(wait_until.unwrap_or(UNIX_EPOCH), new_until));
    }
}
