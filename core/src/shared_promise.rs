use std::borrow::Borrow;

use futures::{future, FutureExt, SinkExt};
use tap::TapFallible;
use tokio::sync::oneshot;
use tracing::{error, warn};

pub struct Swear<T> {
    tx: Option<oneshot::Sender<T>>,
}

#[derive(Debug, Clone)]
pub struct SharedPromise<T: Clone>(future::Shared<oneshot::Receiver<T>>);

pub fn shared_promise_pair<T: Clone>() -> (Swear<T>, SharedPromise<T>) {
    let (tx, rx) = oneshot::channel();
    (Swear::new(tx), SharedPromise(rx.shared()))
}

impl<T> Swear<T> {
    fn new(tx: oneshot::Sender<T>) -> Self {
        Self { tx: Some(tx) }
    }
    pub fn fulfill(mut self, value: T) {
        drop(
            self.tx
                .take()
                .expect("fulfilled only once")
                .send(value)
                .tap_err(|e| warn!("Nobody's listening on promise")),
        );
    }
}

impl<T> Drop for Swear<T> {
    fn drop(&mut self) {
        if self.tx.is_some() {
            error!("Unfulfilled promise");
        }
    }
}

impl<T: Clone + Send + Sync> SharedPromise<T> {
    pub async fn recv(mut self) -> Option<T> {
        self.0.await.ok()
    }
}

#[cfg(test)]
mod tests {
    use std::assert_eq;

    use tokio::sync::{mpsc, oneshot};

    #[tokio::test]
    async fn must_resolve() {
        let (swear, promise) = super::shared_promise_pair();
        let (tx, _rx) = oneshot::channel();
        let handler = tokio::spawn(async move {
            tx.send(()).unwrap();
            assert_eq!(42, promise.recv().await.unwrap());
        });
        swear.fulfill(42);
        handler.await.unwrap();
    }

    #[tokio::test]
    async fn must_retain() {
        let (swear, promise) = super::shared_promise_pair();
        let (tx, _rx) = oneshot::channel();
        let handler = {
            let promise = promise.clone();
            let promise_2 = promise.clone();
            tokio::spawn(async move {
                tx.send(()).unwrap();

                assert_eq!(42, promise.recv().await.unwrap());
                assert_eq!(42, promise_2.recv().await.unwrap());
            })
        };
        swear.fulfill(42);
        assert_eq!(42, promise.recv().await.unwrap());
        handler.await.unwrap();
    }

    #[tokio::test]
    async fn must_resolve_multi() {
        let (swear, promise) = super::shared_promise_pair();
        let (tx, mut rx) = mpsc::channel(5);
        let handlers: Vec<_> = (0..5)
            .map(|_| {
                let tx = tx.clone();
                let promise = promise.clone();
                tokio::spawn(async move {
                    tx.send(()).await.unwrap();
                    assert_eq!(42, promise.recv().await.unwrap());
                })
            })
            .collect();
        for _ in 0..5 {
            drop(rx.recv());
        }
        swear.fulfill(42);
        for handler in handlers {
            handler.await.unwrap();
        }
    }
}
