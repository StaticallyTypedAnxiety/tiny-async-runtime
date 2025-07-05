use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;
use tiny_wasm_runtime::{Timer, WasmRuntimeAsyncEngine};

#[test]
fn test_block_on_return_value() {
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
