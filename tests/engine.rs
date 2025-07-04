use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use tiny_wasm_runtime::{Timer, WasmRuntimeAsyncEngine};

pub async fn test_timers_with_assertions() {
    let task_a_done: Arc<AtomicBool> = Arc::new(AtomicBool::new(false));
    let task_b_done = Arc::new(AtomicBool::new(false));

    let task_a_done_clone = task_a_done.clone();
    let task_b_done_clone = task_b_done.clone();
    println!("spaw goes here");
    WasmRuntimeAsyncEngine::spawn::<(), _>(async move {
        println!("sleep happens here");
        Timer::sleep(std::time::Duration::from_secs(1)).await;
        task_a_done_clone.store(true, Ordering::SeqCst);
    });

    println!("spaw goes here 2");

    WasmRuntimeAsyncEngine::spawn::<(), _>(async move {
        Timer::sleep(std::time::Duration::from_millis(500)).await;
        task_b_done_clone.store(true, Ordering::SeqCst);
    });

    // Do main timeout
    let slow_future = async {
        Timer::sleep(std::time::Duration::from_secs(2)).await;
        "completed"
    };

    let res = Timer::timeout(slow_future, std::time::Duration::from_secs(1)).await;

    assert!(res.is_err(), "Expected the main future to time out");
    assert!(
        task_a_done.load(Ordering::SeqCst) || task_b_done.load(Ordering::SeqCst),
        "At least one background task should have completed by now"
    );
}

#[test]
fn test_full_engine_runtime() {
    WasmRuntimeAsyncEngine::block_on(async {
        test_timers_with_assertions().await;
    });
}
