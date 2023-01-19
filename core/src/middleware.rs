use std::future::Future;
use std::pin::Pin;
use std::time::Duration;

use reqwest::{Request, Response, StatusCode};
use reqwest_middleware::Result;
use reqwest_middleware::{Middleware, Next};
use task_local_extensions::Extensions;
use tokio::sync::Semaphore;

pub struct RetryMiddleware<C> {
    rate_limit_callback: C,
}

impl<C> RetryMiddleware<C> {
    pub const fn new(rate_limit_callback: C) -> Self {
        Self {
            rate_limit_callback,
        }
    }
}

impl<C> RetryMiddleware<C>
where
    C: 'static + Fn(u64) + Send + Sync,
{
    fn execute<'a>(
        &'a self,
        req: Request,
        extensions: &'a mut Extensions,
        next: Next<'a>,
    ) -> Pin<Box<dyn Future<Output = Result<Response>> + 'a + Send>> {
        let duplicate_request = req.try_clone().expect("Request object is not clonable");
        let cloned_next = next.clone();
        Box::pin(async move {
            let result = next.run(req, extensions).await;
            match result {
                Ok(payload) if payload.status() == StatusCode::TOO_MANY_REQUESTS => {
                    if let Some(retry_after) = payload.headers().get("retry-after") {
                        if let Ok(delay) = retry_after.to_str().unwrap_or_default().parse::<u64>() {
                            (self.rate_limit_callback)(delay + 1);
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
}

#[async_trait::async_trait]
impl<C> Middleware for RetryMiddleware<C>
where
    C: 'static + Fn(u64) + Send + Sync,
{
    async fn handle(
        &self,
        req: Request,
        extensions: &mut Extensions,
        next: Next<'_>,
    ) -> Result<Response> {
        self.execute(req, extensions, next).await
    }
}

pub struct MaxConnMiddleware {
    sem: Semaphore,
}

impl MaxConnMiddleware {
    pub fn new(max_conn: usize) -> Self {
        Self {
            sem: Semaphore::new(max_conn),
        }
    }
}

#[async_trait::async_trait]
impl Middleware for MaxConnMiddleware {
    async fn handle(
        &self,
        req: Request,
        extensions: &mut Extensions,
        next: Next<'_>,
    ) -> Result<Response> {
        let _permit = self.sem.acquire().await;
        next.run(req, extensions).await
    }
}
