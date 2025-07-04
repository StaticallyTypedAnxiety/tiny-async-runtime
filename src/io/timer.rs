use crate::engine::REACTOR;
use crate::poll_tasks::EventWithWaker;
use dashmap::DashMap;
use futures::FutureExt;
use lazy_static::lazy_static;
use std::future::Future;
use std::task::Poll;
use std::time::Duration;
use std::time::Instant;
use uuid::Uuid;
lazy_static! {
    pub static ref TIMERS: DashMap<u32, EventWithWaker<Timer>> = DashMap::new();
}

#[derive(Debug, Clone)]
pub struct Timer {
    at: Instant,
    deadline: Duration,
    elapsed: bool,
}

impl Timer {
    /// create a timer that resolves once it elapses
    pub async fn sleep(until: std::time::Duration) {
        let timeout = TimeFuture {
            timer_key: format!("sleep-{}", Uuid::new_v4()),
            timer: Self {
                at: Instant::now(),
                deadline: until,
                elapsed: false,
            },
        };
        timeout.await
    }

    pub async fn timeout<K, F: Future<Output = K>>(
        &self,
        fut: F,
        deadline: std::time::Duration,
    ) -> std::io::Result<K> {
        let timer_future = TimeFuture {
            timer_key: format!("timeout-{}", Uuid::new_v4()),
            timer: Self {
                at: Instant::now(),
                deadline,
                elapsed: false,
            },
        };
        let timeout_future = TimeoutFuture { timer_future, fut };
        timeout_future.await
    }
    pub fn update_elapsed(&mut self) {
        let new_now = Instant::now();
        let elapsed = new_now
            .checked_duration_since(self.at)
            .map(|s| s > self.deadline)
            .unwrap_or_default();
        self.elapsed = elapsed;
    }

    pub fn elapsed(&self) -> bool {
        self.elapsed
    }
}

struct TimeFuture {
    timer_key: String,
    timer: Timer,
}

impl Future for TimeFuture {
    type Output = ();
    fn poll(
        self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<Self::Output> {
        let this = self.get_mut();
        let mut reactor = REACTOR.lock().unwrap();
        if reactor.is_timer_pollable(&this.timer_key) {
            let has_elapsed = reactor.timer_has_elapsed(&this.timer_key);
            if has_elapsed {
                return std::task::Poll::Ready(());
            }
        } else {
            reactor.register_timer(
                this.timer_key.clone(),
                (this.timer.clone(), cx.waker().clone()),
            );
        }

        std::task::Poll::Pending
    }
}

pin_project_lite::pin_project! {
    pub struct TimeoutFuture<K,F:Future<Output = K>>
    {
        #[pin]
        fut:F,
        #[pin]
        timer_future:TimeFuture
    }
}

impl<K, F: Future<Output = K>> Future for TimeoutFuture<K, F> {
    type Output = Result<K, std::io::Error>;
    fn poll(
        self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<Self::Output> {
        let mut this = self.project();
        if this.timer_future.poll_unpin(cx).is_pending() {
            match this.fut.poll_unpin(cx) {
                Poll::Ready(ready) => Poll::Ready(Ok(ready)),
                Poll::Pending => Poll::Pending,
            }
        } else {
            let error = std::io::Error::new(std::io::ErrorKind::TimedOut, "Timer has elapsed");
            Poll::Ready(Err(error))
        }
    }
}
