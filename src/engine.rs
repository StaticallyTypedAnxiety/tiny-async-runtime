use crate::Timer;
use crate::{bindings::wasi::io::poll::Pollable, poll_tasks::PollTasks};
use crate::{io::timer::TIMERS, poll_tasks::EventWithWaker};
use crossbeam::queue::SegQueue;
use dashmap::DashMap;
use lazy_static::lazy_static;
use std::{
    future::Future,
    pin::Pin,
    sync::{Arc, Mutex},
    task::{Context, Wake, Waker},
};
use uuid::Uuid;
lazy_static! {
    /// The global reactor for this runtime
    pub static ref REACTOR: Reactor<'static> = Reactor::default();
    //queue for ready tasks
    pub static ref READY_QUEUE: SegQueue<Uuid> = SegQueue::new();

}
/// The async engine instance
pub struct WasmRuntimeAsyncEngine;

/// the reactor that processes poll submissions. Still Experimental
struct Task<'a> {
    task: Pin<Box<dyn Future<Output = ()> + Send + 'a>>,
    waker: Arc<FutureWaker>,
}

impl<'a> Task<'a> {
    fn new(id: Uuid, task: Pin<Box<dyn Future<Output = ()> + Send + 'a>>) -> Self {
        let waker = Arc::new(FutureWaker::new(id));
        Self { task, waker }
    }
}
#[derive(Default)]
pub struct Reactor<'a> {
    events: Mutex<PollTasks>,
    future_tasks: DashMap<Uuid, Mutex<Task<'a>>>, //right now the engine holds the tasks but depending
    timers: DashMap<String, EventWithWaker<Timer>>,
}

impl<'a> Reactor<'a> {
    //adds event to the queue
    pub fn register(&self, event_name: String, pollable: EventWithWaker<Arc<Pollable>>) {
        self.events.lock().unwrap().push(event_name, pollable);
    }
    //checks if descriptor has been added to the polling queue
    pub fn is_pollable(&self, key: &str) -> bool {
        self.events.lock().unwrap().contains(key)
    }

    //checks if timer is pollable
    pub fn is_timer_pollable(&self, key: &str) -> bool {
        self.timers.contains_key(key)
    }

    //polls event queue to see if any of the events are readycar
    pub fn wait_for_io(&self) {
        self.events.lock().unwrap().wait_for_pollables();
    }

    //checks if event is ready
    pub fn check_ready(&self, event_name: &str) -> bool {
        self.events.lock().unwrap().check_if_ready(event_name)
    }

    pub fn is_empty(&self) -> bool {
        self.events.lock().unwrap().is_empty() && self.future_tasks.is_empty()
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

    pub(crate) fn register_timer(&self, timer_name: String, event: EventWithWaker<Timer>) {
        self.timers.insert(timer_name, event);
    }

    pub(crate) fn push_task<K, F: Future<Output = K> + Send + 'static>(&self, future: F) -> Uuid {
        let id = Uuid::new_v4();
        let task = Task::new(
            id,
            Box::pin(async move {
                let _result = future.await;
            }),
        );
        self.future_tasks.insert(id, Mutex::new(task));
        READY_QUEUE.push(id);
        id
    }
}

impl WasmRuntimeAsyncEngine {
    /// function to execute futures
    pub fn block_on<K, F: Future<Output = K> + Send + 'static>(future: F) {
        let reactor = &REACTOR;
        reactor.push_task(future);
        loop {
            reactor.update_timers();
            reactor.wait_for_io();
            while let Some(id) = READY_QUEUE.pop() {
                reactor.future_tasks.remove_if_mut(&id, |_, task_info| {
                    let task_ref = task_info.get_mut().unwrap();
                    let waker = task_ref.waker.clone();
                    let waker: Waker = waker.into();
                    let mut context = Context::from_waker(&waker);
                    task_ref.task.as_mut().poll(&mut context).is_ready()
                });
            }

            if TIMERS.is_empty() && reactor.is_empty() {
                break;
            }
        }
    }

    pub fn spawn<K, F: Future<Output = ()> + Send + 'static>(future: F) {
        REACTOR.push_task(future);
    }
}

#[derive(Debug)]
struct FutureWaker {
    id: Uuid,
}

impl FutureWaker {
    pub fn new(id: Uuid) -> Self {
        Self { id }
    }
    fn wake_inner(&self) {
        READY_QUEUE.push(self.id);
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
        let reactor = Reactor::default();
        let id = reactor.push_task(async move {
            count_future.await;
        });
        let mut future_task = reactor.future_tasks.get_mut(&id).unwrap();
        let task = future_task.value_mut();
        let fut_waker = task.lock().unwrap().waker.clone();
        let waker: Waker = fut_waker.into();
        let count_future = &mut task.lock().unwrap().task;
        let mut context = Context::from_waker(&waker);
        futures::pin_mut!(count_future);
        let _ = count_future.as_mut().poll(&mut context);
    }

    #[test]
    fn test_block_on() {
        let count_future = CountFuture { max: 3, min: 0 };

        WasmRuntimeAsyncEngine::block_on(async move { assert_eq!(count_future.await, 3) });
    }
}
