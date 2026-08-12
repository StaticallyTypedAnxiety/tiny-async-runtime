use std::time::Duration;

use tiny_wasm_runtime::{Timer, WasmRuntimeAsyncEngine};

async fn basic_usage_example_flow() {
    let (worker_result, fast_result, timeout_kind) = WasmRuntimeAsyncEngine::block_on(async {
        let worker = WasmRuntimeAsyncEngine::spawn(async {
            Timer::sleep(Duration::from_millis(30)).await;
            40
        });

        let fast_result = Timer::timeout(
            async {
                Timer::sleep(Duration::from_millis(10)).await;
                "quick-task"
            },
            Duration::from_millis(100),
        )
        .await
        .expect("quick task should finish before the timeout");

        let timeout_kind = Timer::timeout(
            async {
                Timer::sleep(Duration::from_millis(50)).await;
                "slow-task"
            },
            Duration::from_millis(5),
        )
        .await
        .expect_err("slow task should time out")
        .kind();

        let worker_result = worker.await;

        (worker_result, fast_result, timeout_kind)
    });

    assert_eq!(worker_result, 40);
    assert_eq!(fast_result, "quick-task");
    assert_eq!(timeout_kind, std::io::ErrorKind::TimedOut);
}

tiny_wasm_runtime::async_command! {
    println!("test basic_usage_example_flow ...");
    basic_usage_example_flow().await;
    println!("test basic_usage_example_flow ... ok");
}
