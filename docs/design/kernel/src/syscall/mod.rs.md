# `kernel/src/syscall/mod.rs`

> 对应源码：`kernel/src/syscall/mod.rs`
> 所属子系统：系统调用总入口
> 依赖：`syscalls::Sysno`、`axhal::uspace::UserContext`、各子模块
> 被依赖：`task/user.rs`（`handle_syscall`）、一切用户态程序

## 1. 文件职责

1. 声明并重导出子模块（`fs/io_mpx/ipc/mm/net/resources/signal/sync/sys/task/time`）；
2. `handle_syscall(uctx)`：解析 syscall 号、从 `UserContext` 取参数、单一大 `match` 分派、把 `AxResult<isize>` 转成返回值或 `-errno`。

## 2. 关键流程

```rust
pub fn handle_syscall(uctx: &mut UserContext) {
    let Some(sysno) = Sysno::new(uctx.sysno()) else { warn!; retval = -ENOSYS; return };
    trace!("Syscall {sysno:?}");
    let result = match sysno {
        Sysno::ioctl => sys_ioctl(uctx.arg0() as _, uctx.arg1() as _, uctx.arg2() as _),
        ...
        Sysno::timerfd_create | ... => sys_dummy_fd(sysno),
        Sysno::timer_create | Sysno::timer_gettime | Sysno::timer_settime => Ok(0),
        _ => { warn!("Unimplemented syscall: {sysno}"); Err(AxError::Unsupported) }
    };
    debug!("Syscall {sysno} return {result:?}");
    uctx.set_retval(result.unwrap_or_else(|err| -LinuxError::from(err).code() as _) as _);
}
```

## 3. 分派表逻辑分组

| 分组 | 代表 syscall |
|---|---|
| fs ctl | `ioctl/chdir/fchdir/chroot/mkdir(at)/getdents64/link(at)/unlink(at)/rmdir/getcwd/symlink(at)/readlink(at)/chown/fchmod/utime*/rename(at2)/sync/syncfs` |
| file ops | `open(at)/close/close_range/dup(2/3)/fcntl/flock` |
| io | `read(v)/write(v)/lseek/truncate/ftruncate/fallocate/fsync/fdatasync/fadvise64/pread*64/pwrite*64/sendfile/copy_file_range/splice` |
| io mpx | `poll/ppoll/select/pselect6/epoll_*` |
| fs mount | `mount/umount2` |
| pipe/event/pidfd/memfd | `pipe2/eventfd2/pidfd_*/memfd_create` |
| fs stat | `stat/fstat/lstat/fstatat(2)/statx/access/faccessat(2)/statfs/fstatfs` |
| mm | `brk/mmap/munmap/mprotect/mincore/mremap/madvise/msync/mlock/mlock2` |
| task info/sched | `getpid/getppid/gettid/getrusage/sched_yield/nanosleep/clock_nanosleep/sched_*affinity/sched_get*scheduler/getpriority` |
| task ops | `execve/set_tid_address/arch_prctl/prctl/prlimit64/capget/capset/umask/set*uid/setres*uid/get_mempolicy` |
| task mgmt | `clone/clone3/fork/exit/exit_group/wait4/getsid/setsid/getpgid/setpgid` |
| signal | `rt_sig* /kill/tkill/tgkill/rt_sigqueueinfo/sigaltstack/futex/get/set_robust_list` |
| sys | `getuid/geteuid/getgid/getegid/setuid/setgid/getgroups/setgroups/uname/sysinfo/syslog/getrandom/seccomp/reboot/riscv_flush_icache` |
| sync | `membarrier` |
| time | `gettimeofday/times/clock_gettime/clock_getres/getitimer/setitimer` |
| ipc | `msgget/msgsnd/msgrcv/msgctl/shmget/shmat/shmctl/shmdt` |
| net | `socket/socketpair/bind/connect/getsockname/getpeername/listen/accept(4)/shutdown/sendto/recvfrom/sendmsg/recvmsg/getsockopt/setsockopt` |
| signalfd | `signalfd4` |
| dummy | `timerfd_create/fanotify_init/inotify_init1/userfaultfd/perf_event_open/io_uring_setup/bpf/fsopen/fspick/open_tree/memfd_secret` |

## 4. 不变量与约束

- **C21**：返回值统一 `AxResult<isize>`，错误用 `-LinuxError::from(err).code()`；不要返回 `-1`。
- **C22**：未实现落到 `_` 分支（`warn!` + `Unsupported`）；不要 panic、不要静默成功。
- **C23**：`as _` 转换必须与目标类型宽度匹配；`arg` 是 `usize`，`-1` 类参数（`dirfd`/`pid`/`timeout`）要转成有符号类型（如 `i32`）。
- **C24**：`sigsetsize` 等长度参数由各实现校验（`check_sigset_size`）。
- **架构条件**：`x86_64` 才有 `open/stat/fork/dup2/...`；`riscv64` 没有 `renameat`；`fstatat` 在 riscv64 叫 `newfstatat`；`riscv_flush_icache` 仅 riscv64。新增 syscall 必须考虑 ABI 号差异。
- `sys_dummy_fd` 对 `qemu-*` 任务返回 `Unsupported`（诚实性要求）。
- `timer_create/gettime/settime` 返回 `Ok(0)` 但**不真正创建 timer**（POSIX timer 未实现），调用方可能误判成功——这是有意的宽松（见 P8）。

## 5. 与其他模块的关系

- `task/user.rs`：唯一调用者。
- 各子模块：`sys/stats/...` 的实现。
- `mm/access.rs`：参数读取（`UserPtr`/`vm_load_string`）。
- `file/mod.rs`：fd 表。
- 本文件是"支持列表"的权威：新增 syscall 要同时更新本文件与对应子系统文档。

## 6. 已知局限 / TODO

- 大量 syscall 未实现（`Unsupported`）或 stub（返回 0）。
- `timer_*`（POSIX timer）是 stub。
- 无 `seccomp` 过滤（`sys_seccomp` 直接返回 0）。
- 无 `io_uring`/`bpf`/`fanotify`/`inotify`（dummy fd）。
- `arch_prctl` 仅 x86_64。

## 7. 变更影响面

改本文件 → 所有 `syscall/**` 文档、`task/user.rs.md`、`ARCHITECTURE.md#6`、`CONSTRAINTS.md` C21/C22/C23/C25。
