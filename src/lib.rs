//! A minimal, single-threaded async runtime for WASI Preview 3 components.
//!
//! Built on `wit-bindgen`'s native async support (no more Preview 2
//! pollable/poll loop), with task spawning/cancellation, timers, and TCP
//! sockets layered on top.
//!
//! - WASI 0.3: <https://wasi.dev/releases/wasi-p3>
//! - Component Model: <https://component-model.bytecodealliance.org/>

extern crate self as tiny_wasm_runtime;

pub mod bindings;
pub mod engine;
pub mod io;
pub use engine::WasmRuntimeAsyncEngine;
pub use io::timer::Timer;
pub use tiny_wasm_runtime_macros::{async_command, main};
