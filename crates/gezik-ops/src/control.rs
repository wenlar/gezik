//! A job's switches and counters, shared by its threads and the UI.

use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering};
use std::sync::{Condvar, Mutex};
use std::time::Duration;

use gezik_core::ops::conflict::Decision;

use crate::engine::{PauseReason, lock};
use crate::task::Answer;

#[derive(Default)]
pub(crate) struct Control {
    cancel: AtomicBool,
    paused: Mutex<Option<PauseReason>>,
    resumed: Condvar,
    decisions: Mutex<Option<Vec<Decision>>>,
    decided: Condvar,
    /// The id of the last question asked, and its answer once given.
    answer: Mutex<(u64, Option<Answer>)>,
    answered: Condvar,
    /// Held while an item asks: one question at a time.
    pub asking_turn: Mutex<()>,
    pub start_now: AtomicBool,
    pub running: AtomicBool,
    pub scanning: AtomicBool,
    pub deciding: AtomicBool,
    /// Waiting for the answer to a question.
    pub asking: AtomicBool,
    pub items_done: AtomicU64,
    pub items_total: AtomicU64,
    pub bytes_done: AtomicU64,
    pub bytes_total: AtomicU64,
    failures_in_row: AtomicU32,
}

/// How long a waiting thread sleeps before it looks at the cancel switch again.
const RECHECK: Duration = Duration::from_millis(100);

impl Control {
    pub fn cancel(&self) {
        self.cancel.store(true, Ordering::SeqCst);
        self.resumed.notify_all();
        self.decided.notify_all();
        self.answered.notify_all();
    }

    pub fn cancelled(&self) -> bool {
        self.cancel.load(Ordering::SeqCst)
    }

    pub fn pause(&self, reason: PauseReason) {
        *lock(&self.paused) = Some(reason);
    }

    pub fn resume(&self) {
        *lock(&self.paused) = None;
        self.resumed.notify_all();
    }

    pub fn pause_reason(&self) -> Option<PauseReason> {
        *lock(&self.paused)
    }

    /// Whether the job is paused now; unlike `stopped`, it does not wait.
    pub fn paused(&self) -> bool {
        lock(&self.paused).is_some()
    }

    /// Waits while paused; true once cancelled.
    pub fn stopped(&self) -> bool {
        let mut paused = lock(&self.paused);
        while paused.is_some() && !self.cancelled() {
            paused = match self.resumed.wait_timeout(paused, RECHECK) {
                Ok((guard, _)) => guard,
                Err(poisoned) => poisoned.into_inner().0,
            };
        }
        self.cancelled()
    }

    pub fn set_decisions(&self, decisions: Vec<Decision>) {
        *lock(&self.decisions) = Some(decisions);
        self.decided.notify_all();
    }

    /// Waits for the user's decisions; `None` if the job is cancelled meanwhile.
    pub fn wait_decisions(&self) -> Option<Vec<Decision>> {
        let mut decisions = lock(&self.decisions);
        loop {
            if let Some(chosen) = decisions.take() {
                return Some(chosen);
            }
            if self.cancelled() {
                return None;
            }
            decisions = match self.decided.wait_timeout(decisions, RECHECK) {
                Ok((guard, _)) => guard,
                Err(poisoned) => poisoned.into_inner().0,
            };
        }
    }

    /// The answer to question `id`; one to an earlier question (late, or given twice) is
    /// dropped.
    pub fn set_answer(&self, id: u64, answer: Answer) {
        let mut current = lock(&self.answer);
        if current.0 == id {
            current.1 = Some(answer);
            self.answered.notify_all();
        }
    }

    /// A new question's id; an answer left from an earlier one is dropped.
    pub fn new_question(&self) -> u64 {
        let mut current = lock(&self.answer);
        current.0 += 1;
        current.1 = None;
        current.0
    }

    /// Waits for the user's answer; `None` if the job is cancelled meanwhile.
    pub fn wait_answer(&self) -> Option<Answer> {
        let mut answer = lock(&self.answer);
        loop {
            if let Some(given) = answer.1.take() {
                return Some(given);
            }
            if self.cancelled() {
                return None;
            }
            answer = match self.answered.wait_timeout(answer, RECHECK) {
                Ok((guard, _)) => guard,
                Err(poisoned) => poisoned.into_inner().0,
            };
        }
    }

    pub fn add_total(&self, items: u64, bytes: u64) {
        self.items_total.fetch_add(items, Ordering::Relaxed);
        self.bytes_total.fetch_add(bytes, Ordering::Relaxed);
    }

    pub fn add_bytes(&self, bytes: u64) {
        self.bytes_done.fetch_add(bytes, Ordering::Relaxed);
    }

    /// Takes back bytes counted for work that is done again (an item started over).
    pub fn take_back_bytes(&self, bytes: u64) {
        let _ =
            self.bytes_done.try_update(Ordering::Relaxed, Ordering::Relaxed, |done| Some(done.saturating_sub(bytes)));
    }

    pub fn item_done(&self) {
        self.items_done.fetch_add(1, Ordering::Relaxed);
    }

    /// One more failure in a row; returns how many.
    pub fn failed_once(&self) -> u32 {
        self.failures_in_row.fetch_add(1, Ordering::SeqCst) + 1
    }

    pub fn succeeded(&self) {
        self.failures_in_row.store(0, Ordering::SeqCst);
    }
}
