use std::future::Future;
use std::pin::Pin;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::task::{Context, Poll, Waker};
use std::time::Duration;
use tiny_wasm_runtime::{Timer, WasmRuntimeAsyncEngine};

struct StoreWakerThenReady {
    stored_waker: Arc<Mutex<Option<Waker>>>,
}

impl StoreWakerThenReady {
    fn new(stored_waker: Arc<Mutex<Option<Waker>>>) -> Self {
        Self { stored_waker }
    }
}

impl Future for StoreWakerThenReady {
    type Output = ();

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        *self
            .stored_waker
            .lock()
            .expect("stored waker mutex should not be poisoned") = Some(cx.waker().clone());
        Poll::Ready(())
    }
}

struct NoisyCompletionFuture {
    pending_polls: usize,
    wakeups_per_poll: usize,
    checksum: u64,
}

impl NoisyCompletionFuture {
    fn new(pending_polls: usize, wakeups_per_poll: usize) -> Self {
        Self {
            pending_polls,
            wakeups_per_poll,
            checksum: 0,
        }
    }

    fn push_extra_wakeups(&self, cx: &Context<'_>) {
        for _ in 0..self.wakeups_per_poll {
            cx.waker().wake_by_ref();
        }
    }
}

impl Future for NoisyCompletionFuture {
    type Output = u64;

    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        self.checksum = self
            .checksum
            .wrapping_mul(1_664_525)
            .wrapping_add(1_013_904_223);
        self.push_extra_wakeups(cx);

        if self.pending_polls == 0 {
            return Poll::Ready(self.checksum);
        }

        self.pending_polls -= 1;
        Poll::Pending
    }
}

async fn test_block_on_return_value() {
    let result = WasmRuntimeAsyncEngine::block_on(async {
        println!("=== Block On Return Value Test Start ===");

        // Sleep a little to test timers
        Timer::sleep(Duration::from_millis(200)).await;
        println!("Slept 200ms.");

        // Spawn a task that returns something
        let handle = WasmRuntimeAsyncEngine::spawn(async {
            println!("[Spawned Task] Sleeping 100ms...");
            Timer::sleep(Duration::from_millis(100)).await;
            println!("[Spawned Task] Returning 999.");
            999
        });

        println!("wait for result");

        // Await the spawned task
        let spawned_result = handle.await;
        println!("[Main] Spawned task returned: {spawned_result}");

        // Compose a result
        let final_result = spawned_result + 1;

        println!("=== Block On Return Value Test Done ===");

        final_result
    });

    assert_eq!(result, 1000, "The final result should be 1000");
}

async fn test_block_on_drains_spawned_tasks_before_return() {
    let task_completed = Arc::new(AtomicBool::new(false));
    let task_completed_clone = Arc::clone(&task_completed);

    let result = WasmRuntimeAsyncEngine::block_on(async move {
        WasmRuntimeAsyncEngine::spawn(async move {
            Timer::sleep(Duration::from_millis(25)).await;
            task_completed_clone.store(true, Ordering::SeqCst);
        });

        7
    });

    assert_eq!(result, 7);
    assert!(
        task_completed.load(Ordering::SeqCst),
        "spawned tasks should still complete before block_on returns"
    );
}

async fn test_cancel_prevents_task_from_running() {
    let task_ran = Arc::new(AtomicBool::new(false));
    let task_ran_clone = Arc::clone(&task_ran);

    WasmRuntimeAsyncEngine::block_on(async move {
        let handle = WasmRuntimeAsyncEngine::spawn(async move {
            task_ran_clone.store(true, Ordering::SeqCst);
        });

        handle.cancel();
        Timer::sleep(Duration::from_millis(25)).await;
    });

    assert!(
        !task_ran.load(Ordering::SeqCst),
        "canceled tasks should not run after cancellation"
    );
}

async fn test_stale_waker_from_completed_future_does_not_break_next_run() {
    let stored_waker = Arc::new(Mutex::new(None));

    WasmRuntimeAsyncEngine::block_on(StoreWakerThenReady::new(Arc::clone(&stored_waker)));

    let stale_waker = stored_waker
        .lock()
        .expect("stored waker mutex should not be poisoned")
        .clone()
        .expect("completed future should have captured a waker");

    for _ in 0..256 {
        stale_waker.wake_by_ref();
    }

    let result = WasmRuntimeAsyncEngine::block_on(async { 42usize });
    assert_eq!(result, 42);
}

async fn test_block_on_handles_excess_wakeups_after_future_completes() {
    let checksum = WasmRuntimeAsyncEngine::block_on(async {
        Timer::sleep(Duration::from_millis(1)).await;
        NoisyCompletionFuture::new(4, 512).await
    });
    assert_ne!(checksum, 0, "noisy future should complete with a checksum");

    let follow_up = WasmRuntimeAsyncEngine::block_on(async { 7usize });
    assert_eq!(
        follow_up, 7,
        "follow-up runs should still succeed after draining stale wakeups"
    );
}

async fn test_dropped_join_handle_for_completed_future_does_not_block_shutdown() {
    let task_completed = Arc::new(AtomicBool::new(false));
    let task_completed_clone = Arc::clone(&task_completed);

    let result = WasmRuntimeAsyncEngine::block_on(async move {
        let handle = WasmRuntimeAsyncEngine::spawn(async move {
            let checksum = NoisyCompletionFuture::new(3, 256).await;
            task_completed_clone.store(true, Ordering::SeqCst);
            checksum
        });

        drop(handle);
        Timer::sleep(Duration::from_millis(10)).await;
        11usize
    });

    assert_eq!(result, 11);
    assert!(
        task_completed.load(Ordering::SeqCst),
        "dropping a join handle should not prevent the completed task from being drained"
    );
}

tiny_wasm_runtime::async_command! {
    macro_rules! run_test {
        ($name:ident) => {{
            println!("test {} ...", stringify!($name));
            $name().await;
            println!("test {} ... ok", stringify!($name));
        }};
    }

    run_test!(test_block_on_return_value);
    run_test!(test_block_on_drains_spawned_tasks_before_return);
    run_test!(test_cancel_prevents_task_from_running);
    run_test!(test_stale_waker_from_completed_future_does_not_break_next_run);
    run_test!(test_block_on_handles_excess_wakeups_after_future_completes);
    run_test!(test_dropped_join_handle_for_completed_future_does_not_block_shutdown);
}
