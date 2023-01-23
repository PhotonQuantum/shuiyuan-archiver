use std::collections::hash_map::Entry;
use std::collections::{HashMap, HashSet};
use std::fs;
use std::fs::File;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use reqwest::header::CONTENT_TYPE;
use scopeguard::ScopeGuard;
use tap::{Pipe, TapFallible, TapOptional};
use tokio::sync::mpsc::Sender;
use tokio::sync::Semaphore;
use tracing::{error, warn};

use crate::archiver::DownloadEvent;
use crate::client::{Client, IntoRequestBuilderWrapped, RequestBuilderExt, ResponseExt};
use crate::error;
use crate::shared_promise::{shared_promise_pair, SharedPromise};

const OPEN_FILES_LIMIT: usize = 128;

pub struct DownloadManager {
    client: Client,
    downloaded_assets: Mutex<HashSet<String>>,
    downloaded_avatars: Mutex<HashMap<String, SharedPromise<PathBuf>>>,
    save_to: PathBuf,
    reporter: Sender<DownloadEvent>,
    open_files_sem: Arc<Semaphore>,
}

impl DownloadManager {
    pub fn new(client: Client, save_to: PathBuf, reporter: Sender<DownloadEvent>) -> Self {
        Self {
            client,
            save_to,
            downloaded_assets: Mutex::new(HashSet::new()),
            downloaded_avatars: Mutex::new(HashMap::new()),
            reporter,
            open_files_sem: Arc::new(Semaphore::new(OPEN_FILES_LIMIT)),
        }
    }
}

impl DownloadManager {
    pub async fn download_asset(
        &self,
        from: String,
        filename: &str,
        bypass_limit: bool,
    ) -> error::Result<()> {
        if !self.downloaded_assets.lock().unwrap().insert(from.clone()) {
            return Ok(());
        }

        self.reporter.send(DownloadEvent::ResourceTotalInc).await?;

        let save_path = self.save_to.join("resources").join(filename);

        let req = self
            .client
            .get(format!("https://shuiyuan.sjtu.edu.cn{from}"))
            .into_request_builder_wrapped()
            .pipe(|req| {
                if bypass_limit {
                    req.bypass_max_conn().bypass_throttle()
                } else {
                    req
                }
            });
        self.client
            .with(req, move |req| {
                let save_path = save_path.clone();
                let open_files_sem = self.open_files_sem.clone();
                async move {
                    let resp = req.send().await?;

                    let delete_guard = scopeguard::guard((), |_| {
                        drop(fs::remove_file(&save_path).tap_err(|e| {
                            warn!(?save_path, ?e, "Failed to remove file on error");
                        }));
                    });
                    let _guard = open_files_sem.acquire().await.expect("semaphore closed");
                    let mut file = File::create(&save_path)
                        .tap_err(|e| error!(?save_path, ?e, "[download_asset] file_create"))?;

                    resp.bytes_to_writer(&mut file)
                        .await
                        .tap_err(|e| error!(?save_path, ?e, "[download_asset] file_write"))?;
                    ScopeGuard::into_inner(delete_guard); // defuse
                    Ok(())
                }
            })
            .await?;

        self.reporter
            .send(DownloadEvent::ResourceDownloadedInc)
            .await?;
        Ok(())
    }
    pub async fn download_avatar(&self, from: String, filename: &str) -> error::Result<PathBuf> {
        let filename = PathBuf::from(filename);
        let relative_path = PathBuf::from("resources").join(&filename);
        let save_path = self.save_to.join(&relative_path);

        #[allow(clippy::significant_drop_in_scrutinee)]
        let swear_or_promise = match self.downloaded_avatars.lock().unwrap().entry(from.clone()) {
            Entry::Occupied(e) => Err(e.get().clone()),
            Entry::Vacant(e) => {
                let (swear, promise) = shared_promise_pair();
                e.insert(promise);
                Ok(swear)
            }
        };

        match swear_or_promise {
            Ok(swear) => {
                self.reporter.send(DownloadEvent::ResourceTotalInc).await?;

                let url = format!("https://shuiyuan.sjtu.edu.cn{from}");
                let req = self.client.get(url);
                self.client
                    .with(req, move |req| {
                        let mut save_path = save_path.clone();
                        let mut filename = filename.clone();
                        let open_files_sem = self.open_files_sem.clone();
                        async move {
                            let resp = req.send().await?;
                            let content_type = resp.headers().get(CONTENT_TYPE).unwrap().clone();

                            if content_type.to_str().unwrap().contains("svg") {
                                save_path.set_extension("svg");
                                filename.set_extension("svg");
                            }

                            let _guard = open_files_sem.acquire().await.expect("semaphore closed");
                            let delete_guard = scopeguard::guard((), |_| {
                                drop(fs::remove_file(&save_path).tap_err(|e| {
                                    warn!(
                                        "Failed to remove file on error ({}): {:?}",
                                        save_path.display(),
                                        e
                                    );
                                }));
                            });
                            let mut file = File::create(&save_path).tap_err(|e| {
                                error!(
                                    "[download_asset] file_create({}): {:?}",
                                    save_path.display(),
                                    e
                                );
                            })?;

                            resp.bytes_to_writer(&mut file)
                                .await
                                .tap_err(|e| error!("[download_asset] file_write: {:?}", e))?;
                            ScopeGuard::into_inner(delete_guard); // defuse
                            Ok(())
                        }
                    })
                    .await?;

                swear.fulfill(relative_path.clone());

                self.reporter
                    .send(DownloadEvent::ResourceDownloadedInc)
                    .await?;
                Ok(relative_path)
            }
            Err(promise) => Ok(
                promise
                    .recv()
                    .await
                    .tap_none(|| {
                        warn!("Promise not fulfilled which indicates an error in another task.");
                    })
                    .unwrap_or_default(), // error in another task will be collected so what is returned here doesn't matter
            ),
        }
    }
}
