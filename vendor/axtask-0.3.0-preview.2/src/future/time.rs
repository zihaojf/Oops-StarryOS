use alloc::collections::BTreeMap;
use core::{
    fmt,
    pin::Pin,
    task::{Context, Poll, Waker},
    time::Duration,
};

use axerrno::AxError;
use axhal::time::{TimeValue, wall_time};
use futures_util::{FutureExt, select_biased};

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
struct TimerKey {
    deadline: TimeValue,
    key: u64,
}

struct TimerRuntime {
    key: u64,
    wheel: BTreeMap<TimerKey, Waker>,
    /// Monotonic deadline (ns) currently programmed into the hardware timer,
    /// `0` when this runtime has not armed it (e.g. the timer just fired).
    armed: u64,
}

impl TimerRuntime {
    const fn new() -> Self {
        TimerRuntime {
            key: 0,
            wheel: BTreeMap::new(),
            armed: 0,
        }
    }

    /// Programs the hardware timer at `mono_ns` if that is earlier than what is
    /// already armed.
    ///
    /// Without this, a task asking for a 1 ms sleep right after a 10 ms tick
    /// would only be woken at the next tick: sub-tick sleeps would silently
    /// degrade to the tick period.
    #[cfg(feature = "irq")]
    fn arm_earliest(&mut self, mono_ns: u64) {
        if self.armed != 0 && mono_ns >= self.armed {
            return;
        }
        self.armed = mono_ns;
        axhal::time::set_oneshot_timer(mono_ns);
    }

    fn add(&mut self, deadline: TimeValue) -> Option<TimerKey> {
        if deadline <= wall_time() {
            return None;
        }

        let key = TimerKey {
            deadline,
            key: self.key,
        };
        self.wheel.insert(key, Waker::noop().clone());
        self.key += 1;

        // If this sleep is now the earliest one, pull the hardware timer in so
        // that it fires exactly at the deadline instead of at the next tick.
        #[cfg(feature = "irq")]
        if self.wheel.keys().next().map(|k| k.deadline) == Some(deadline) {
            let mono_ns = (deadline.as_nanos() as u64)
                .saturating_sub(axhal::time::epochoffset_nanos())
                // Keep a margin of 100 us: the deadline may be in the past (then
                // the timer must fire immediately, but not at "now"), and on
                // LoongArch64 `nanos_to_ticks()` truncation can turn a 1 us
                // margin into a zero-length timer, which storms the CPU.
                .max(axhal::time::monotonic_time_nanos() + 100_000);
            self.arm_earliest(mono_ns);
        }

        Some(key)
    }

    fn poll(&mut self, key: &TimerKey, cx: &mut Context<'_>) -> Poll<()> {
        if let Some(w) = self.wheel.get_mut(key) {
            *w = cx.waker().clone();
            Poll::Pending
        } else {
            Poll::Ready(())
        }
    }

    fn cancel(&mut self, key: &TimerKey) {
        self.wheel.remove(key);
    }

    fn wake(&mut self) {
        // The hardware timer has fired, so nothing is armed any more.
        self.armed = 0;

        if self.wheel.is_empty() {
            return;
        }

        let now = wall_time();

        let pending = self.wheel.split_off(&TimerKey {
            deadline: now,
            key: u64::MAX,
        });

        let expired = core::mem::replace(&mut self.wheel, pending);
        for (_, w) in expired {
            w.wake();
        }
    }
}

percpu_static! {
    TIMER_RUNTIME: TimerRuntime = TimerRuntime::new(),
}

#[allow(dead_code)]
pub(crate) fn check_timer_events() {
    // SAFETY: only called in timer::check_events
    unsafe { TIMER_RUNTIME.current_ref_mut_raw() }.wake();
}

fn with_current<R>(f: impl FnOnce(&mut TimerRuntime) -> R) -> R {
    // FIXME: optimize `percpu` crate! should disable irq and provide more apis
    let _g = kernel_guard::NoPreemptIrqSave::new();
    f(unsafe { TIMER_RUNTIME.current_ref_mut_raw() })
}

/// Returns the earliest deadline registered on the current CPU, if any.
///
/// The value uses the same time base as [`wall_time()`], i.e. it may include the
/// epoch offset and must be rebased to monotonic time before it is handed to
/// the hardware timer.
pub fn next_deadline() -> Option<TimeValue> {
    with_current(|r| r.wheel.keys().next().map(|k| k.deadline))
}

/// Programs the hardware timer at `mono_ns`, overriding any earlier deadline.
///
/// Used by the periodic tick handler; sleep deadlines go through the
/// "earliest wins" path in [`TimerRuntime::add`] instead.
#[cfg(feature = "irq")]
pub fn set_armed_timer(mono_ns: u64) {
    with_current(|r| {
        r.armed = mono_ns;
        axhal::time::set_oneshot_timer(mono_ns);
    });
}

/// Future returned by `sleep` and `sleep_until`.
#[must_use = "futures do nothing unless you `.await` or poll them"]
pub struct TimerFuture(TimerKey);

impl Future for TimerFuture {
    type Output = ();

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        with_current(|r| r.poll(&self.0, cx))
    }
}

impl Drop for TimerFuture {
    fn drop(&mut self) {
        with_current(|r| r.cancel(&self.0));
    }
}

/// Waits until `duration` has elapsed.
pub async fn sleep(duration: Duration) {
    sleep_until(wall_time() + duration).await
}

/// Waits until `deadline` is reached.
pub async fn sleep_until(deadline: TimeValue) {
    let key = with_current(|r| r.add(deadline));
    if let Some(key) = key {
        TimerFuture(key).await;
    }
}

/// Error returned by [`timeout`] and [`timeout_at`].
#[derive(Debug, PartialEq, Eq)]
pub struct Elapsed(());

impl fmt::Display for Elapsed {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "deadline elapsed")
    }
}

impl core::error::Error for Elapsed {}

impl From<Elapsed> for AxError {
    fn from(_: Elapsed) -> Self {
        AxError::TimedOut
    }
}

/// Requires a `Future` to complete before the specified duration has elapsed.
pub async fn timeout<F: IntoFuture>(
    duration: Option<Duration>,
    f: F,
) -> Result<F::Output, Elapsed> {
    timeout_at(
        duration.and_then(|x| x.checked_add(axhal::time::wall_time())),
        f,
    )
    .await
}

/// Requires a `Future` to complete before the specified deadline.
pub async fn timeout_at<F: IntoFuture>(
    deadline: Option<TimeValue>,
    f: F,
) -> Result<F::Output, Elapsed> {
    if let Some(deadline) = deadline {
        select_biased! {
            res = f.into_future().fuse() => Ok(res),
            _ = sleep_until(deadline).fuse() => Err(Elapsed(())),
        }
    } else {
        Ok(f.await)
    }
}
