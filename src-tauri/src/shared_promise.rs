use tokio::sync::watch::{channel, Receiver, Sender};

pub struct Swear<T>(Sender<Option<T>>);

#[derive(Debug, Clone)]
pub struct SharedPromise<T>(Receiver<Option<T>>);

pub fn shared_promise_pair<T>() -> (Swear<T>, SharedPromise<T>) {
    let (tx, rx) = channel(None);
    (Swear(tx), SharedPromise(rx))
}

impl<T> Swear<T> {
    pub fn fulfill(self, value: T) {
        self.0.send_replace(Some(value));
    }
}

impl<T: Clone + Send + Sync> SharedPromise<T> {
    pub async fn recv(mut self) -> T {
        drop(self.0.changed().await);
        self.0.borrow().as_ref().unwrap().clone()
    }
}

#[cfg(test)]
mod tests {
    use tokio::sync::{mpsc, oneshot};

    #[tokio::test]
    async fn must_resolve() {
        let (swear, promise) = super::shared_promise_pair();
        let (tx, _rx) = oneshot::channel();
        let handler = tokio::spawn(async move {
            tx.send(()).unwrap();
            assert_eq!(42, promise.recv().await);
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

                assert_eq!(42, promise.recv().await);
                assert_eq!(42, promise_2.recv().await);
            })
        };
        swear.fulfill(42);
        assert_eq!(42, promise.recv().await);
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
                    assert_eq!(42, promise.recv().await);
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
