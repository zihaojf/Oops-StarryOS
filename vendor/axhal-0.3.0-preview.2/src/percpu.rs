//! CPU-local data structures.

pub use axplat::percpu::*;

// A uniprocessor has one current-task slot. A single atomic access remains
// valid across preemption because the interrupted task resumes on the same CPU.
#[cfg(not(feature = "smp"))]
static CURRENT_TASK_PTR: core::sync::atomic::AtomicPtr<()> =
    core::sync::atomic::AtomicPtr::new(core::ptr::null_mut());

#[cfg(feature = "smp")]
#[percpu::def_percpu]
static CURRENT_TASK_PTR: usize = 0;

/// Gets the pointer to the current task with preemption-safety.
///
/// Preemption may be enabled when calling this function. This function will
/// guarantee the correctness even the current task is preempted.
#[inline]
pub fn current_task_ptr<T>() -> *const T {
    #[cfg(not(feature = "smp"))]
    {
        CURRENT_TASK_PTR.load(core::sync::atomic::Ordering::Relaxed).cast()
    }
    #[cfg(all(feature = "smp", target_arch = "x86_64"))]
    unsafe {
        // on x86, only one instruction is needed to read the per-CPU task pointer from `gs:[off]`.
        CURRENT_TASK_PTR.read_current_raw() as _
    }
    #[cfg(all(feature = "smp", any(
        target_arch = "aarch64",
        target_arch = "riscv32",
        target_arch = "riscv64",
        target_arch = "loongarch64"
    )))]
    unsafe {
        // on RISC-V and LA64, reading `CURRENT_TASK_PTR` requires multiple instruction, so we disable local IRQs.
        let _guard = kernel_guard::IrqSave::new();
        CURRENT_TASK_PTR.read_current_raw() as _
    }
}

/// Sets the pointer to the current task with preemption-safety.
///
/// Preemption may be enabled when calling this function. This function will
/// guarantee the correctness even the current task is preempted.
///
/// # Safety
///
/// The given `ptr` must be pointed to a valid task structure.
#[inline]
pub unsafe fn set_current_task_ptr<T>(ptr: *const T) {
    #[cfg(not(feature = "smp"))]
    CURRENT_TASK_PTR.store(ptr.cast_mut().cast(), core::sync::atomic::Ordering::Relaxed);
    #[cfg(all(feature = "smp", target_arch = "x86_64"))]
    {
        unsafe { CURRENT_TASK_PTR.write_current_raw(ptr as usize) }
    }
    #[cfg(all(feature = "smp", any(
        target_arch = "aarch64",
        target_arch = "riscv32",
        target_arch = "riscv64",
        target_arch = "loongarch64"
    )))]
    {
        let _guard = kernel_guard::IrqSave::new();
        unsafe { CURRENT_TASK_PTR.write_current_raw(ptr as usize) }
    }
}
