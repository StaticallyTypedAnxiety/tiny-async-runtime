use crate::engine::REACTOR;
use crate::poll_tasks::EventWithWaker;
use dashmap::DashMap;
use lazy_static::lazy_static;
use std::future::Future;
use std::time::Duration;
use std::time::Instant;
use uuid::Uuid;
lazy_static! {
    pub static ref TIMERS: DashMap<u32, EventWithWaker<Timer>> = DashMap::new();
}

#[derive(Debug, Clone)]
pub struct Timer {
    at: Instant,
    until: Duration,
    elapsed: bool,
}

impl Timer {
    /// create a timer that resolves once it elapses
    pub async fn sleep(until: std::time::Duration) {
        let timeout = TimeoutFuture {
            timer_key: format!("sleep-{}", Uuid::new_v4()),
            timer: Self {
                at: Instant::now(),
                until,
                elapsed: false,
            },
        };
        timeout.await
    }
    pub fn update_elapsed(&mut self) {
        let new_now = Instant::now();
        let elapsed = new_now
            .checked_duration_since(self.at)
            .map(|s| s > self.until)
            .unwrap_or_default();
        self.elapsed = elapsed;
    }

    pub fn elapsed(&self) -> bool {
        self.elapsed
    }
}

struct TimeoutFuture {
    timer_key: String,
    timer: Timer,
}

impl Future for TimeoutFuture {
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
