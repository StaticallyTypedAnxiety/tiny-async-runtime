use dashmap::DashMap;
use lazy_static::lazy_static;
use std::future::Future;
use std::time::Duration;
use std::time::Instant;

use crate::engine::NEXT_ID;
lazy_static! {
    pub static ref TIMERS: DashMap<u32, Timer> = DashMap::new();
}

#[derive(Debug)]
pub struct Timer {
    at: Instant,
    until: Duration,
    elapsed: bool,
}

impl Timer {
    /// create a timer that resolves once it elapses
    pub async fn sleep(until: std::time::Duration) {
        let id = NEXT_ID.load(std::sync::atomic::Ordering::Relaxed);
        TIMERS.insert(
            id,
            Self {
                at: Instant::now(),
                until,
                elapsed: false,
            },
        );
        NEXT_ID.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let timeout = TimeoutFuture { id };
        timeout.await
    }
    pub fn update_elapsed(&mut self) {
        let new_now = Instant::now();
        let elapsed = new_now
            .checked_duration_since(self.at)
            .map(|s| s > self.until)
            .unwrap_or_default();
        println!("elapsed {elapsed}");
        self.elapsed = elapsed;
    }

    pub fn elapsed(&self) -> bool {
        self.elapsed
    }
}

struct TimeoutFuture {
    id: u32,
}

impl Future for TimeoutFuture {
    type Output = ();
    fn poll(
        self: std::pin::Pin<&mut Self>,
        _: &mut std::task::Context<'_>,
    ) -> std::task::Poll<Self::Output> {
        let has_elapsed = TIMERS
            .get(&self.id)
            .map(|s| s.elapsed())
            .unwrap_or_default();
        if has_elapsed {
            return std::task::Poll::Ready(());
        }
        std::task::Poll::Pending
    }
}
