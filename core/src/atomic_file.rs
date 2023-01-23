#[cfg(test)]
use std::io;
use std::io::{BufWriter, Write};
use std::path::{Path, PathBuf};
#[cfg(test)]
use std::sync::atomic::AtomicBool;
#[cfg(test)]
use std::sync::atomic::Ordering;

use bytes::Bytes;
use futures::FutureExt;
use tap::TapFallible;
use tempfile::NamedTempFile;
use tokio::runtime::Handle;
use tokio::sync::oneshot::error::TryRecvError;
use tokio::sync::{mpsc, oneshot};
use tokio::task::JoinHandle;
use tracing::warn;

use crate::error::{Error, Result};

#[cfg(test)]
static WRITE_FAIL: AtomicBool = AtomicBool::new(false);

#[derive(Debug)]
enum Event {
    Data(Bytes),
    Finish,
}

pub struct AtomicFile {
    handle: JoinHandle<()>,
    data_tx: mpsc::Sender<Event>,
    cancel_tx: Option<oneshot::Sender<()>>,
    result_rx: Option<oneshot::Receiver<Result<()>>>,
}

impl Drop for AtomicFile {
    fn drop(&mut self) {
        self.handle.abort();
    }
}

impl AtomicFile {
    pub fn new(path: &Path) -> Result<Self> {
        let file = NamedTempFile::new_in(path.parent().expect("file has parent"))?;
        let (data_tx, data_rx) = mpsc::channel(16);
        let (cancel_tx, cancel_rx) = oneshot::channel();
        let (result_tx, result_rx) = oneshot::channel();
        let handle = tokio::task::spawn_blocking({
            let path = path.to_path_buf();
            move || Self::blocking_task(file, path, data_rx, cancel_rx, result_tx)
        });
        Ok(Self {
            handle,
            data_tx,
            cancel_tx: Some(cancel_tx),
            result_rx: Some(result_rx),
        })
    }
    pub async fn write(&mut self, data: Bytes) -> Result<()> {
        let result = self
            .result_rx
            .as_mut()
            .ok_or(Error::AtomicFilePoisoned)?
            .try_recv();
        match result {
            Ok(Ok(())) => unreachable!("finalized or cancel channel closed"),
            Ok(Err(e)) => {
                self.result_rx.take().expect("poison");
                return Err(e);
            } // write error
            Err(TryRecvError::Empty) => (), // no error
            Err(TryRecvError::Closed) => {
                unreachable!("sync thread dead without sending result, or poll after complete")
            }
        }
        self.data_tx.send(Event::Data(data)).await?;
        Ok(())
    }

    /// Commit the file to the final path.
    ///
    /// # Errors
    ///
    /// Returns error if data task is dead, or any error occurs during file write.
    pub async fn commit(mut self) -> Result<()> {
        if let Some(result_rx) = self.result_rx.take() {
            self.data_tx
                .send(Event::Finish)
                .await
                .expect("sync thread dead");
            result_rx
                .await
                .expect("sync thread dead without sending result")
        } else {
            Err(Error::AtomicFilePoisoned)
        }
    }

    /// Commit the file to the final path.
    ///
    /// # Errors
    ///
    /// Returns error if data task is dead, or any error occurs during file write.
    pub async fn cancel(mut self) -> Result<()> {
        if let Some(result_rx) = self.result_rx.take() {
            self.cancel_tx
                .take()
                .expect("can only cancel once")
                .send(())
                .expect("sync thread dead");
            result_rx
                .await
                .expect("sync thread dead without sending result")
        } else {
            Err(Error::AtomicFilePoisoned)
        }
    }
    fn blocking_task(
        file: NamedTempFile,
        path: PathBuf,
        mut data_rx: mpsc::Receiver<Event>,
        cancel_rx: oneshot::Receiver<()>,
        result_tx: oneshot::Sender<Result<()>>,
    ) {
        let mut writer = BufWriter::with_capacity(64 * 1024, file);
        let mut cancel_rx = cancel_rx.fuse();
        let res = Handle::current().block_on(async move {
            loop {
                break tokio::select! {
                    res = data_rx.recv() => match res {
                        Some(Event::Data(data)) => match writer.write_all(&data) {
                            Ok(()) => {  // file write succeeded
                                #[cfg(test)]
                                if WRITE_FAIL.load(Ordering::SeqCst) {
                                    Err(Error::from(io::Error::new(io::ErrorKind::Other, "test")))
                                } else {
                                    continue
                                }
                                #[cfg(not(test))]
                                continue
                            },
                            Err(e) => Err(Error::from(e)),    // file write failed
                        },
                        Some(Event::Finish) => writer
                            .into_inner()
                            .map_err(std::io::IntoInnerError::into_error)?
                            .persist(path)
                            .map_err(std::convert::Into::into)
                            .and_then(|f| Ok(f.sync_all()?)),
                        None => continue,   // data channel closed, should receive cancel_rx ok or err soon
                    },
                    res = &mut cancel_rx => match res {
                        Ok(()) => Ok(()),  // cancel requested
                        Err(_e) => {
                            warn!("AtomicFile closed without cancel or commit");
                            Ok(())
                        }
                    }
                };
            }
        });
        drop(
            result_tx
                .send(res)
                .tap_err(|_| warn!("async side is not listening")),
        );
    }
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::Ordering;
    use std::time::Duration;
    use tokio::sync::RwLock;

    use bytes::Bytes;
    use once_cell::sync::Lazy;
    use tempfile::TempDir;

    use crate::error::Error;

    use super::AtomicFile;
    use super::WRITE_FAIL;

    /// Read lock for normal behavior and write lock for fail cases.
    static TEST_LOCK: Lazy<RwLock<()>> = Lazy::new(|| RwLock::new(()));

    #[tokio::test]
    async fn must_commit() {
        let _guard = TEST_LOCK.read().await;

        let dir = TempDir::new().unwrap();
        let mut f = AtomicFile::new(&dir.path().join("test")).unwrap();

        f.write(Bytes::from("hello world")).await.unwrap();
        f.commit().await.unwrap();

        assert_eq!(
            std::fs::read_to_string(dir.path().join("test")).unwrap(),
            "hello world"
        );
    }

    #[tokio::test]
    async fn must_cancel() {
        let _guard = TEST_LOCK.read().await;

        let dir = TempDir::new().unwrap();
        let mut f = AtomicFile::new(&dir.path().join("test")).unwrap();

        f.write(Bytes::from("hello world")).await.unwrap();
        f.cancel().await.unwrap();

        tokio::time::sleep(Duration::from_millis(500)).await; // hope that's enough
        assert!(!dir.path().join("test").exists());
    }

    #[tokio::test]
    async fn must_cancel_on_drop() {
        let _guard = TEST_LOCK.read().await;

        let dir = TempDir::new().unwrap();

        {
            let mut f = AtomicFile::new(&dir.path().join("test")).unwrap();
            f.write(Bytes::from("hello world")).await.unwrap();
        }

        tokio::time::sleep(Duration::from_millis(500)).await; // hope that's enough
        assert!(!dir.path().join("test").exists());
    }

    #[tokio::test]
    async fn must_error_on_write() {
        let _guard = TEST_LOCK.write().await;
        WRITE_FAIL.store(true, Ordering::SeqCst);

        let dir = TempDir::new().unwrap();
        let mut f = AtomicFile::new(&dir.path().join("test")).unwrap();

        assert!(f.write(Bytes::from("hello world")).await.is_ok()); // write failed, but no error at once
        tokio::time::sleep(Duration::from_millis(500)).await; // hope that's enough
        assert!(f.write(Bytes::from("hello world")).await.is_err()); // last error returned
        assert!(matches!(
            f.write(Bytes::from("hello world")).await,
            Err(Error::AtomicFilePoisoned)
        )); // may not write afterwards

        assert!(matches!(f.cancel().await, Err(Error::AtomicFilePoisoned))); // poisoned after error

        WRITE_FAIL.store(false, Ordering::SeqCst);
    }
}
