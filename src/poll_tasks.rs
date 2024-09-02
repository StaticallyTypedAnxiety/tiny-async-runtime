use std::{
    collections::HashMap,
    task::{Context, Poll},
};

use futures::Stream;

use crate::bindings::wasi::io::poll::{poll, Pollable};

///Future that is used to poll changes from the host\
#[derive(Default, Debug)]
pub struct PollTasks {
    pendings: HashMap<String, Pollable>,
}

impl PollTasks {
    pub(crate) fn push(&mut self, event_name: String, pollable: Pollable) {
        self.pendings.insert(event_name, pollable);
    }
}

impl Stream for PollTasks {
    type Item = Vec<String>;

    fn poll_next(self: std::pin::Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        let this = self.get_mut();
        if this.pendings.is_empty() {
            return Poll::Ready(None);
        }
        let pending_polls = this.pendings.values().collect::<Vec<_>>();
        poll(pending_polls.as_slice());
        let ready_set = this
            .pendings
            .iter()
            .filter(|(_, pollable)| pollable.ready())
            .map(|(key, _)| key.clone())
            .collect::<Vec<_>>();
        //remove pollables
        for key in ready_set.iter() {
            this.pendings.remove(key);
        }

        cx.waker().wake_by_ref();
        if !ready_set.is_empty() {
            return Poll::Ready(Some(ready_set));
        }

        std::task::Poll::Pending
    }
}
