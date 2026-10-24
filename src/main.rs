#![no_std]
#![no_main]
#![doc = include_str!("../README.md")]

extern crate alloc;

use alloc::{borrow::ToOwned, vec};

/// The command line used to start the init process.
pub const CMDLINE: &[&str] = &["/bin/sh", "-c", include_str!("init.sh")];

/// Candidate shells on the root filesystem, in priority order.
///
/// Bootable disk images from the OS competition only contain busybox binaries
/// (e.g. `/musl/busybox`) instead of a regular `/bin/sh`, so fall back to them.
const SHELL_CANDIDATES: &[&str] = &["/bin/sh", "/musl/busybox", "/glibc/busybox", "/busybox"];

fn rootfs_contains(path: &str) -> bool {
    axfs::FS_CONTEXT.lock().resolve(path).is_ok()
}

#[unsafe(no_mangle)]
fn main() {
    let shell = SHELL_CANDIDATES
        .iter()
        .copied()
        .find(|path| rootfs_contains(path))
        .unwrap_or(CMDLINE[0]);

    let mut args = vec![shell.to_owned()];
    // Running a busybox binary requires an applet name unless it is invoked as
    // one of its applet names (e.g. `/bin/sh`).
    if shell != "/bin/sh" {
        args.push("sh".to_owned());
    }
    args.extend(CMDLINE[1..].iter().copied().map(str::to_owned));

    let envs = [];

    starry_kernel::entry::init(&args, &envs);
}

#[cfg(feature = "vf2")]
extern crate axplat_riscv64_visionfive2;
