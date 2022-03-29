use std::future::Future;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;

use futures::Stream;
use parking_lot::{Mutex, RwLock};
use tokio::sync::mpsc::{unbounded_channel, UnboundedReceiver, UnboundedSender};
use tokio::task::JoinHandle;
use tokio_stream::wrappers::UnboundedReceiverStream;

pub struct FutQueue<F>
where
    F: Future,
{
    task_tx: RwLock<Option<UnboundedSender<F>>>,
    resp_rx: Mutex<Option<UnboundedReceiver<F::Output>>>,
    max_count: Arc<AtomicUsize>,
    handler: JoinHandle<()>,
}

impl<F> Drop for FutQueue<F>
where
    F: Future,
{
    fn drop(&mut self) {
        self.handler.abort();
    }
}

impl<F> FutQueue<F>
where
    F: 'static + Future + Send,
    F::Output: Send,
{
    pub fn new() -> Self {
        let (task_tx, mut task_rx) = unbounded_channel();
        let (resp_tx, resp_rx) = unbounded_channel();
        Self {
            task_tx: RwLock::new(Some(task_tx)),
            resp_rx: Mutex::new(Some(resp_rx)),
            max_count: Arc::new(Default::default()),
            handler: tokio::spawn(async move {
                while let Some(fut) = task_rx.recv().await {
                    let res = fut.await;
                    drop(resp_tx.send(res));
                }
            }),
        }
    }
    pub fn finish(&self) {
        self.task_tx.write().take().expect("queue already finished");
    }
    pub fn add_future(&self, fut: F) {
        self.max_count.fetch_add(1, Ordering::SeqCst);
        if self
            .task_tx
            .read()
            .as_ref()
            .expect("queue already closed")
            .send(fut)
            .is_err()
        {
            panic!("queue already closed");
        }
    }
    pub fn max_count(&self) -> usize {
        self.max_count.load(Ordering::SeqCst)
    }
    pub fn take_stream(&self) -> impl Stream<Item = F::Output> {
        UnboundedReceiverStream::new(self.resp_rx.lock().take().expect("queue already closed"))
    }
}
