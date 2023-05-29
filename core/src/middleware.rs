use std::time::{Duration, Instant};

use reqwest::{Request, Response, StatusCode};
use reqwest_middleware::{Error, Result};
use reqwest_middleware::{Middleware, Next};
use task_local_extensions::Extensions;
use tracing::warn;

use crate::client::with_timeout;

pub struct RetryMiddleware<C> {
    rate_limit_callback: C,
    rate_limit_lock: tokio::sync::RwLock<()>,
}

pub struct BypassThrottle(pub bool);

impl<C> RetryMiddleware<C> {
    pub fn new(rate_limit_callback: C) -> Self {
        Self {
            rate_limit_callback,
            rate_limit_lock: Default::default(),
        }
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
        loop {
            let duplicate_request = req.try_clone().expect("Request object is not clonable");

            let bypass_throttle = extensions
                .get::<BypassThrottle>()
                .map(|BypassThrottle(b)| *b)
                .unwrap_or_default();
            if !bypass_throttle {
                drop(self.rate_limit_lock.read().await); // ensure no rate limit in effect
            }

            let result = next.clone().run(duplicate_request, extensions).await;
            break match result {
                Ok(payload) if payload.status() == StatusCode::TOO_MANY_REQUESTS => {
                    warn!(url=?payload.url(), "TOO MANY REQUESTS");
                    let retry_after =
                        payload
                            .headers()
                            .get("retry-after")
                            .and_then(|retry_after| {
                                retry_after.to_str().unwrap_or_default().parse::<u64>().ok()
                            });
                    if let Some(retry_after) = retry_after {
                        // Lock all other requests.
                        let before_lock = Instant::now();

                        let _guard = self.rate_limit_lock.write().await;

                        let elapsed = before_lock.elapsed();
                        let retry_after = retry_after.saturating_sub(elapsed.as_secs());

                        if retry_after != 0 {
                            (self.rate_limit_callback)(retry_after + 1);
                            tokio::time::sleep(Duration::from_secs(retry_after + 1)).await;
                        }
                        continue;
                    }
                    Ok(payload)
                }
                _ => result,
            };
        }
    }
}

#[derive(Debug, Copy, Clone)]
pub struct TimeoutMiddleware(Duration);

impl TimeoutMiddleware {
    pub const fn new(timeout: Duration) -> Self {
        Self(timeout)
    }
}

#[async_trait::async_trait]
impl Middleware for TimeoutMiddleware {
    async fn handle(
        &self,
        req: Request,
        extensions: &mut Extensions,
        next: Next<'_>,
    ) -> Result<Response> {
        with_timeout(next.run(req, extensions), self.0)
            .await
            .map_err(Error::middleware)
    }
}
