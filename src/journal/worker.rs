use std::collections::VecDeque;
use std::sync::{Condvar, Mutex, MutexGuard};
use std::time::{Duration, Instant};

use super::{Config, Entry, MAX_SERVER_BATCH};

#[derive(Debug, Default)]
pub(super) struct State {
    pub(super) buffer: VecDeque<Entry>,
    pub(super) dropped: usize,
    pub(super) retry_at: Option<Instant>,
    kicked: bool,
    stopping: bool,
}

#[derive(Debug)]
pub struct Shared {
    pub(super) config: Config,
    pub(super) agent: ureq::Agent,
    state: Mutex<State>,
    wake: Condvar,
}

impl Shared {
    pub fn new(mut config: Config) -> Self {
        config.max_batch = config.max_batch.clamp(1, MAX_SERVER_BATCH);
        config.buffer_cap = config.buffer_cap.max(1);
        if config.flush_interval.is_zero() {
            config.flush_interval = Duration::from_secs(2);
        }
        let agent = ureq::Agent::config_builder()
            .http_status_as_error(false)
            .timeout_global(Some(Duration::from_secs(10)))
            .build()
            .into();
        Self { config, agent, state: Mutex::new(State::default()), wake: Condvar::new() }
    }

    pub fn app(&self) -> &str {
        &self.config.app
    }

    pub(super) fn lock(&self) -> MutexGuard<'_, State> {
        self.state.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    pub fn dropped(&self) -> usize {
        self.lock().dropped
    }

    pub fn enqueue(&self, entry: Entry) {
        let mut state = self.lock();
        if state.buffer.len() >= self.config.buffer_cap {
            state.buffer.pop_front();
            state.dropped += 1;
        }
        state.buffer.push_back(entry);
        if state.buffer.len() >= self.config.max_batch {
            state.kicked = true;
            self.wake.notify_one();
        }
    }

    pub fn stop(&self) {
        self.lock().stopping = true;
        self.wake.notify_one();
    }

    pub fn run(&self) {
        loop {
            let stopping = {
                let state = self.lock();
                let (mut state, _) = self
                    .wake
                    .wait_timeout_while(state, self.config.flush_interval, |s| !s.kicked && !s.stopping)
                    .unwrap_or_else(|poisoned| poisoned.into_inner());
                state.kicked = false;
                state.stopping
            };
            if stopping {
                return self.drain();
            }
            self.flush(false);
        }
    }

    fn drain(&self) {
        let rounds = self.config.buffer_cap / MAX_SERVER_BATCH + 1;
        for _ in 0..=rounds {
            if !self.flush(true) {
                break;
            }
        }
        let mut state = self.lock();
        state.dropped += state.buffer.len();
        state.buffer.clear();
    }
}
