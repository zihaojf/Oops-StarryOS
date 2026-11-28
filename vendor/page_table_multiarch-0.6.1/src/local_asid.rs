//! Single-CPU ASID cache. The architecture code performs all hardware fences.
use core::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use memory_addr::PhysAddr;

const SLOTS: usize = 128;
static ENABLED: AtomicBool = AtomicBool::new(false);
static OWNERS: [AtomicUsize; SLOTS] = [const { AtomicUsize::new(0) }; SLOTS];

fn slot(root: PhysAddr) -> usize {
    let page = root.as_usize() >> 12;
    (page ^ (page >> 7)) & (SLOTS - 1)
}

/// Whether context switches can retain tagged translations on this machine.
pub fn enabled() -> bool {
    ENABLED.load(Ordering::Acquire)
}

/// Enable after checking hardware and flushing old translations.
///
/// # Safety
/// The system must have one CPU and at least eight writable ASID bits.
pub unsafe fn enable() {
    ENABLED.store(true, Ordering::Release);
}

/// The hardware tag for this root. Zero is reserved for the boot context.
pub fn tag(root: PhysAddr) -> usize {
    if root.as_usize() == 0 {
        0
    } else {
        slot(root) + 1
    }
}

/// Claim the root's slot. A different owner requires a tag-specific fence.
///
/// # Safety
/// Called with IRQs disabled while installing the matching page-table root.
pub unsafe fn claim(root: PhysAddr) -> (usize, bool) {
    let tag = tag(root);
    if tag == 0 {
        return (tag, false);
    }
    let owner = &OWNERS[tag - 1];
    let replace = owner.load(Ordering::Relaxed) != root.as_usize();
    if replace {
        owner.store(root.as_usize(), Ordering::Relaxed);
    }
    (tag, replace)
}

/// A newly allocated root page denotes a new address space, even on frame reuse.
pub fn root_allocated(root: PhysAddr) {
    let _ = OWNERS[slot(root)].compare_exchange(
        root.as_usize(),
        0,
        Ordering::Relaxed,
        Ordering::Relaxed,
    );
}
