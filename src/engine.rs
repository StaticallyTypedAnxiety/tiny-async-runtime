use crate::Timer;
use crate::{bindings::wasi::io::poll::Pollable, poll_tasks::PollTasks};
use crate::{io::timer::TIMERS, poll_tasks::EventWithWaker};
use dashmap::DashMap;
use futures::pin_mut;
use lazy_static::lazy_static;
use std::{
    future::Future,
    pin::Pin,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Mutex,
    },
    task::{Context, Wake, Waker},
};
lazy_static! {
    /// The global reactor for this runtime
    pub static ref REACTOR: Mutex<Reactor<'static>> = Mutex::new(Reactor::default());

}
/// The async engine instance
pub struct WasmRuntimeAsyncEngine;

/// the reactor that processes poll submissions. Still Experimental
struct Task<'a> {
    task: Pin<Box<dyn Future<Output = ()> + Send + 'a>>,
    waker: Arc<FutureWaker>,
}

impl<'a> Task<'a> {
    fn new(task: Pin<Box<dyn Future<Output = ()> + Send + 'a>>) -> Self {
        let waker = Arc::new(FutureWaker::default());
        Self { task, waker }
    }
}
#[derive(Default)]
pub struct Reactor<'a> {
    events: PollTasks,
    future_tasks: Vec<Task<'a>>, //right now the engine holds the tasks but depending
    timers: DashMap<String, EventWithWaker<Timer>>,
}

impl<'a> Reactor<'a> {
    //adds event to the queue
    pub fn register(&mut self, event_name: String, pollable: EventWithWaker<Arc<Pollable>>) {
        self.events.push(event_name, pollable);
    }
    //checks if descriptor has been added to the polling queue
    pub fn is_pollable(&self, key: &str) -> bool {
        self.events.contains(key)
    }

    //checks if timer is pollable
    pub fn is_timer_pollable(&self, key: &str) -> bool {
        self.timers.contains_key(key)
    }

    //polls event queue to see if any of the events are readycar
    pub fn wait_for_io(&mut self) {
        self.events.wait_for_pollables();
    }

    //checks if event is ready
    pub fn check_ready(&mut self, event_name: &str) -> bool {
        self.events.check_if_ready(event_name)
    }

    pub fn is_empty(&self) -> bool {
        self.events.is_empty() && self.future_tasks.is_empty()
    }

    pub(crate) fn update_timers(&self) {
        self.timers.iter_mut().for_each(|mut cell| {
            cell.0.update_elapsed();
            if cell.0.elapsed() {
                cell.1.wake_by_ref();
            }
        });
    }

    pub(crate) fn timer_has_elapsed(&self, timer_key: &str) -> bool {
        self.timers
            .get(timer_key)
            .map(|s| s.0.elapsed())
            .unwrap_or_default()
    }

    pub(crate) fn register_timer(&mut self, timer_name: String, event: EventWithWaker<Timer>) {
        self.timers.insert(timer_name, event);
    }
}

impl WasmRuntimeAsyncEngine {
    /// function to execute futures
    pub fn block_on<K, F: Future<Output = K> + Send, Fun: FnOnce() -> F>(async_closure: Fun) {
        let future = async_closure();
        pin_mut!(future);
        let task = Task::new(Box::pin(async move {
            let _result = future.await;
        }));
        let mut future_tasks = Vec::new();
        future_tasks.push(task);
        loop {
            let mut reactor = REACTOR.lock().unwrap();
            reactor.update_timers();
            reactor.wait_for_io();
            reactor.future_tasks.retain_mut(|task_info| {
                if task_info.waker.should_wake() {
                    task_info.waker.reset();
                    let waker: Waker = task_info.waker.clone().into();
                    let mut context = Context::from_waker(&waker);
                    if task_info.task.as_mut().poll(&mut context).is_ready() {
                        return false;
                    }
                }
                true
            });
            if TIMERS.is_empty() && reactor.is_empty() {
                break;
            }
        }
    }

    pub fn spawn<K, F: Future<Output = ()> + Send + 'static>(future: F) {
        let task = Task::new(Box::pin(async move {
            let _result = future.await;
        }));
        REACTOR.lock().unwrap().future_tasks.push(task);
    }
}

#[derive(Debug)]
struct FutureWaker(AtomicBool);

impl Default for FutureWaker {
    fn default() -> Self {
        Self(AtomicBool::new(true))
    }
}
impl FutureWaker {
    fn wake_inner(&self) {
        self.0.store(true, Ordering::Relaxed);
    }

    fn reset(&self) {
        self.0.store(false, Ordering::Relaxed);
    }

    fn should_wake(&self) -> bool {
        self.0.load(Ordering::Relaxed)
    }
}

impl Wake for FutureWaker {
    fn wake(self: std::sync::Arc<Self>) {
        self.wake_inner();
    }
}

#[cfg(test)]
mod test {

    use super::*;
    use std::future::Future;

    struct CountFuture {
        min: u8,
        max: u8,
    }

    impl Future for CountFuture {
        type Output = u8;
        fn poll(
            self: std::pin::Pin<&mut Self>,
            cx: &mut std::task::Context<'_>,
        ) -> std::task::Poll<Self::Output> {
            let count_fut_mut = self.get_mut();
            if count_fut_mut.min == count_fut_mut.max {
                return std::task::Poll::Ready(count_fut_mut.min);
            }

            count_fut_mut.min += 1;
            cx.waker().wake_by_ref();
            std::task::Poll::Pending
        }
    }

    #[test]
    fn test_enqueue() {
        let count_future = CountFuture { max: 3, min: 0 };
        let mut reactor = Reactor::default();
        reactor.future_tasks.push(Task::new(Box::pin(async move {
            count_future.await;
        })));
        let task = reactor.future_tasks.first_mut().unwrap();
        let fut_waker = task.waker.clone();
        let waker: Waker = fut_waker.into();
        let count_future = &mut task.task;
        let mut context = Context::from_waker(&waker);
        futures::pin_mut!(count_future);
        let _ = count_future.as_mut().poll(&mut context);
    }

    #[test]
    fn test_block_on() {
        let count_future = CountFuture { max: 3, min: 0 };

        WasmRuntimeAsyncEngine::block_on(|| async move { assert_eq!(count_future.await, 3) });
    }
}
