//! Waiting for a conversation's memory to finish building.
//!
//! Appending a message stores it immediately, but the facts drawn from it are
//! extracted in the background, so a just-appended message is not searchable
//! yet. `--wait` polls `cook-status` until the conversation reports finished.
//!
//! Two properties of that endpoint shape the rules here. It is **conversation
//! scoped**, not message scoped, so waiting means "this conversation has
//! nothing left in flight", not "my message specifically is done" — a
//! concurrent writer can keep it unfinished. And the API documents that not
//! every conversation reaches a finished state, so the wait is always bounded
//! by a timeout rather than looping forever.

use std::time::{Duration, Instant};

use anyhow::{Context, Result};
use memorylake_core::Client;
use memorylake_core::api::conversations::get_cook_status;

/// Seconds `--wait` polls before giving up.
///
/// Matches `project document import --wait`, which faces the same
/// asynchronous-processing problem.
pub const DEFAULT_WAIT_TIMEOUT_SECS: u64 = 600;

/// Gap before the second poll; doubles after that.
const INITIAL_POLL_DELAY: Duration = Duration::from_secs(1);

/// Longest gap between polls.
const MAX_POLL_DELAY: Duration = Duration::from_secs(15);

/// What the wait loop needs from the outside world.
///
/// Injected so the polling rules can be tested without a network and without
/// spending the real seconds the backoff schedule describes.
pub trait CookPoller {
    /// Whether the conversation's memory is up to date with its messages.
    fn cook_finished(&self) -> Result<bool>;
    /// Pause before the next round.
    fn sleep(&self, delay: Duration);
    /// Time spent waiting so far.
    fn elapsed(&self) -> Duration;
}

/// Polls the real `cook-status` endpoint.
pub struct ApiCookPoller<'a> {
    client: &'a Client,
    workspace: &'a str,
    conversation: &'a str,
    start: Instant,
}

impl<'a> ApiCookPoller<'a> {
    /// Start the clock and poll `conversation` inside `workspace`.
    pub fn new(client: &'a Client, workspace: &'a str, conversation: &'a str) -> Self {
        Self {
            client,
            workspace,
            conversation,
            start: Instant::now(),
        }
    }
}

impl CookPoller for ApiCookPoller<'_> {
    fn cook_finished(&self) -> Result<bool> {
        let status = get_cook_status(self.client, self.workspace, self.conversation)
            .with_context(|| format!("poll cook status of conversation `{}`", self.conversation))?;
        Ok(status.cook_finished)
    }

    fn sleep(&self, delay: Duration) {
        std::thread::sleep(delay);
    }

    fn elapsed(&self) -> Duration {
        self.start.elapsed()
    }
}

/// How waiting ended.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WaitOutcome {
    /// The conversation reported its memory finished.
    Finished,
    /// The timeout elapsed first. Processing continues server-side.
    TimedOut,
}

/// Poll until the conversation reports finished or `timeout` passes.
///
/// The first poll happens immediately, so an already-finished conversation
/// costs one request and no delay.
pub fn wait_for_cook(ctx: &dyn CookPoller, timeout: Duration) -> Result<WaitOutcome> {
    let mut delay = INITIAL_POLL_DELAY;
    loop {
        if ctx.cook_finished()? {
            return Ok(WaitOutcome::Finished);
        }
        // Checked after the poll, so a timeout of zero still asks once rather
        // than reporting a timeout without ever having looked.
        if ctx.elapsed() >= timeout {
            return Ok(WaitOutcome::TimedOut);
        }
        ctx.sleep(delay);
        delay = (delay * 2).min(MAX_POLL_DELAY);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::{Cell, RefCell};

    /// A poller answering from a script, with a clock that only the recorded
    /// sleeps advance.
    struct FakePoller {
        answers: RefCell<Vec<bool>>,
        slept: RefCell<Vec<Duration>>,
        elapsed: Cell<Duration>,
        polls: Cell<usize>,
    }

    impl FakePoller {
        fn new(answers: &[bool]) -> Self {
            Self {
                answers: RefCell::new(answers.to_vec()),
                slept: RefCell::new(Vec::new()),
                elapsed: Cell::new(Duration::ZERO),
                polls: Cell::new(0),
            }
        }
    }

    impl CookPoller for FakePoller {
        fn cook_finished(&self) -> Result<bool> {
            self.polls.set(self.polls.get() + 1);
            let mut answers = self.answers.borrow_mut();
            if answers.is_empty() {
                return Ok(false);
            }
            Ok(answers.remove(0))
        }

        fn sleep(&self, delay: Duration) {
            self.slept.borrow_mut().push(delay);
            self.elapsed.set(self.elapsed.get() + delay);
        }

        fn elapsed(&self) -> Duration {
            self.elapsed.get()
        }
    }

    #[test]
    fn an_already_finished_conversation_costs_one_poll_and_no_sleep() {
        let poller = FakePoller::new(&[true]);
        let outcome = wait_for_cook(&poller, Duration::from_secs(600)).expect("wait");
        assert_eq!(outcome, WaitOutcome::Finished);
        assert_eq!(poller.polls.get(), 1);
        assert!(
            poller.slept.borrow().is_empty(),
            "no delay before the answer"
        );
    }

    #[test]
    fn waiting_ends_as_soon_as_the_conversation_reports_finished() {
        let poller = FakePoller::new(&[false, false, true]);
        let outcome = wait_for_cook(&poller, Duration::from_secs(600)).expect("wait");
        assert_eq!(outcome, WaitOutcome::Finished);
        assert_eq!(poller.polls.get(), 3);
    }

    #[test]
    fn delays_double_up_to_the_ceiling() {
        // Never finishes; a generous timeout lets the schedule run long enough
        // to reach and hold the cap.
        let poller = FakePoller::new(&[]);
        let outcome = wait_for_cook(&poller, Duration::from_secs(120)).expect("wait");
        assert_eq!(outcome, WaitOutcome::TimedOut);
        let slept = poller.slept.borrow();
        assert_eq!(
            &slept[..5],
            &[
                Duration::from_secs(1),
                Duration::from_secs(2),
                Duration::from_secs(4),
                Duration::from_secs(8),
                Duration::from_secs(15),
            ],
            "1s doubling, clamped at 15s: {slept:?}"
        );
        assert!(
            slept.iter().all(|delay| *delay <= MAX_POLL_DELAY),
            "no delay exceeds the ceiling: {slept:?}"
        );
    }

    #[test]
    fn a_conversation_that_never_finishes_times_out() {
        let poller = FakePoller::new(&[false, false]);
        let outcome = wait_for_cook(&poller, Duration::from_secs(3)).expect("wait");
        assert_eq!(outcome, WaitOutcome::TimedOut);
        assert!(
            poller.elapsed() >= Duration::from_secs(3),
            "the loop ran until the deadline: {:?}",
            poller.elapsed()
        );
    }

    #[test]
    fn a_zero_timeout_still_asks_once() {
        // Otherwise `--timeout 0` would report a timeout on a conversation
        // that was already finished.
        let poller = FakePoller::new(&[true]);
        assert_eq!(
            wait_for_cook(&poller, Duration::ZERO).expect("wait"),
            WaitOutcome::Finished
        );
        assert_eq!(poller.polls.get(), 1);

        let poller = FakePoller::new(&[false]);
        assert_eq!(
            wait_for_cook(&poller, Duration::ZERO).expect("wait"),
            WaitOutcome::TimedOut
        );
        assert_eq!(poller.polls.get(), 1, "no second poll after giving up");
    }

    #[test]
    fn a_polling_error_stops_the_wait() {
        struct Failing;
        impl CookPoller for Failing {
            fn cook_finished(&self) -> Result<bool> {
                anyhow::bail!("network down")
            }
            fn sleep(&self, _: Duration) {
                unreachable!("a failed poll must not lead to a sleep")
            }
            fn elapsed(&self) -> Duration {
                Duration::ZERO
            }
        }
        let err = wait_for_cook(&Failing, Duration::from_secs(600)).expect_err("must fail");
        assert!(err.to_string().contains("network down"), "{err}");
    }
}
