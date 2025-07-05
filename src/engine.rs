use crate::Timer;
use crate::{bindings::wasi::io::poll::Pollable, poll_tasks::PollTasks};
use crate::{io::timer::TIMERS, poll_tasks::EventWithWaker};
use crossbeam::queue::SegQueue;
use futures::channel::oneshot;
use futures::FutureExt;
use lazy_static::lazy_static;
use std::collections::hash_map::Entry;
use std::collections::HashMap;
use std::sync::RwLock;
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
    #[derive(Debug)]
    pub static ref READY_QUEUE: SegQueue<Uuid> = SegQueue::new();

}
/// The async engine instance
pub struct WasmRuntimeAsyncEngine;

/// the reactor that processes poll submissions. Still Experimental
struct Task<'a> {
    id: Uuid,
    task: Pin<Box<dyn Future<Output = ()> + Send + 'a>>,
    waker: Arc<FutureWaker>,
}

impl<'a> Task<'a> {
    fn new(task: Pin<Box<dyn Future<Output = ()> + Send + 'a>>) -> Self {
        let id = Uuid::new_v4();
        let waker = Arc::new(FutureWaker::new(id));
        Self { id, task, waker }
    }
}
#[derive(Default)]
pub struct Reactor<'a> {
    events: Mutex<PollTasks>,
    spawn_queue: SegQueue<Task<'a>>, //right now the engine holds the tasks but depending
    future_tasks: RwLock<HashMap<Uuid, Mutex<Task<'a>>>>,
    timers: RwLock<HashMap<String, EventWithWaker<Timer>>>,
}

pub struct JoinHandle<T> {
    id: Uuid,
    receiver: oneshot::Receiver<T>,
}

impl<T> JoinHandle<T> {
    pub fn cancel(&self) {
        REACTOR.remove_task(self.id);
    }
}

impl<T> Future for JoinHandle<T> {
    type Output = T;
    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> std::task::Poll<Self::Output> {
        let this = self.get_mut();
        match this.receiver.poll_unpin(cx) {
            std::task::Poll::Ready(Ok(result)) => std::task::Poll::Ready(result),
            std::task::Poll::Ready(_) => panic!("Could not join result"),
            std::task::Poll::Pending => std::task::Poll::Pending,
        }
    }
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
        self.timers.read().unwrap().contains_key(key)
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
        self.events.lock().unwrap().is_empty() && self.future_tasks.read().unwrap().is_empty()
    }

    pub(crate) fn update_timers(&self) {
        self.timers
            .write()
            .unwrap()
            .iter_mut()
            .for_each(|(_, cell)| {
                cell.0.update_elapsed();
                if cell.0.elapsed() {
                    cell.1.wake_by_ref();
                }
            });
    }
    pub(crate) fn remove_task(&self, id: Uuid) {
        self.future_tasks.write().unwrap().remove(&id);
        //Todo: See if you can remove timers and sockets as well
    }

    pub(crate) fn timer_has_elapsed(&self, timer_key: &str) -> bool {
        self.timers
            .read()
            .unwrap()
            .get(timer_key)
            .map(|s| s.0.elapsed())
            .unwrap_or_default()
    }

    pub(crate) fn remove_timer(&self, timer_key: &str) {
        self.timers.write().unwrap().remove(timer_key);
    }

    pub(crate) fn register_timer(&self, timer_name: String, event: EventWithWaker<Timer>) {
        self.timers.write().unwrap().insert(timer_name, event);
    }

    pub(crate) fn push_task<K: Send + 'a, F: Future<Output = K> + Send + 'static>(
        &self,
        future: F,
    ) -> JoinHandle<K> {
        let (sender, receiver) = oneshot::channel();
        let task = Task::new(Box::pin(async move {
            let result = future.await;
            let _ = sender.send(result);
        }));
        let id = task.id;
        self.spawn_queue.push(task);
        READY_QUEUE.push(id);
        JoinHandle { id, receiver }
    }

    pub(crate) fn drain_queue(&self) {
        while let Some(task) = self.spawn_queue.pop() {
            self.future_tasks
                .write()
                .unwrap()
                .insert(task.id, Mutex::new(task));
        }
    }
}

impl WasmRuntimeAsyncEngine {
    /// function to execute futures
    pub fn block_on<K: Send + 'static, F: Future<Output = K> + Send + 'static>(future: F) -> K {
        let reactor = &REACTOR;
        let mut join_handle = reactor.push_task(future);
        loop {
            reactor.update_timers();
            reactor.wait_for_io();
            while let Some(id) = READY_QUEUE.pop() {
                reactor.drain_queue();
                if let Entry::Occupied(mut entry) = reactor.future_tasks.write().unwrap().entry(id)
                {
                    let task_info = entry.get_mut();
                    let task_ref = task_info.get_mut().unwrap();
                    let waker = task_ref.waker.clone();
                    let waker: Waker = waker.into();
                    let mut context = Context::from_waker(&waker);
                    let polling_state = task_ref.task.as_mut().poll(&mut context);
                    if polling_state.is_ready() {
                        entry.remove();
                    }
                }
            }

            if TIMERS.is_empty() && reactor.is_empty() {
                break;
            }
        }

        loop {
            let mut context = Context::from_waker(Waker::noop());
            if let std::task::Poll::Ready(result) = join_handle.poll_unpin(&mut context) {
                return result;
            }
        }
    }

    pub fn spawn<K: Send + 'static, F: Future<Output = K> + Send + 'static>(
        future: F,
    ) -> JoinHandle<K> {
        println!("spawn code");
        REACTOR.push_task(future)
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
        let handle = reactor.push_task(async move {
            count_future.await;
        });
        reactor.drain_queue();
        let mut future_task = reactor.future_tasks.write().unwrap();
        let task = future_task.get_mut(&handle.id).unwrap();
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
