use crate::bindings::wasi::{clocks::monotonic_clock::{subscribe_duration, subscribe_instant}, io::poll::Pollable};
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

    pub fn subscribe_instant(duration:std::time::Duration) -> Self{
        Self { pollable: subscribe_instant(duration.as_nanos() as u64) }
    }
}
impl From<Timer> for Pollable {
    fn from(value: Timer) -> Self {
        value.pollable
    }
}
