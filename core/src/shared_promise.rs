use tokio::sync::watch::{channel, Receiver, Sender};
use tracing::error;

pub struct Swear<T> {
    tx: Sender<Option<T>>,
    fulfilled: bool,
}

#[derive(Debug, Clone)]
pub struct SharedPromise<T>(Receiver<Option<T>>);

pub fn shared_promise_pair<T>() -> (Swear<T>, SharedPromise<T>) {
    let (tx, rx) = channel(None);
    (Swear::new(tx), SharedPromise(rx))
}

impl<T> Swear<T> {
    fn new(tx: Sender<Option<T>>) -> Self {
        Self {
            tx,
            fulfilled: false,
        }
    }
    pub fn fulfill(mut self, value: T) {
        self.tx.send_replace(Some(value));
        self.fulfilled = true;
    }
}

impl<T> Drop for Swear<T> {
    fn drop(&mut self) {
        if !self.fulfilled {
            error!("Unfulfilled promise");
        }
    }
}

impl<T: Clone + Send + Sync> SharedPromise<T> {
    pub fn is_forgot(&self) -> bool {
        self.0.has_changed().is_err()
    }
    pub async fn recv(mut self) -> Option<T> {
        drop(self.0.changed().await);
        self.0.borrow().as_ref().cloned()
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
