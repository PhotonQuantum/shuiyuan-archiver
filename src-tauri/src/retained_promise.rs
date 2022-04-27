use std::future::Future;
use std::sync::Arc;

use parking_lot::RwLock;
use tokio::sync::watch::{channel, Receiver, Sender};

pub struct RetainedSwear<T>(Sender<Option<T>>, Arc<RwLock<Inner<T>>>);

pub struct RetainedPromise<T>(Arc<RwLock<Inner<T>>>);

#[derive(Debug, Clone)]
enum Inner<T> {
    Resolved(T),
    Pending(Receiver<Option<T>>),
}

pub fn promise_pair<T>() -> (RetainedSwear<T>, RetainedPromise<T>) {
    let (tx, rx) = channel(None);
    let promise = RetainedPromise(Arc::new(RwLock::new(Inner::Pending(rx))));
    let swear = RetainedSwear(tx, promise.0.clone());
    (swear, promise)
}

impl<T: Clone> RetainedSwear<T> {
    pub fn fulfill(self, value: T) {
        *self.1.write() = Inner::Resolved(value.clone());
        drop(self.0.send(Some(value))); // It's ok that we have no receivers.
    }
}

impl<T: Clone> RetainedPromise<T> {
    pub fn recv(&self) -> impl Future<Output = T> {
        let inner = (&*self.0.read()).clone();
        async move {
            match inner {
                Inner::Resolved(v) => v,
                Inner::Pending(mut f) => {
                    f.changed()
                        .await
                        .expect("Fatal: Promise is in pending state but sender is gone");
                    f.borrow().as_ref().expect("Fatal: Phantom receive").clone()
                }
            }
        }
    }
}
