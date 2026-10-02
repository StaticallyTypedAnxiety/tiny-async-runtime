# tiny-async-runtime

**tiny-async-runtime** is a minimal, WASI-compatible async runtime designed to run on WebAssembly with **WASI Preview 3**.

It provides:

- [x] Single-threaded cooperative task scheduling
- [x] Futures spawning and cancellation
- [x] Timeout support
- [x] Socket I/O integration

This runtime is inspired by `mio` but is purpose-built for WASI environments.

## Running In The Right Environment

This crate is meant to be validated in a **WASI Preview 3** host, not as a plain native executable.

- The socket and clock support depends on Preview 3 (WASI 0.3.0) imports from `wit/world.wit`.
- Native host runs such as `cargo build`/`cargo check` on Windows or Linux do not provide those imports (they compile fine -- the extern imports are just stubbed to `unreachable!()` off-`wasm32` -- but calling them there panics).
- There is no `wasm32-wasip3` rustc target yet, so components still compile for `wasm32-wasip2` and run under a wasmtime built with WASI 0.3 support (**wasmtime 43+**; validated against 47.0.3), with `-S p3=y` passed to enable it.

## Features

- **`block_on()`**
  Runs an async function to completion, driving timers, I/O readiness, and spawned tasks. Thin wrapper around `wit_bindgen::rt::async_support::block_on`.

- **`#[tiny_wasm_runtime::main]`**
  On `fn main`, exports the `Guest`/`export_command!` boilerplate for you, wrapping the body in `WasmRuntimeAsyncEngine::block_on(...)` -- see Example below. On any other function name, it just wraps that function's body in `block_on(...)` and leaves it as a plain synchronous function, similar to `tokio::main`; calling it is fine from within an already-async `Guest::run`, but don't nest it inside another `block_on` call (non-reentrant).

- **`tiny_wasm_runtime::async_command! { ... }`**
  Exports the `Guest`/`export_command!` boilerplate from a raw statement body, with **no** implicit `block_on` wrapper. Use this instead of `#[tiny_wasm_runtime::main]` on `fn main` when the body needs to call `block_on` itself one or more times -- an outer wrapper would nest a second, reentrant call on the same thread and deadlock. All five files under `tests/` use this, since each of their test functions calls `block_on` independently.

- **`spawn()`**
  Launches a future in the runtime (via `wit_bindgen`'s `spawn_local`, wrapped with a cancellation flag checked on every poll). Returns a `JoinHandle` for cancellation or awaiting completion.

- **Timers**
  - `Timer::sleep(duration)` awaits `wasi:clocks/monotonic-clock`'s `wait-for` import directly.
  - `Timer::timeout(fut, duration)` races a future against a timer.

- **Cancellation**
  - Calling `JoinHandle::cancel()` marks the task canceled; the cancellation flag is checked on every poll, so it takes effect whether the task hasn't started yet or is suspended mid-`.await`.
  - `block_on` doesn't return until all spawned tasks finish, even if the main future is already done -- dropping a `JoinHandle` doesn't cancel or detach its task.

- **Socket support**
  - `TcpStream`/`TCPListener` (`src/io/net.rs`) wrap `wasi:sockets/types@0.3.0`'s `tcp-socket` resource: `connect` is a native `async fn`, `send`/`receive` return `stream<u8>`/`future<result<...>>` pairs. Both `listen` and `receive`/`send` may only be called once per socket per the WIT contract, so the reader/writer/completion handles are created lazily and cached.

## Example

Here is a minimal example using `block_on` and `spawn`, exported as this crate's async `run` via `#[tiny_wasm_runtime::main]` on `fn main`:

```rust
use tiny_wasm_runtime::{Timer, WasmRuntimeAsyncEngine};

#[tiny_wasm_runtime::main]
async fn main() {
    let handle = WasmRuntimeAsyncEngine::spawn(async {
        Timer::sleep(std::time::Duration::from_secs(1)).await;
        42
    });

    let result = handle.await;
    println!("Background task returned: {result}");
}
```

This is equivalent to writing the `Guest`/`export_command!` boilerplate by
hand (see above), and is what `examples/basic_usage.rs` and
`examples/macro_main.rs` do.

The five files under `tests/` instead use `async_command!`, since their test
bodies each call `block_on` themselves and can't be wrapped in another one:

```rust
tiny_wasm_runtime::async_command! {
    println!("running a test...");
    some_async_test_fn().await;
}
```



