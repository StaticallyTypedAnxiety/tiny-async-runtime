#[allow(warnings)]
mod bindings;
use bindings::Guest;
use std::time::Duration;
struct Component;
impl Guest for Component {
    /// Say hello!
    fn run_timer_example() {
        wasm_runtime::WasmRuntimeAsyncEngine::block_on(|mut reactor| async move {
            let mut add_to_reactor = |subscription_name: String, duration: Duration| {
                reactor.add_to_queue(
                    subscription_name,
                    wasm_runtime::io::Timer::subscribe_duration(duration),
                );
            };
            [
                ("timer1", Duration::from_secs(5)),
                ("timer2", Duration::from_secs(2)),
                ("timer3", Duration::from_secs(7)),
            ]
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
