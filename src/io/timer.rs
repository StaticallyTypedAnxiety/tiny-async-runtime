use crate::bindings::wasi::clocks::monotonic_clock::wait_for;
use std::future::Future;
use std::task::Poll;
use std::time::{Duration, Instant};

#[derive(Debug, Clone)]
pub struct Timer {
    at: Instant,
    deadline: Duration,
    elapsed: bool,
}

impl Timer {
    pub async fn sleep(until: Duration) {
        wait_for(until.as_nanos() as u64).await;
    }

    pub async fn timeout<K, F: Future<Output = K>>(
        fut: F,
        deadline: Duration,
    ) -> std::io::Result<K> {
        let timeout_future = TimeoutFuture {
            fut,
            timer_future: wait_for(deadline.as_nanos() as u64),
        };
        timeout_future.await
    }

    pub fn update_elapsed(&mut self) {
        let new_now = Instant::now();
        let elapsed = new_now
            .checked_duration_since(self.at)
            .map(|duration| duration > self.deadline)
            .unwrap_or_default();
        self.elapsed = elapsed;
    }

    pub fn elapsed(&self) -> bool {
        self.elapsed
    }
}

pin_project_lite::pin_project! {
    pub struct TimeoutFuture<K, F: Future<Output = K>, J: Future<Output = ()>> {
        #[pin]
        fut: F,
        #[pin]
        timer_future: J,
    }
}

impl<K, F: Future<Output = K>, J: Future<Output = ()>> Future for TimeoutFuture<K, F, J> {
    type Output = Result<K, std::io::Error>;

    fn poll(
        self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<Self::Output> {
        let this = self.project();
        if this.timer_future.poll(cx).is_pending() {
            match this.fut.poll(cx) {
                Poll::Ready(ready) => Poll::Ready(Ok(ready)),
                Poll::Pending => Poll::Pending,
            }
        } else {
            let error = std::io::Error::new(std::io::ErrorKind::TimedOut, "Timer has elapsed");
            Poll::Ready(Err(error))
        }
    }
}
