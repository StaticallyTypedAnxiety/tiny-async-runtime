use std::time::Duration;

use tiny_wasm_runtime as runtime;
use tiny_wasm_runtime::{Timer, WasmRuntimeAsyncEngine};

#[tiny_wasm_runtime::main]
async fn run_with_main_macro() -> usize {
    let background = WasmRuntimeAsyncEngine::spawn(async {
        Timer::sleep(Duration::from_millis(15)).await;
        3
    });

    let foreground = Timer::timeout(
        async {
            Timer::sleep(Duration::from_millis(5)).await;
            4
        },
        Duration::from_millis(50),
    )
    .await
    .expect("foreground future should finish");

    background.await + foreground
}

#[runtime::main(crate = "runtime")]
async fn run_with_renamed_runtime_path() -> usize {
    Timer::sleep(Duration::from_millis(5)).await;
    9
}

tiny_wasm_runtime::async_command! {
    println!("test main_macro_runs_async_functions ...");
    assert_eq!(run_with_main_macro(), 7);
    assert_eq!(run_with_renamed_runtime_path(), 9);
    println!("test main_macro_runs_async_functions ... ok");
}
