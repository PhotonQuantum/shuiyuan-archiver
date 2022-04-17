use std::future::Future;
use std::pin::Pin;
use std::time::Duration;

use reqwest::{Request, Response, StatusCode};
use reqwest_middleware::{Middleware, Next};
use reqwest_middleware::Result;
use task_local_extensions::Extensions;

use crate::RateLimitWatcher;

pub struct RetryMiddleware {
    rate_limit_watcher: RateLimitWatcher,
}

impl RetryMiddleware {
    fn execute<'a>(
        &'a self,
        req: Request,
        extensions: &'a mut Extensions,
        next: Next<'a>,
    ) -> Pin<Box<dyn Future<Output=Result<Response>> + 'a + Send>> {
        let duplicate_request = req.try_clone().expect("Request object is not clonable");
        let cloned_next = next.clone();
        Box::pin(async move {
            let result = next.run(req, extensions).await;
            match result {
                Ok(payload) if payload.status() == StatusCode::TOO_MANY_REQUESTS => {
                    if let Some(retry_after) = payload.headers().get("retry-after") {
                        if let Ok(delay) = retry_after.to_str().unwrap_or_default().parse::<u64>() {
                            eprintln!("Wait for {} seconds", delay + 1);
                            self.rate_limit_watcher.register_limit(delay + 1);
                            tokio::time::sleep(Duration::from_secs(delay + 1)).await;
                            return self
                                .execute(duplicate_request, extensions, cloned_next)
                                .await;
                        }
                    }
                    Ok(payload)
                }
                _ => result,
            }
        })
    }
    pub fn new(rate_limit_watcher: RateLimitWatcher) -> Self {
        Self { rate_limit_watcher }
    }
}

#[async_trait::async_trait]
impl Middleware for RetryMiddleware {
    async fn handle(
        &self,
        req: Request,
        extensions: &mut Extensions,
        next: Next<'_>,
    ) -> Result<Response> {
        self.execute(req, extensions, next).await
    }
}
