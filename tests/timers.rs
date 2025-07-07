use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;
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

#[test]
fn test_timeout_behavior() {
    WasmRuntimeAsyncEngine::block_on(async {
        println!("=== Timeout Behavior Test Start ===");

        //  This should complete successfully
        let fast_future = async {
            println!("[Fast Future] Sleeping 100ms...");
            Timer::sleep(Duration::from_millis(100)).await;
            println!("[Fast Future] Done!");
            "fast_result"
        };

        let result_fast = Timer::timeout(fast_future, Duration::from_secs(1)).await;

        assert!(
            result_fast.is_ok(),
            "Fast future should complete before timeout"
        );
        assert_eq!(result_fast.unwrap(), "fast_result");

        // This should time out
        let slow_future = async {
            println!("[Slow Future] Sleeping 2s...");
            Timer::sleep(Duration::from_secs(2)).await;
            println!("[Slow Future] Done!");
            "slow_result"
        };

        let result_slow = Timer::timeout(slow_future, Duration::from_millis(500)).await;

        assert!(result_slow.is_err(), "Slow future should time out");
        let err = result_slow.unwrap_err();
        assert_eq!(err.kind(), std::io::ErrorKind::TimedOut);

        // This should be right on the edge (expect to succeed)
        let edge_future = async {
            println!("[Edge Future] Sleeping 500ms...");
            Timer::sleep(Duration::from_millis(500)).await;
            println!("[Edge Future] Done!");
            "edge_result"
        };

        let result_edge = Timer::timeout(edge_future, Duration::from_millis(600)).await;

        assert!(
            result_edge.is_ok(),
            "Edge future should complete before timeout"
        );
        assert_eq!(result_edge.unwrap(), "edge_result");

        println!("=== Timeout Behavior Test Complete ===");
    });
}
