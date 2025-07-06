# tiny-async-runtime

**tiny-async-runtime** is a minimal, WASI-compatible async runtime designed to run on WebAssembly with **WASI Preview 2**.  
It provides:

✅ Single-threaded cooperative task scheduling  
✅ Futures spawning and cancellation  
✅ Timeout support  
✅ (Planned) socket I/O integration  

This runtime is inspired by `mio` but is purpose-built for WASI environments.


## Features

- **`block_on()`**  
  Runs an async function to completion, driving timers, I/O readiness, and spawned tasks.

- **`spawn()`**  
  Launches a future in the runtime. Returns a `JoinHandle` for cancellation or awaiting completion.

- **Timers**
  - `Timer::sleep(duration)` creates a future that resolves after a given time.
  - Timer futures integrate into the same event loop.

- **Cancellation**
  - Calling `JoinHandle::cancel()` removes the task from the scheduler.
  - Timers and sockets are left in a completed state (future enhancement: automatic cleanup).

- **Partial Support for Sockets**
   - At this point sockets can only connect but planned support is coming to the future

## Example

Here’s a minimal example using `block_on` and `spawn`:

```rust
use tiny_async_runtime::{WasmRuntimeAsyncEngine, Timer};

fn main() {
    WasmRuntimeAsyncEngine::block_on(async {
        // Spawn a background task
        let handle = WasmRuntimeAsyncEngine::spawn(async {
            println!("Background task sleeping...");
            Timer::sleep(std::time::Duration::from_secs(1)).await;
            println!("Background task done.");
            42
        });

        // Wait for the result
        let result = handle.await;
        println!("Background task returned: {result}");

        // Sleep in main
        Timer::sleep(std::time::Duration::from_millis(500)).await;
        println!("Main task done.");
    });
}

