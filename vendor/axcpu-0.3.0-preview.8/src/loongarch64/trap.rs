use loongArch64::register::{
    badv,
    estat::{self, Exception, Trap},
    ticlr,
};

use super::context::TrapFrame;
use crate::trap::PageFaultFlags;

core::arch::global_asm!(
    include_asm_macros!(),
    include_str!("trap.S"),
    trapframe_size = const (core::mem::size_of::<TrapFrame>()),
);

fn handle_breakpoint(era: &mut usize) {
    debug!("Exception(Breakpoint) @ {era:#x} ");
    *era += 4;
}

fn handle_page_fault(tf: &mut TrapFrame, access_flags: PageFaultFlags) {
    let vaddr = va!(badv::read().vaddr());
    if handle_trap!(PAGE_FAULT, vaddr, access_flags) {
        return;
    }
    #[cfg(feature = "uspace")]
    if tf.fixup_exception() {
        return;
    }
    panic!(
        "Unhandled PLV0 Page Fault @ {:#x}, fault_vaddr={:#x} ({:?}):\n{:#x?}\n{}",
        tf.era,
        vaddr,
        access_flags,
        tf,
        tf.backtrace()
    );
}

/// Bit of the timer interrupt line in `ESTAT.IS` (`IS.TI`, LoongArch Vol.1).
pub(crate) const TIMER_IS_BIT: usize = 11;

/// Dispatches **every** interrupt line pending in `ESTAT.IS`.
///
/// `ESTAT.IS` reports all pending lines at once, and the timer line is
/// level-triggered: it stays pending until `TICLR` is written. Dispatching
/// only `estat.is().trailing_zeros()` therefore handles the lowest pending
/// line; when a timer interrupt arrives together with any external interrupt
/// (HWI occupies the lower bits) the timer line never reaches the timer
/// handler, is never acknowledged, and the CPU re-enters this trap forever.
/// No task runs again, which reads as a silently hung machine rather than as a
/// failing syscall. So the timer line is dispatched and acknowledged first, and
/// the remaining lines are drained afterwards.
pub(crate) fn dispatch_pending_irqs() {
    let pending = estat::read().is();

    if pending & (1 << TIMER_IS_BIT) != 0 {
        if !handle_trap!(IRQ, TIMER_IS_BIT) {
            // No handler installed (or it reported failure): acknowledge the
            // line here so the level-triggered interrupt cannot storm.
            ticlr::clear_timer_interrupt();
        }
    }

    let mut rest = pending & !(1 << TIMER_IS_BIT);
    while rest != 0 {
        let irq = rest.trailing_zeros() as usize;
        rest &= rest - 1;
        handle_trap!(IRQ, irq);
    }
}

#[unsafe(no_mangle)]
fn loongarch64_trap_handler(tf: &mut TrapFrame) {
    let estat = estat::read();

    match estat.cause() {
        Trap::Exception(Exception::LoadPageFault)
        | Trap::Exception(Exception::PageNonReadableFault) => {
            handle_page_fault(tf, PageFaultFlags::READ)
        }
        Trap::Exception(Exception::StorePageFault)
        | Trap::Exception(Exception::PageModifyFault) => {
            handle_page_fault(tf, PageFaultFlags::WRITE)
        }
        Trap::Exception(Exception::FetchPageFault)
        | Trap::Exception(Exception::PageNonExecutableFault) => {
            handle_page_fault(tf, PageFaultFlags::EXECUTE);
        }
        Trap::Exception(Exception::Breakpoint) => handle_breakpoint(&mut tf.era),
        Trap::Exception(Exception::AddressNotAligned) => unsafe {
            tf.emulate_unaligned().unwrap();
        },
        Trap::Interrupt(_) => dispatch_pending_irqs(),
        trap => {
            panic!(
                "Unhandled trap {:?} @ {:#x}:\n{:#x?}\n{}",
                trap,
                tf.era,
                tf,
                tf.backtrace()
            );
        }
    }
}
