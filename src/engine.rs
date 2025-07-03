use crossbeam::channel::{Receiver, Sender};
use futures::pin_mut;
use lazy_static::lazy_static;
use std::{
    future::Future,
    sync::{atomic::AtomicU32, Arc, Mutex},
    task::{Context, Poll, Wake},
};
const TASK_QUEUE_BUFFER: usize = 1024;
use crate::io::timer::TIMERS;
use crate::{bindings::wasi::io::poll::Pollable, poll_tasks::PollTasks};
lazy_static! {
    /// The global reactor for this runtime
    pub static ref REACTOR: Mutex<Reactor> = Mutex::new(Reactor::default());

}

pub(crate) static NEXT_ID: AtomicU32 = AtomicU32::new(0);
/// The async engine instance
pub struct WasmRuntimeAsyncEngine {
    waker: Arc<FutureWaker>,
    recv: Receiver<()>,
}

/// the reactor that processes poll submissions. Still Experimental

#[derive(Default)]
pub struct Reactor {
    events: PollTasks,
}

impl Reactor {
    //adds event to the queue
    pub fn register(&mut self, event_name: String, pollable: Arc<Pollable>) {
        self.events.push(event_name, pollable);
    }
    //checks if descriptor has been added to the polling queue
    pub fn is_pollable(&self, key: &str) -> bool {
        self.events.contains(key)
    }

    //polls event queue to see if any of the events are readycar
    pub fn wait(&mut self) {
        self.events.wait_for_pollables();
    }

    //checks if event is ready
    pub fn check_ready(&mut self, event_name: &str) -> bool {
        self.events.check_if_ready(event_name)
    }
}

impl WasmRuntimeAsyncEngine {
    /// function to execute futures
    pub fn block_on<K, F: Future<Output = K>, Fun: FnOnce() -> F>(async_closure: Fun) -> K {
        let future = async_closure();
        pin_mut!(future);
        let (sender, recv) = crossbeam::channel::bounded(TASK_QUEUE_BUFFER);
        let runtime_engine = WasmRuntimeAsyncEngine {
            waker: Arc::new(FutureWaker(sender.clone())),
            recv,
        };
        let waker = runtime_engine.waker.into();
        let mut context = Context::from_waker(&waker);
        let _ = sender.send(()); //initial send;
        loop {
            TIMERS.iter_mut().for_each(|mut cell| cell.update_elapsed());
            REACTOR.lock().unwrap().wait();
            if runtime_engine.recv.recv().is_ok() {
                if let Poll::Ready(res) = future.as_mut().poll(&mut context) {
                    return res;
                }
            }
        }
    }
}

struct FutureWaker(Sender<()>);

impl FutureWaker {
    fn wake_inner(&self) {
        let _ = self.0.send(());
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
        let (sender, recv) = crossbeam::channel::unbounded();
        let count_future = CountFuture { max: 3, min: 0 };
        let runtime_engine = WasmRuntimeAsyncEngine {
            waker: FutureWaker(sender).into(),
            recv,
        };
        let waker = runtime_engine.waker.into();
        let mut context = Context::from_waker(&waker);
        futures::pin_mut!(count_future);
        let _ = count_future.as_mut().poll(&mut context);
        let _ = count_future.as_mut().poll(&mut context);
        assert_eq!(runtime_engine.recv.len(), 2);
    }

    #[test]
    fn test_block_on() {
        let count_future = CountFuture { max: 3, min: 0 };

        assert_eq!(
            WasmRuntimeAsyncEngine::block_on(|| async move { count_future.await }),
            3
        );
    }
}
