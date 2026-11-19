# `kernel/src/syscall/fs/pidfd.rs`

> 对应源码：`kernel/src/syscall/fs/pidfd.rs`
> 所属子系统：pidfd syscall
> 依赖：`file::{PidFd, add_file_like, FD_TABLE, FileLike}`、`syscall/signal::make_queue_signal_info`、`task::{get_process_data, get_task, send_signal_to_process}`
> 被依赖：`syscall/fs/mod.rs`

## 1. 文件职责

实现 `pidfd_open`、`pidfd_getfd`、`pidfd_send_signal`。

```rust
bitflags! { struct PidFdFlags: u32 { NONBLOCK = 2048 /*PIDFD_NONBLOCK*/, THREAD = 128 /*PIDFD_THREAD*/ } }
```

| syscall | 行为 |
|---|---|
| `pidfd_open(pid, flags)` | `THREAD` → `PidFd::new_thread(get_task(pid)?.as_thread())`；否则 `PidFd::new_process(&get_process_data(pid)?)`；`NONBLOCK` → `set_nonblocking`；`add_to_fd_table(cloexec=true)` |
| `pidfd_getfd(pidfd, target_fd, flags)` | 取 pidfd 的 `process_data`，从目标进程的 `FD_TABLE.scope(...)` 取 `inner.clone()`，`add_file_like(clone, true)` |
| `pidfd_send_signal(pidfd, signo, sig, flags)` | `flags != 0` → `InvalidInput`；取 pid；`make_queue_signal_info` + `send_signal_to_process` |

## 2. 不变量与约束

- **`PidFd` 是弱引用**：进程/线程退出后 `process_data()` 返回 `NoSuchProcess`（见 `file/pidfd.rs`）。
- **C12**：`PidFd` 必须实现 `FileLike + Pollable`（已实现）。
- `pidfd_getfd` 未做权限检查（Linux 要求 ptrace 权限）；当前全 root 下可接受，但是安全缺口。
- `pidfd_send_signal` 的 `sig` 指针经 `make_queue_signal_info` 读取并校验来源（非本进程时 code 受限）。
- 所有 pidfd 都 `cloexec = true`（Linux 语义）。
- `PidFdFlags` 的数值来自 Linux 头文件（`PIDFD_NONBLOCK = O_NONBLOCK = 2048`，`PIDFD_THREAD = O_EXCL = 128`）；不要改成 1/2。

## 3. 与其他模块的关系

| 模块 | 关系 |
|---|---|
| `file/pidfd.rs` | `PidFd` 实现 |
| `file/mod.rs` | `FD_TABLE`/`add_file_like` |
| `task/ops.rs` | `get_process_data`/`get_task`/`exit_event` |
| `task/signal.rs` | `send_signal_to_process` |
| `syscall/signal.rs` | `make_queue_signal_info` |
| `syscall/task/clone.rs` | `CLONE_PIDFD` 也创建 pidfd |

## 4. 已知局限 / TODO

- `pidfd_getfd` 无权限检查。
- 无 `waitid(P_PIDFD)`。
- `pidfd` 的 poll 语义（存活 vs 已退出）与 Linux 相反（见 `file/pidfd.rs.md`）。
- `PIDFD_THREAD` 的 `pidfd_open` 依赖 `get_task`（tid 查找），与 Linux 一致。

## 5. 变更影响面

改本文件 → `file/pidfd.rs.md`、`syscall/signal.rs.md`、`syscall/task/clone.rs.md`、`syscall/fs/mod.rs.md`。
