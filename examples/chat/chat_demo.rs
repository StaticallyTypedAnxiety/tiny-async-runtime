#[path = "common.rs"]
mod common;

use std::time::Duration;

use common::{
    run_chat_initiator, run_chat_listener, run_directory_server, send_directory_shutdown,
    session_log_path, DirectoryServerConfig, InitiatorConfig, ListenerConfig,
    DEFAULT_DIRECTORY_BIND_HOST, DEFAULT_DIRECTORY_HOST, DEFAULT_DIRECTORY_PORT,
};
use tiny_wasm_runtime::Timer;
use tiny_wasm_runtime::WasmRuntimeAsyncEngine;

#[tiny_wasm_runtime::main]
async fn main() {
    let directory_server =
        WasmRuntimeAsyncEngine::spawn(run_directory_server(DirectoryServerConfig::new(
            DEFAULT_DIRECTORY_BIND_HOST,
            DEFAULT_DIRECTORY_PORT,
            "directory-server.log",
            None,
        )));

    Timer::sleep(Duration::from_millis(250)).await;

    let bob_listener = WasmRuntimeAsyncEngine::spawn(run_chat_listener(ListenerConfig {
        user: "bob".to_string(),
        bind_host: DEFAULT_DIRECTORY_BIND_HOST.to_string(),
        advertised_host: DEFAULT_DIRECTORY_HOST.to_string(),
        port: 64_100,
        directory_host: DEFAULT_DIRECTORY_HOST.to_string(),
        directory_port: DEFAULT_DIRECTORY_PORT,
        reply_messages: vec!["bob here".to_string(), "session complete".to_string()],
        hold_duration: Duration::from_millis(1_250),
        max_sessions: 2,
    }));

    Timer::sleep(Duration::from_millis(350)).await;

    let alice = WasmRuntimeAsyncEngine::spawn(run_chat_initiator(InitiatorConfig {
        user: "alice".to_string(),
        target_user: "bob".to_string(),
        directory_host: DEFAULT_DIRECTORY_HOST.to_string(),
        directory_port: DEFAULT_DIRECTORY_PORT,
        message_script: vec![
            "hello bob".to_string(),
            "alice reached you first".to_string(),
        ],
        wait_for_availability: false,
        lookup_interval: Duration::from_millis(200),
        retry_connect_delay: Duration::from_millis(150),
    }));

    Timer::sleep(Duration::from_millis(100)).await;

    let dave = WasmRuntimeAsyncEngine::spawn(run_chat_initiator(InitiatorConfig {
        user: "dave".to_string(),
        target_user: "bob".to_string(),
        directory_host: DEFAULT_DIRECTORY_HOST.to_string(),
        directory_port: DEFAULT_DIRECTORY_PORT,
        message_script: vec![
            "dave is waiting in line".to_string(),
            "thanks for becoming available".to_string(),
        ],
        wait_for_availability: true,
        lookup_interval: Duration::from_millis(250),
        retry_connect_delay: Duration::from_millis(150),
    }));

    let alice_session = alice.await.expect("alice session should complete");
    let dave_session = dave.await.expect("dave session should complete");

    bob_listener
        .await
        .expect("bob listener should complete its sessions");

    send_directory_shutdown(DEFAULT_DIRECTORY_HOST, DEFAULT_DIRECTORY_PORT)
        .await
        .expect("directory shutdown should succeed");

    directory_server
        .await
        .expect("directory server should finish shutdown");

    println!(
        "chat demo complete. session logs: {}, {}",
        session_log_path(&alice_session).display(),
        session_log_path(&dave_session).display()
    );
}
