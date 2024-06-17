use std::task::{Context, Poll};

use futures::Future;

use crate::bindings::wasi::io::poll::{poll, Pollable};

///Future that is used to poll changes from the host
pub struct PollTasks {
    pollables: Vec<Pollable>,
    processed_polls: Vec<u32>,
}

impl PollTasks {
    pub(crate) fn new(pollables: Vec<Pollable>) -> Self {
        Self {
            processed_polls: Vec::with_capacity(pollables.len()),
            pollables,
        }
    }
}

impl Future for PollTasks {
    type Output = Vec<u32>;
    fn poll(self: std::pin::Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        let pollables = self.get_mut();
        let pollable_results = pollables.pollables.iter().collect::<Vec<_>>();
        let results_vec = poll(pollable_results.as_slice());
        //take all the polls that are not needed away
        for index in results_vec {
            //if processed remove it from process queue and add it to processed polls
            let _ = pollables.pollables.remove(index as usize);
            pollables.processed_polls.push(index);
        }

        //if the pollable set is empty that means we have finished processing everything
        if pollables.pollables.is_empty() {
            Poll::Ready(pollables.processed_polls.clone())
        } else {
            //still more polls that might need to get processed for a later date
            cx.waker().wake_by_ref();
            Poll::Pending
        }
    }
}
