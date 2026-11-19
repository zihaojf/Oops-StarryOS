# `kernel/src/syscall/fs/event.rs`

> 对应源码：`kernel/src/syscall/fs/event.rs`
> 所属子系统：eventfd syscall
> 依赖：`file::{FileLike, add_file_like, event::EventFd}`、`linux_raw_sys::general::{EFD_CLOEXEC, EFD_NONBLOCK, EFD_SEMAPHORE}`
> 被依赖：`syscall/fs/mod.rs`

## 1. 文件职责

实现 `eventfd2(initval, flags)`：

```rust
bitflags! { struct EventFdFlags: u32 { CLOEXEC = EFD_CLOEXEC; NONBLOCK = EFD_NONBLOCK; SEMAPHORE = EFD_SEMAPHORE } }

pub fn sys_eventfd2(initval: u32, flags: u32) -> AxResult<isize> {
    let flags = EventFdFlags::from_bits(flags).ok_or(InvalidInput)?;
    let event_fd = EventFd::new(initval as _, flags.contains(SEMAPHORE));
    event_fd.set_nonblocking(flags.contains(NONBLOCK))?;
    add_file_like(event_fd as _, flags.contains(CLOEXEC)).map(|fd| fd as _)
}
```

## 2. 语义要点

- `initval` 是计数器初值（`u32` 提升到 `u64`）。
- `EFD_SEMAPHORE` 决定 read 是"取全部"还是"取 1"。
- `EFD_NONBLOCK` 通过 `FileLike::set_nonblocking` 设置（不是文件状态 flag）。
- `EFD_CLOEXEC` 存进 fd 表的 `cloexec`，`execve` 时关闭。
- 未知 flags 位 → `InvalidInput`（`from_bits` 严格）。

## 3. 不变量与约束

- **C12/C13**：`EventFd` 的阻塞/非阻塞语义在 `file/event.rs` 实现；本文件只负责 flags 解析与登记。
- `flags` 必须严格校验：不允许多余位（`from_bits` 而非 `from_bits_truncate`）。
- 返回的 fd 必须通过 `add_file_like` 分配（受 `RLIMIT_NOFILE` 限制）。
- `eventfd2` 是 Linux 2.6.27+ 接口；没有 `eventfd`（旧）分支。

## 4. 与其他模块的关系

| 模块 | 关系 |
|---|---|
| `file/event.rs` | `EventFd` 实现 |
| `file/mod.rs` | `add_file_like` |
| `syscall/fs/fd_ops.rs` | `F_SETFL`/`fcntl` 可改非阻塞 |
| `syscall/io_mpx/*` | 通过 `Pollable` 等待 |

## 5. 已知局限 / TODO

- 无旧 `eventfd`（无 flags）syscall 号。
- `EFD_NONBLOCK` 与 fd 表 `cloexec` 分开存储（正确）。
- 没有 `eventfd` 的 `semaphore` 跨进程共享检查（Linux 允许）。

## 6. 变更影响面

改本文件 → `file/event.rs.md`、`syscall/fs/mod.rs.md`、`CONSTRAINTS.md` C12/C13。
