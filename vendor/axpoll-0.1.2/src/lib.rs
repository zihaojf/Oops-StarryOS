//! A library for polling I/O events and waking up tasks.

#![no_std]
#![deny(missing_docs)]

extern crate alloc;

use core::{
    task::{Context, Waker},
};

use alloc::{collections::VecDeque, sync::Arc, task::Wake};
use bitflags::bitflags;
use linux_raw_sys::general::*;
use spin::Mutex;

bitflags! {
    /// I/O events.
    #[derive(Debug, Clone, Copy)]
    pub struct IoEvents: u32 {
        /// Available for read
        const IN     = POLLIN;
        /// Urgent data for read
        const PRI    = POLLPRI;
        /// Available for write
        const OUT    = POLLOUT;

        /// Error condition
        const ERR    = POLLERR;
        /// Hang up
        const HUP    = POLLHUP;
        /// Invalid request
        const NVAL   = POLLNVAL;

        /// Equivalent to [`IN`](Self::IN)
        const RDNORM = POLLRDNORM;
        /// Priority band data can be read
        const RDBAND = POLLRDBAND;
        /// Equivalent to [`OUT`](Self::OUT)
        const WRNORM = POLLWRNORM;
        /// Priority data can be written
        const WRBAND = POLLWRBAND;

        /// Message
        const MSG    = POLLMSG;
        /// Remove
        const REMOVE = POLLREMOVE;
        /// Stream socket peer closed connection, or shut down writing half of connection.
        const RDHUP  = POLLRDHUP;

        /// Events that are always polled even without specifying them.
        const ALWAYS_POLL = Self::ERR.bits() | Self::HUP.bits();
    }
}

/// Trait for types that can be polled for I/O events.
pub trait Pollable {
    /// Polls for I/O events.
    fn poll(&self) -> IoEvents;

    /// Registers wakers for I/O events.
    fn register(&self, context: &mut Context<'_>, events: IoEvents);
}

const POLL_SET_CAPACITY: usize = 64;

#[derive(Default)]
struct Inner {
    first: Option<Waker>,
    others: VecDeque<Waker>,
}

impl Inner {
    fn register(&mut self, waker: &Waker) -> Option<Waker> {
        let Some(first) = &self.first else {
            self.first = Some(waker.clone());
            return None;
        };
        if first.will_wake(waker) || self.others.iter().any(|w| w.will_wake(waker)) {
            return None;
        }
        let evicted = if self.others.len() == POLL_SET_CAPACITY - 1 {
            core::mem::replace(&mut self.first, self.others.pop_front())
        } else {
            None
        };
        self.others.push_back(waker.clone());
        evicted
    }
}

/// A data structure for waking up tasks that are waiting for I/O events.
pub struct PollSet(Mutex<Inner>);

impl Default for PollSet {
    fn default() -> Self {
        Self::new()
    }
}

impl PollSet {
    /// Creates a new empty [`PollSet`].
    pub const fn new() -> Self {
        Self(Mutex::new(Inner {
            first: None,
            others: VecDeque::new(),
        }))
    }

    /// Registers a waker.
    pub fn register(&self, waker: &Waker) {
        let evicted = self.0.lock().register(waker);
        if let Some(w) = evicted {
            w.wake();
        }
    }

    /// Wakes up all registered wakers.
    pub fn wake(&self) -> usize {
        // Ordinary pipe handoff has one waiter. Keep that case allocation-free,
        // and run callbacks after releasing the registration lock.
        let inner = core::mem::take(&mut *self.0.lock());
        let count = usize::from(inner.first.is_some()) + inner.others.len();
        for w in inner.first.into_iter().chain(inner.others) {
            w.wake();
        }
        count
    }
}

impl Drop for PollSet {
    fn drop(&mut self) {
        // Ensure all entries are dropped
        self.wake();
    }
}

impl Wake for PollSet {
    fn wake(self: Arc<Self>) {
        self.as_ref().wake();
    }

    fn wake_by_ref(self: &Arc<Self>) {
        self.as_ref().wake();
    }
}
