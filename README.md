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



