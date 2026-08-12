# tiny-async-runtime

**tiny-async-runtime** is a minimal, WASI-compatible async runtime designed to run on WebAssembly with **WASI Preview 3**.

It provides:

- [x] Single-threaded cooperative task scheduling
- [x] Futures spawning and cancellation
- [x] Timeout support
- [x] Socket I/O integration
- [ ] File IO integration

This runtime is inspired by `mio` but is purpose-built for WASI environments.

## Running In The Right Environment

This crate is meant to be validated in a **WASI Preview 3** host, not as a plain native executable.

- The socket and clock support depends on Preview 3 (WASI 0.3.0) imports from `wit/world.wit`.
- Native host runs such as `cargo build`/`cargo check` on Windows or Linux do not provide those imports (they compile fine -- the extern imports are just stubbed to `unreachable!()` off-`wasm32` -- but calling them there panics).
- There is no `wasm32-wasip3` rustc target yet, so components still compile for `wasm32-wasip2` and run under a wasmtime built with WASI 0.3 support (**wasmtime 43+**; validated against 47.0.3), with `-S p3=y` passed to enable it.

### Why every runnable target here is a `cdylib` exporting `run`

WASI 0.3's `wasi:cli/command` world declares `run` as `async func`. Rust's
standard library doesn't know that yet (there's no `wasm32-wasip3` target),
so a plain `fn main()` still compiles down to the *old*, synchronous `run`
export. The Component Model forbids a sync-typed export from blocking on
`waitable-set.wait`, which is exactly what awaiting a WASI 0.3 import
(`Timer::sleep`, any socket call, etc.) requires -- so a normal `fn main()`
binary traps immediately with `cannot block a synchronous task before
returning` the moment it touches one.

A WASM component doesn't really have a "process" with a `fn main()` to begin
with -- it's a set of exported functions a host calls into, which is exactly
what a `cdylib` is. So every runnable target here (`[[example]]` in
`Cargo.toml`, all with `crate-type = ["cdylib"]`) exports the async `run`
itself instead of relying on std's `fn main()` glue, using the `wasi:cli/run`
bindings this crate generates -- either by hand:

```rust
use tiny_wasm_runtime::bindings;
use tiny_wasm_runtime::{Timer, WasmRuntimeAsyncEngine};

struct MyComponent;

impl bindings::exports::wasi::cli::run::Guest for MyComponent {
    async fn run() -> Result<(), ()> {
        WasmRuntimeAsyncEngine::block_on(async {
            Timer::sleep(std::time::Duration::from_millis(10)).await;
        });
        Ok(())
    }
}

bindings::export_command!(MyComponent);
```

or with the macros this crate provides for exactly this -- see Features
below.

Because `crate-type = ["cdylib"]` targets have no traditional entrypoint,
`cargo run --example` and `cargo test` don't work on them ("is a library and
cannot be executed" / nothing to discover). Build, then invoke wasmtime
directly:

```powershell
cargo build --target wasm32-wasip2 --examples

wasmtime -W component-model-async=y -S cli=y -S inherit-network=y -S tcp=y -S udp=y -S p3=y `
  target/wasm32-wasip2/debug/examples/basic_usage.wasm
```

Flag rationale:

| Flag | Why |
| --- | --- |
| `-W component-model-async=y` | The wasm-level async lifting/lowering opcodes `wit_bindgen`'s generated bindings use. Without it: `async component functions require the component model async feature`. |
| `-S p3=y` | The actual WASI 0.3 host implementations (`wasi:clocks`, `wasi:sockets`). Requires **wasmtime 43+**; validated against 47.0.3. |
| `-S cli=y` | stdout/stderr/environment for `println!`, panics, etc. |
| `-S inherit-network=y -S tcp=y -S udp=y` | Socket access. WASI 0.3 removed the `wasi:sockets` `network` capability resource, so this is granted unconditionally rather than gated behind an `instance-network()` call. |

`.cargo/config.toml` sets these same flags as the `wasm32-wasip2` runner, so
`cargo run --target wasm32-wasip2 --bin <name>` still works normally for any
plain (non-cdylib) `[[bin]]` target that doesn't touch WASI 0.3 async imports
-- e.g. `benchmarks/high_frequency_benchmark.rs`.

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

## Validated Examples

Runnable, wasmtime-verified examples and tests (all `crate-type = ["cdylib"]`
`[[example]]` targets -- see above for why `cargo run`/`cargo test` don't
apply and how to invoke them):

- `examples/basic_usage.rs` -- `block_on`, `spawn`, `Timer::sleep`, `Timer::timeout`.
- `examples/macro_main.rs` -- the `#[tiny_wasm_runtime::main]` macro.
- `examples/chat/` -- a socket-based chat demo (directory server + peers); see `examples/chat/README.md`.
- `tests/*.rs` -- registered as `test-engine`, `test-timers`, `test-macro-main`, `test-usage-example`, `test-network` (Cargo normalizes the hyphens to underscores in the actual `.wasm` filename, e.g. `test_engine.wasm`).

```powershell
cargo build --target wasm32-wasip2 --examples

wasmtime -W component-model-async=y -S cli=y -S inherit-network=y -S tcp=y -S udp=y -S p3=y `
  target/wasm32-wasip2/debug/examples/test_engine.wasm
```

`benchmarks/high_frequency_benchmark.rs` is a plain (non-cdylib) `[[bin]]`
that doesn't touch WASI 0.3 async imports, so it needs none of this --
`cargo run --target wasm32-wasip2 --bin high-frequency-benchmark` works
normally.

