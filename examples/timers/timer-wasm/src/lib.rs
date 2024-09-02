#[allow(warnings)]
mod bindings;
use crate::bindings::wasi::clocks::monotonic_clock::subscribe_duration;
use bindings::Guest;
struct Component;
impl Guest for Component {
    /// Say hello!
    fn run_timer_example() {
        wasm_runtime::WasmRuntimeAsyncEngine::block_on(|mut reactor| async move {
            let mut add_to_reactor = |subscription_name: String, duration: u64| {
                let time = subscribe_duration(duration);
                // I don't like the conversion between different types of polls and I need to restrcutre
                // timer should come from the crate itself
                let handle = time.take_handle();
                let new_handle = unsafe {
                    wasm_runtime::bindings::wasi::io::poll::Pollable::from_handle(handle)
                };
                reactor.add_to_queue(subscription_name, new_handle);
            };
            [("timer1", 1000000), ("timer2", 5000), ("timer3", 2000)]
                .iter()
                .for_each(|(duration_name, duration)| {
                    add_to_reactor(duration_name.to_string(), *duration);
                });
            while let Some(timers) = reactor.wait().await {
                timers.iter().for_each(|each_timer| {
                    println!("timer ready {each_timer}");
                })
            }
        });
    }
}

bindings::export!(Component with_types_in bindings);
