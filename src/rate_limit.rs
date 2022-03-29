use std::cmp::max;
use std::sync::Arc;
use std::sync::mpsc::{channel, RecvTimeoutError, Sender};
use std::thread;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use parking_lot::Mutex;
use slint::Weak;

use crate::MainWindow;

#[derive(Clone)]
pub struct RateLimitWatcher {
    wait_until: Arc<Mutex<Option<SystemTime>>>,
    _stop_tx: Arc<Mutex<Sender<()>>>,
}

impl RateLimitWatcher {
    pub fn new(ui: Weak<MainWindow>) -> Self {
        let wait_until: Arc<Mutex<Option<SystemTime>>> = Arc::new(Mutex::new(None));
        let (stop_tx, stop_rx) = channel();
        let wait_until_in_thread = wait_until.clone();
        thread::spawn(move || {
            while let Err(RecvTimeoutError::Timeout) = stop_rx.recv_timeout(Duration::from_millis(500)) {
                let wait_until_in_thread = wait_until_in_thread.clone();
                ui.upgrade_in_event_loop(move |ui| {
                    if let Some(wait_until) = wait_until_in_thread.lock().as_ref() {
                        ui.set_fetch_retry_after(wait_until.duration_since(SystemTime::now()).unwrap_or_default().as_secs() as i32);
                    } else {
                        ui.set_fetch_retry_after(0);
                    }
                });
            }
        });
        Self {
            wait_until,
            _stop_tx: Arc::new(Mutex::new(stop_tx)),
        }
    }
    pub fn register_limit(&self, delay: u64) {
        let mut wait_until = self.wait_until.lock();
        let new_until = SystemTime::now() + Duration::from_secs(delay);
        *wait_until = Some(max(wait_until.unwrap_or(UNIX_EPOCH), new_until));
    }
}