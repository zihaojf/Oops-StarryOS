use alloc::vec::Vec;
use core::{fmt, time::Duration};

use axerrno::{AxError, AxResult};
use axio::Read;
use axpoll::IoEvents;
use axtask::future::{self, block_on, poll_io};
use bitmaps::Bitmap;
use linux_raw_sys::general::*;
use starry_signal::SignalSet;
use starry_vm::{VmPtr, vm_write_slice};

use super::FdPollSet;
use crate::{
    file::current_files,
    mm::{UserConstPtr, UserPtr, VmBytes, nullable},
    syscall::signal::check_sigset_size,
    task::with_blocked_signals,
    time::TimeValueLike,
};

type Bits = Bitmap<{ __FD_SETSIZE as usize }>;

struct FdSet {
    ptr: UserPtr<__kernel_fd_set>,
    bytes: usize,
    bits: Bits,
}

impl FdSet {
    fn load(ptr: UserPtr<__kernel_fd_set>, nfds: usize) -> AxResult<Self> {
        let bytes = nfds.div_ceil(usize::BITS as usize) * size_of::<usize>();
        let mut bits = Bits::new();
        if bytes != 0 && !ptr.is_null() {
            VmBytes::new(ptr.address().as_ptr(), bytes).read(&mut bits.as_mut()[..bytes])?;
            bits &= Bits::mask(nfds);
        }
        Ok(Self { ptr, bytes, bits })
    }

    fn store(&self, bits: Bits) -> AxResult<()> {
        if self.bytes != 0 && !self.ptr.is_null() {
            vm_write_slice(
                self.ptr.address().as_mut_ptr(),
                &bits.as_bytes()[..self.bytes],
            )?;
        }
        Ok(())
    }
}

impl fmt::Debug for FdSet {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_list().entries(&self.bits).finish()
    }
}

fn fixed_ready_count(
    read: &FdSet,
    write: &FdSet,
    except: &FdSet,
    fixed: &Bits,
    nfds: u32,
) -> Option<u32> {
    let mut count = 0;
    for i in 0..nfds.div_ceil(u128::BITS) as usize {
        let r = read.bits.as_value()[i];
        let w = write.bits.as_value()[i];
        if (r | w | except.bits.as_value()[i]) & !fixed.as_value()[i] != 0 {
            return None;
        }
        count += r.count_ones() + w.count_ones();
    }
    (count > 0).then_some(count)
}

fn do_select(
    nfds: u32,
    readfds: UserPtr<__kernel_fd_set>,
    writefds: UserPtr<__kernel_fd_set>,
    exceptfds: UserPtr<__kernel_fd_set>,
    timeout: Option<Duration>,
    sigmask: UserConstPtr<SignalSetWithSize>,
) -> AxResult<isize> {
    if nfds > __FD_SETSIZE {
        return Err(AxError::InvalidInput);
    }
    let sigmask = if let Some(sigmask) = nullable!(sigmask.get_as_ref())? {
        check_sigset_size(sigmask.sigsetsize)?;
        let set = sigmask.set;
        nullable!(set.get_as_ref())?.copied()
    } else {
        None
    };

    let read = FdSet::load(readfds, nfds as usize)?;
    let write = FdSet::load(writefds, nfds as usize)?;
    let except = FdSet::load(exceptfds, nfds as usize)?;
    debug!("sys_select <= nfds: {nfds} read: {read:?} write: {write:?} except: {except:?}");

    let handle = current_files();
    let table = handle.read();
    let fixed = table.always_ready();
    // A descriptor marked here can never have exceptional events. Only inspect
    // the words covered by nfds, even when the process has many high-numbered fds.
    if let Some(count) = fixed_ready_count(&read, &write, &except, &fixed, nfds) {
        drop(table);
        drop(handle);
        with_blocked_signals(sigmask, || {
            axtask::future::blocking_io(&FdPollSet(Vec::new()), IoEvents::empty(), true, || Ok(()))
        })?;
        read.store(read.bits)?;
        write.store(write.bits)?;
        except.store(Bits::new())?;
        return Ok(count as isize);
    }
    let read_ready = read.bits & fixed;
    let write_ready = write.bits & fixed;
    let dynamic = (read.bits | write.bits | except.bits) & !fixed;
    let mut files = Vec::with_capacity(dynamic.len());
    let mut indices = Vec::with_capacity(dynamic.len());
    for index in &dynamic {
        let file = table.get(index).ok_or(AxError::BadFileDescriptor)?;
        let mut events = IoEvents::empty();
        events.set(IoEvents::IN, read.bits.get(index));
        events.set(IoEvents::OUT, write.bits.get(index));
        events.set(IoEvents::ERR, except.bits.get(index));
        files.push((file.inner.clone(), events));
        indices.push(index);
    }
    drop(table);
    drop(handle);
    let files = FdPollSet(files);

    let (r, w, e) = with_blocked_signals(sigmask, || {
        block_on(future::timeout(
            timeout,
            poll_io(&files, IoEvents::empty(), false, || {
                let (mut r, mut w, mut e) = (read_ready, write_ready, Bits::new());
                for ((file, interest), &index) in files.0.iter().zip(&indices) {
                    let raw = file.poll();
                    if interest.contains(IoEvents::IN)
                        && raw.intersects(IoEvents::IN | IoEvents::HUP | IoEvents::ERR)
                    {
                        r.set(index, true);
                    }
                    if interest.contains(IoEvents::OUT)
                        && raw.intersects(IoEvents::OUT | IoEvents::ERR)
                    {
                        w.set(index, true);
                    }
                    if interest.contains(IoEvents::ERR)
                        && raw.intersects(IoEvents::PRI | IoEvents::ERR)
                    {
                        e.set(index, true);
                    }
                }
                if r.is_empty() && w.is_empty() && e.is_empty() {
                    Err(AxError::WouldBlock)
                } else {
                    Ok((r, w, e))
                }
            }),
        ))
        .unwrap_or_else(|_| Ok((Bits::new(), Bits::new(), Bits::new())))
    })?;
    read.store(r)?;
    write.store(w)?;
    except.store(e)?;
    Ok((r.len() + w.len() + e.len()) as isize)
}

#[cfg(target_arch = "x86_64")]
pub fn sys_select(
    nfds: u32,
    readfds: UserPtr<__kernel_fd_set>,
    writefds: UserPtr<__kernel_fd_set>,
    exceptfds: UserPtr<__kernel_fd_set>,
    timeout: UserConstPtr<timeval>,
) -> AxResult<isize> {
    do_select(
        nfds,
        readfds,
        writefds,
        exceptfds,
        if timeout.is_null() {
            None
        } else {
            // The ABI structure consists solely of integer fields.
            let time = unsafe {
                timeout
                    .address()
                    .as_ptr_of::<timeval>()
                    .vm_read_uninit()?
                    .assume_init()
            };
            Some(time.try_into_time_value()?)
        },
        0.into(),
    )
}

#[repr(C)]
#[derive(Clone, Copy)]
pub struct SignalSetWithSize {
    set: UserConstPtr<SignalSet>,
    sigsetsize: usize,
}

pub fn sys_pselect6(
    nfds: u32,
    readfds: UserPtr<__kernel_fd_set>,
    writefds: UserPtr<__kernel_fd_set>,
    exceptfds: UserPtr<__kernel_fd_set>,
    timeout: UserConstPtr<timespec>,
    sigmask: UserConstPtr<SignalSetWithSize>,
) -> AxResult<isize> {
    do_select(
        nfds,
        readfds,
        writefds,
        exceptfds,
        if timeout.is_null() {
            None
        } else {
            // The ABI structure consists solely of integer fields.
            let time = unsafe {
                timeout
                    .address()
                    .as_ptr_of::<timespec>()
                    .vm_read_uninit()?
                    .assume_init()
            };
            Some(time.try_into_time_value()?)
        },
        sigmask,
    )
}
