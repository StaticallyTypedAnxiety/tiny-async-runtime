//! `block_on` drives a future to completion via `wit_bindgen`'s
//! component-model-async support; `spawn` runs another one alongside it and
//! returns a cancellable [`JoinHandle`].

use futures::channel::oneshot;
use futures::FutureExt;
use std::future::Future;
use std::pin::Pin;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::task::{Context, Poll};
use wit_bindgen::rt::async_support;

static BLOCK_ON_GUARD: Mutex<()> = Mutex::new(());

pub struct WasmRuntimeAsyncEngine;

pin_project_lite::pin_project! {
    struct Cancelable<F> {
        canceled: Arc<AtomicBool>,
        #[pin]
        inner: F,
    }
}

impl<F: Future> Future for Cancelable<F> {
    type Output = Option<F::Output>;

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        let this = self.project();
        if this.canceled.load(Ordering::SeqCst) {
            return Poll::Ready(None);
        }
        this.inner.poll(cx).map(Some)
    }
}

pub struct JoinHandle<T> {
    receiver: oneshot::Receiver<T>,
    canceled: Arc<AtomicBool>,
}

impl<T> JoinHandle<T> {
    pub fn cancel(&self) {
        self.canceled.store(true, Ordering::SeqCst);
    }
}

impl<T> Future for JoinHandle<T> {
    type Output = T;

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        let this = self.get_mut();
        match this.receiver.poll_unpin(cx) {
            Poll::Ready(Ok(result)) => Poll::Ready(result),
            Poll::Ready(Err(_)) => {
                panic!("task was cancelled or panicked before producing a result")
            }
            Poll::Pending => Poll::Pending,
        }
    }
}

impl WasmRuntimeAsyncEngine {
    pub fn block_on<K: 'static, F: Future<Output = K> + 'static>(future: F) -> K {
        let _guard = BLOCK_ON_GUARD
            .lock()
            .expect("block_on runtime guard lock poisoned");
        async_support::block_on(future)
    }

    pub fn spawn<K: 'static, F: Future<Output = K> + 'static>(future: F) -> JoinHandle<K> {
        let (sender, receiver) = oneshot::channel();
        let canceled = Arc::new(AtomicBool::new(false));
        let wrapped = Cancelable {
            canceled: canceled.clone(),
            inner: future,
        };
        async_support::spawn_local(async move {
            if let Some(result) = wrapped.await {
                let _ = sender.send(result);
            }
        });
        JoinHandle { receiver, canceled }
    }
}
