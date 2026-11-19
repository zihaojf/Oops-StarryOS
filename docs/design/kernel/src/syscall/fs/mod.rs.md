# `kernel/src/syscall/fs/mod.rs`

> 对应源码：`kernel/src/syscall/fs/mod.rs`
> 所属子系统：文件系统 syscall（模块门面）
> 依赖：子模块 `ctl/event/fd_ops/io/memfd/mount/pidfd/pipe/signalfd/stat`

## 1. 文件职责

纯门面：声明并 `pub use` 全部子模块，使 `syscall/mod.rs` 可直接调用 `sys_ioctl`、`sys_openat` 等。

```rust
mod ctl; mod event; mod fd_ops; mod io; mod memfd; mod mount; mod pidfd; mod pipe; mod signalfd; mod stat;
pub use self::{ctl::*, event::*, fd_ops::*, io::*, memfd::*, mount::*, pidfd::*, pipe::*, signalfd::*, stat::*};
```

## 2. 子系统划分

| 文件 | 覆盖 syscall |
|---|---|
| `ctl.rs` | `ioctl/chdir/fchdir/chroot/mkdir(at)/getdents64/link(at)/unlink(at)/rmdir/getcwd/symlink(at)/readlink(at)/chown*/chmod*/utime*/rename*/sync/syncfs` |
| `fd_ops.rs` | `open(at)/close/close_range/dup/dup2/dup3/fcntl/flock` |
| `io.rs` | `read/readv/write/writev/lseek/truncate/ftruncate/fallocate/fsync/fdatasync/fadvise64/pread*64/pwrite*64/sendfile/copy_file_range/splice` |
| `stat.rs` | `stat/fstat/lstat/fstatat/statx/access/faccessat2/statfs/fstatfs` |
| `mount.rs` | `mount/umount2` |
| `pipe.rs` | `pipe2` |
| `event.rs` | `eventfd2` |
| `pidfd.rs` | `pidfd_open/pidfd_getfd/pidfd_send_signal` |
| `memfd.rs` | `memfd_create` |
| `signalfd.rs` | `signalfd4` |

## 3. 不变量与约束

- 新增 fs 类 syscall：在此声明子模块（若新建文件）+ 实现 + `syscall/mod.rs` 分派 + 两处文档。
- 所有实现必须复用 `file::{FileLike, resolve_at, with_fs, get_file_like}` 与 `mm::{UserPtr, vm_load_string}`（C6/C11）。
- 返回 `AxResult<isize>`，错误经 `-errno`（C21）。

## 4. 变更影响面

改本文件 → 所有 `syscall/fs/*.md`、`syscall/mod.rs.md`、`TRACEABILITY.md`。
