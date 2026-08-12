use std::time::Duration;

use tiny_wasm_runtime::{Timer, WasmRuntimeAsyncEngine};

#[tiny_wasm_runtime::main]
async fn main() {
    let worker = WasmRuntimeAsyncEngine::spawn(async {
        Timer::sleep(Duration::from_millis(30)).await;
        40
    });

    let status = Timer::timeout(
        async {
            Timer::sleep(Duration::from_millis(10)).await;
            "quick-task"
        },
        Duration::from_millis(100),
    )
    .await
    .expect("quick task should finish before the timeout");

    let worker_result = worker.await;

    let outcome = format!("{status}:{worker_result}");
    assert_eq!(outcome, "quick-task:40");
    println!("runtime example finished successfully: {outcome}");
}
