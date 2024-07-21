use std::{
    collections::{BTreeMap},
    task::{Context, Poll},
};

use futures::Stream;

use crate::bindings::wasi::io::poll::{poll, Pollable};

///Future that is used to poll changes from the host\
#[derive(Default)]
pub struct PollTasks {
    pollables: BTreeMap<String, Pollable>,
    processed_polls: Vec<String>,
}

impl PollTasks {
    pub(crate) fn push(&mut self, event_name: String, pollable: Pollable) {
        self.pollables.insert(event_name, pollable);
    }
}

impl Stream for PollTasks {
    type Item = Vec<String>;

    fn poll_next(self: std::pin::Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        let pollables = self.get_mut();
        let pollable_results = pollables.pollables.values().collect::<Vec<_>>();
        let results_vec = poll(pollable_results.as_slice());
        //take all the polls that are not needed away
        for (_, key) in results_vec.into_iter().zip(pollables.pollables.keys()) {
            //if processed remove it from process queue and add it to processed polls
            pollables.processed_polls.push(key.clone());
        }

        //remove pollables
        for key in pollables.processed_polls.iter() {
            pollables.pollables.remove(key);
        }
        //if the pollable set is empty that means we have finished processing everything
        if pollables.pollables.is_empty() {
            Poll::Ready(None)
        } else {
            cx.waker().wake_by_ref();
            if pollables.processed_polls.is_empty() {
                std::task::Poll::Pending
            } else {
                Poll::Ready(Some(pollables.processed_polls.clone()))
            }
        }
    }
}
