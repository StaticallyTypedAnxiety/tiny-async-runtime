use std::{collections::HashMap, sync::Arc, task::Waker};

use crate::bindings::wasi::io::poll::{poll, Pollable};

pub type EventWithWaker<T> = (T, Waker);

///Future that is used to poll changes from the host\
#[derive(Default, Debug)]
pub struct PollTasks {
    pendings: HashMap<String, EventWithWaker<Arc<Pollable>>>,
    finished: HashMap<String, Arc<Pollable>>,
}

impl PollTasks {
    pub(crate) fn push(&mut self, event_name: String, pollable: EventWithWaker<Arc<Pollable>>) {
        self.pendings.insert(event_name, pollable);
    }

    pub(crate) fn contains(&self, key: &str) -> bool {
        self.pendings.contains_key(key)
    }

    pub(crate) fn check_if_ready(&mut self, key: &str) -> bool {
        self.finished.remove(key).is_some()
    }

    pub(crate) fn is_empty(&self) -> bool {
        self.finished.is_empty() && self.pendings.is_empty()
    }

    pub(crate) fn wait_for_pollables(&mut self) {
        if self.pendings.is_empty() {
            return;
        }
        let pending_polls = self
            .pendings
            .values()
            .map(|(pollable, _)| pollable.as_ref())
            .collect::<Vec<_>>();
        poll(pending_polls.as_slice());
        let ready_set = self
            .pendings
            .iter()
            .filter(|(_, (pollable, _))| pollable.ready())
            .map(|(key, _)| key.clone())
            .collect::<Vec<_>>();
        //remove pollables
        for key in ready_set.iter() {
            if let Some((finished, waker)) = self.pendings.remove(key) {
                waker.wake();
                self.finished.insert(key.to_string(), finished);
            }
        }
    }
}
