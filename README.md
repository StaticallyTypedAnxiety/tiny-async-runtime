# tiny-async-runtime

**tiny-async-runtime** is a minimal, WASI-compatible async runtime designed to run on WebAssembly with **WASI Preview 3**.

It provides:

- [x] Single-threaded cooperative task scheduling
- [x] Futures spawning and cancellation
- [x] Timeout support
- [x] Socket I/O integration

This runtime is inspired by `mio` but is purpose-built for WASI environments.

## Running In The Right Environment

This crate is meant to be validated in a **WASI Preview 3** host.

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

