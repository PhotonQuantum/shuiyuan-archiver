use std::future::Future;
use std::path::Path;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use console::style;
use indicatif::{MultiProgress, ProgressBar, ProgressStyle};
use tokio::sync::mpsc;
use tokio::task::JoinHandle;

use sa_core::archiver;
use sa_core::archiver::DownloadEvent;
use sa_core::client::create_client_with_token;

#[derive(Debug)]
struct TimeoutInEffect {
    handler: JoinHandle<()>,
    progress_bar: ProgressBar,
}

fn rate_limit_callback(progress: MultiProgress) -> impl 'static + Fn(u64) + Send + Sync {
    let sty = ProgressStyle::with_template(
        "[{elapsed_precise:.yellow}] {bar:40.yellow/red} {pos:>7}/{len:7} {msg:.yellow}",
    )
    .unwrap()
    .progress_chars("##-");
    let timeout_progress: Arc<Mutex<Option<TimeoutInEffect>>> = Arc::new(Mutex::new(None));
    move |delay| {
        let mut timeout_progress = timeout_progress.lock().unwrap();
        if let Some(timeout) = &mut *timeout_progress {
            let remaining = timeout.progress_bar.length().expect("has length")
                - timeout.progress_bar.position();
            if delay < remaining {
                return; // No need to update the timeout.
            }

            // Need to remove the old progress bar.
            progress.remove(&timeout.progress_bar);
            timeout.handler.abort();
        }
        let timeout_bar = ProgressBar::new(delay)
            .with_style(sty.clone())
            .with_message("Rate limited...");
        progress.add(timeout_bar.clone());
        let handler = tokio::spawn({
            let timeout_bar = timeout_bar.clone();
            async move {
                while timeout_bar.position() < timeout_bar.length().expect("has length") {
                    timeout_bar.inc(1);
                    tokio::time::sleep(Duration::from_secs(1)).await;
                }
                timeout_bar.finish_and_clear();
            }
        });

        *timeout_progress = Some(TimeoutInEffect {
            handler,
            progress_bar: timeout_bar,
        });
    }
}

fn display_task(
    progress: MultiProgress,
    mut rx: mpsc::Receiver<DownloadEvent>,
) -> impl Future<Output = ()> {
    let sty = ProgressStyle::with_template(
        "[{elapsed_precise}] {bar:40.cyan/blue} {pos:>7}/{len:7} {msg}",
    )
    .unwrap()
    .progress_chars("##-");
    let (mut meta_spinner, mut post_progress, mut asset_progress) = (None, None, None);
    async move {
        while let Some(msg) = rx.recv().await {
            match msg {
                DownloadEvent::FetchingMeta => {
                    let spinner = ProgressBar::new_spinner().with_message("Fetching metadata...");
                    spinner.enable_steady_tick(Duration::from_millis(100));
                    meta_spinner = Some(spinner.clone());
                    progress.add(spinner);
                }
                DownloadEvent::PostChunksTotal(total) => {
                    let spinner = meta_spinner.take().unwrap();
                    spinner.finish_with_message("Fetching metadata... done");
                    let post_prog = ProgressBar::new(total as u64)
                        .with_style(sty.clone())
                        .with_message("Downloading posts...");
                    post_progress = Some(post_prog.clone());
                    progress.add(post_prog.clone());
                    post_prog.enable_steady_tick(Duration::from_millis(100));
                }
                DownloadEvent::PostChunksDownloadedInc => {
                    let post_prog = post_progress.as_ref().unwrap();
                    post_prog.inc(1);
                    if post_prog.position() == post_prog.length().unwrap() {
                        post_prog.finish_with_message("Downloading posts... done");
                        let asset_prog = ProgressBar::new(0)
                            .with_style(sty.clone())
                            .with_message("Downloading assets...");
                        asset_progress = Some(asset_prog.clone());
                        progress.add(asset_prog.clone());
                        asset_prog.enable_steady_tick(Duration::from_millis(100));
                    }
                }
                DownloadEvent::ResourceTotalInc => {
                    let asset_prog = asset_progress.as_ref().unwrap();
                    asset_prog.inc_length(1);
                }
                DownloadEvent::ResourceDownloadedInc => {
                    let asset_prog = asset_progress.as_ref().unwrap();
                    asset_prog.inc(1);
                    if asset_prog.position() == asset_prog.length().unwrap() {
                        asset_prog.finish_with_message("Downloading assets... done");
                    }
                }
            }
        }
    }
}

pub async fn archive(
    token: &str,
    topic_id: usize,
    save_at: &Path,
    anonymous: bool,
) -> anyhow::Result<()> {
    let progress = MultiProgress::new();

    let client = create_client_with_token(token, rate_limit_callback(progress.clone())).await?;
    let (tx, rx) = mpsc::channel(8);
    tokio::spawn(display_task(progress, rx));
    archiver::archive(&client, topic_id, save_at, anonymous, tx).await?;

    println!("{}", style("Done.").green());
    Ok(())
}
