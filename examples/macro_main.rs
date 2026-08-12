use std::time::Duration;

use tiny_wasm_runtime::{Timer, WasmRuntimeAsyncEngine};

#[tiny_wasm_runtime::main]
async fn main() {
    let handle = WasmRuntimeAsyncEngine::spawn(async {
        Timer::sleep(Duration::from_millis(25)).await;
        12
    });

    let quick_result = Timer::timeout(
        async {
            Timer::sleep(Duration::from_millis(5)).await;
            "macro-ready"
        },
        Duration::from_millis(100),
    )
    .await
    .expect("timeout should not trigger");

    let value = handle.await;
    println!("macro example finished successfully: {quick_result}:{value}");
}
