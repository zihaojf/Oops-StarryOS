use axerrno::{AxError, AxResult};
use axhal::time::TimeValue;
use axtask::{
    AxCpuMask, AxTaskRef, current,
    future::{block_on, interruptible, sleep},
};
use linux_raw_sys::general::{
    __kernel_clockid_t, CLOCK_MONOTONIC, CLOCK_REALTIME, PRIO_PGRP, PRIO_PROCESS, PRIO_USER,
    SCHED_FIFO, SCHED_RR, TIMER_ABSTIME, timespec,
};
use starry_vm::{VmMutPtr, VmPtr, vm_load, vm_write_slice};

use crate::{
    task::{get_process_data, get_process_group, get_task},
    time::TimeValueLike,
};

pub fn sys_sched_yield() -> AxResult<isize> {
    axtask::yield_now();
    Ok(0)
}

fn sleep_impl(clock: impl Fn() -> TimeValue, dur: TimeValue) -> TimeValue {
    debug!("sleep_impl <= {dur:?}");

    let start = clock();

    // TODO: currently ignoring concrete clock type
    // We detect EINTR manually if the slept time is not enough.
    let _ = block_on(interruptible(sleep(dur)));

    clock() - start
}

/// Sleep some nanoseconds
pub fn sys_nanosleep(req: *const timespec, rem: *mut timespec) -> AxResult<isize> {
    // FIXME: AnyBitPattern
    let req = unsafe { req.vm_read_uninit()?.assume_init() }.try_into_time_value()?;
    debug!("sys_nanosleep <= req: {req:?}");

    let actual = sleep_impl(axhal::time::monotonic_time, req);

    if let Some(diff) = req.checked_sub(actual) {
        debug!("sys_nanosleep => rem: {diff:?}");
        if let Some(rem) = rem.nullable() {
            rem.vm_write(timespec::from_time_value(diff))?;
        }
        Err(AxError::Interrupted)
    } else {
        Ok(0)
    }
}

pub fn sys_clock_nanosleep(
    clock_id: __kernel_clockid_t,
    flags: u32,
    req: *const timespec,
    rem: *mut timespec,
) -> AxResult<isize> {
    let clock = match clock_id as u32 {
        CLOCK_REALTIME => axhal::time::wall_time,
        CLOCK_MONOTONIC => axhal::time::monotonic_time,
        _ => {
            warn!("Unsupported clock_id: {clock_id}");
            return Err(AxError::InvalidInput);
        }
    };

    let req = unsafe { req.vm_read_uninit()?.assume_init() }.try_into_time_value()?;
    debug!("sys_clock_nanosleep <= clock_id: {clock_id}, flags: {flags}, req: {req:?}");

    let dur = if flags & TIMER_ABSTIME != 0 {
        req.saturating_sub(clock())
    } else {
        req
    };

    let actual = sleep_impl(clock, dur);

    if let Some(diff) = dur.checked_sub(actual) {
        debug!("sys_clock_nanosleep => rem: {diff:?}");
        if let Some(rem) = rem.nullable() {
            rem.vm_write(timespec::from_time_value(diff))?;
        }
        Err(AxError::Interrupted)
    } else {
        Ok(0)
    }
}

pub fn sys_sched_getaffinity(pid: i32, cpusetsize: usize, user_mask: *mut u8) -> AxResult<isize> {
    if cpusetsize * 8 < axhal::cpu_num() {
        return Err(AxError::InvalidInput);
    }

    // TODO: support other threads
    if pid != 0 {
        return Err(AxError::OperationNotPermitted);
    }

    let mask = current().cpumask();
    let mask_bytes = mask.as_bytes();

    vm_write_slice(user_mask, mask_bytes)?;

    Ok(mask_bytes.len() as _)
}

pub fn sys_sched_setaffinity(
    _pid: i32,
    cpusetsize: usize,
    user_mask: *const u8,
) -> AxResult<isize> {
    let size = cpusetsize.min(axhal::cpu_num().div_ceil(8));
    let user_mask = vm_load(user_mask, size)?;
    let mut cpu_mask = AxCpuMask::new();

    for i in 0..(size * 8).min(axhal::cpu_num()) {
        if user_mask[i / 8] & (1 << (i % 8)) != 0 {
            cpu_mask.set(i, true);
        }
    }

    // TODO: support other threads
    axtask::set_current_affinity(cpu_mask);

    Ok(0)
}

/// `struct sched_param` carries a single `int` on Linux, so the pointer is
/// read and written as `i32`.
const SCHED_RESET_ON_FORK: i32 = 0x4000_0000;

/// Normal time-sharing policy. It is 0 on Linux, but `linux_raw_sys` does
/// not export it, so it is defined here.
const SCHED_OTHER: u32 = 0;

fn task_for_sched_call(pid: i32) -> AxResult<AxTaskRef> {
    get_task(pid as _)
}

pub fn sys_sched_getscheduler(pid: i32) -> AxResult<isize> {
    let task = task_for_sched_call(pid)?;
    let policy = match task.sched_policy() {
        axtask::SCHED_FIFO => SCHED_FIFO,
        axtask::SCHED_RR => SCHED_RR,
        _ => SCHED_OTHER,
    };
    Ok(policy as isize)
}

pub fn sys_sched_setscheduler(pid: i32, policy: i32, param: *const i32) -> AxResult<isize> {
    let policy = policy & !SCHED_RESET_ON_FORK;
    let prio = match param.nullable() {
        Some(p) => p.vm_read()?,
        None => 0,
    };

    let (kernel_policy, kernel_prio) = match policy as u32 {
        SCHED_FIFO => {
            if !(1..=99).contains(&prio) {
                return Err(AxError::InvalidInput);
            }
            (axtask::SCHED_FIFO, prio as u8)
        }
        SCHED_RR => {
            if !(1..=99).contains(&prio) {
                return Err(AxError::InvalidInput);
            }
            (axtask::SCHED_RR, prio as u8)
        }
        SCHED_OTHER => {
            if prio != 0 {
                return Err(AxError::InvalidInput);
            }
            (axtask::SCHED_OTHER, 0)
        }
        _ => return Err(AxError::InvalidInput),
    };

    task_for_sched_call(pid)?.set_sched_param(kernel_policy, kernel_prio);
    Ok(0)
}

pub fn sys_sched_getparam(pid: i32, param: *mut i32) -> AxResult<isize> {
    let task = task_for_sched_call(pid)?;
    if let Some(p) = param.nullable() {
        p.vm_write(task.rt_priority() as i32)?;
    }
    Ok(0)
}

pub fn sys_getpriority(which: u32, who: u32) -> AxResult<isize> {
    debug!("sys_getpriority <= which: {which}, who: {who}");

    match which {
        PRIO_PROCESS => {
            if who != 0 {
                let _proc = get_process_data(who)?;
            }
            Ok(20)
        }
        PRIO_PGRP => {
            if who != 0 {
                let _pg = get_process_group(who)?;
            }
            Ok(20)
        }
        PRIO_USER => {
            if who == 0 {
                Ok(20)
            } else {
                Err(AxError::NoSuchProcess)
            }
        }
        _ => Err(AxError::InvalidInput),
    }
}
