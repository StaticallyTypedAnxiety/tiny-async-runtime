use crate::bindings::wasi::{clocks::monotonic_clock::subscribe_duration, io::poll::Pollable};
#[derive(Debug)]
pub struct Timer {
    pollable: Pollable,
}

impl Timer {
    /// create a timer that resolves once it elapses
    pub fn subscribe_duration(duration: std::time::Duration) -> Self {
        Timer {
            pollable: subscribe_duration(duration.as_nanos() as u64),
        }
    }
}
impl From<Timer> for Pollable {
    fn from(value: Timer) -> Self {
        value.pollable
    }
}
