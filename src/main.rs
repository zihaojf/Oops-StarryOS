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

/// Install native hello helpers for test images that only provide shell wrappers.
fn install_lmbench_helper(libc: &str, bytes: &[u8]) {
    if bytes.is_empty() || !rootfs_contains(&alloc::format!("/{libc}/lmbench_testcode.sh")) {
        return;
    }
    let path = alloc::format!("/starry-lmbench-hello-{libc}");
    let Ok(opened) = axfs::OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(true)
        .mode(0o755)
        .open(&axfs::FS_CONTEXT.lock(), path.as_str())
    else {
        return;
    };
    let Ok(file) = opened.into_file() else {
        return;
    };
    let mut written = 0;
    while written < bytes.len() {
        match file.write(&bytes[written..]) {
            Ok(0) | Err(_) => break,
            Ok(n) => written += n,
        }
    }
}

#[unsafe(no_mangle)]
fn main() {
    install_lmbench_helper(
        "glibc",
        include_bytes!(concat!(env!("OUT_DIR"), "/lmbench-hello-glibc")),
    );
    install_lmbench_helper(
        "musl",
        include_bytes!(concat!(env!("OUT_DIR"), "/lmbench-hello-musl")),
    );
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
