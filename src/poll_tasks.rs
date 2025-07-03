use std::{
    collections::HashMap,
    sync::Arc,
};

use crate::bindings::wasi::io::poll::{poll, Pollable};

///Future that is used to poll changes from the host\
#[derive(Default, Debug)]
pub struct PollTasks {
    pendings: HashMap<String, Arc<Pollable>>,
    finished: HashMap<String, Arc<Pollable>>,
}

impl PollTasks {
    pub(crate) fn push(&mut self, event_name: String, pollable: Arc<Pollable>) {
        self.pendings.insert(event_name, pollable);
    }

    pub(crate) fn contains(&self, key: &str) -> bool {
        self.pendings.contains_key(key)
    }

    pub(crate) fn check_if_ready(&mut self, key: &str) -> bool {
        self.finished.remove(key).is_some()
    }

    pub(crate) fn wait_for_pollables(&mut self) {
        if self.pendings.is_empty() {
            return;
        }
        let pending_polls = self
            .pendings
            .values()
            .map(|pollable| pollable.as_ref())
            .collect::<Vec<_>>();
        poll(pending_polls.as_slice());
        let ready_set = self
            .pendings
            .iter()
            .filter(|(_, pollable)| pollable.ready())
            .map(|(key, _)| key.clone())
            .collect::<Vec<_>>();
        //remove pollables
        for key in ready_set.iter() {
            if let Some(finished) = self.pendings.remove(key) {
                self.finished.insert(key.to_string(), finished);
            }
        }
    }
}

