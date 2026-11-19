# `kernel/src/syscall/task/job.rs`

> 对应源码：`kernel/src/syscall/task/job.rs`
> 所属子系统：作业控制（会话/进程组）
> 依赖：`starry_process::{Process, ProcessGroup, Session, Pid}`、`task::{get_process_data, get_process_group, AsThread}`
> 被依赖：`syscall/task/mod.rs`、`pseudofs/dev/tty/*`（会话/前台组）

## 1. 文件职责

实现 `getsid/setsid/getpgid/setpgid`。

## 2. 各函数行为

| syscall | 行为 |
|---|---|
| `sys_getsid(pid)` | `get_process_data(pid)?.proc.group().session().sid()` |
| `sys_setsid()` | 若 `get_process_group(proc.pid()).is_ok()`（自己已是组长）→ `OperationNotPermitted`；否则 `proc.create_session()` 返回新 sid（失败返回 pid） |
| `sys_getpgid(pid)` | `get_process_data(pid)?.proc.group().pgid()` |
| `sys_setpgid(pid, pgid)` | `pgid == 0` → `proc.create_group()`；否则 `proc.move_to_group(&get_process_group(pgid)?)`，失败 `OperationNotPermitted` |

## 3. 语义细节

- `setsid` 要求调用者不是进程组组长（Linux 语义）；`create_session` 失败时返回 pid（宽松）。
- `setpgid(pid, 0)` 创建新组并把 `pid` 移入（`create_group`）。
- `setpgid` 不校验 pid 是否为当前进程的子进程/自身（Linux 有限制，这里靠 `get_process_data` 存在性）。
- `getsid(0)` 的 pid=0 语义由 `get_process_data(0)` → 当前进程提供。

## 4. 不变量与约束

- **会话/组关系由 `starry_process` 维护**：不要把 `pid/pgid/sid` 存在 `ProcessData` 里。
- `setsid` 后进程成为新会话首进程，且没有控制终端（`JobControl.set_session` 由 `TIOCSCTTY` 后续绑定）。
- `setpgid` 的目标组必须存在（`get_process_group`）。
- `sys_setsid` 的返回值：新 sid（成功）或 pid（`create_session` 返回 None 时）。
- 无 `setpgrp`（等价 `setpgid(0,0)`）。
- 无 `getsid` 的权限检查（Linux 允许同会话查询）。

## 5. 与其他模块的关系

| 模块 | 关系 |
|---|---|
| `starry_process` | `Process/ProcessGroup/Session` |
| `task/ops.rs` | `get_process_data/get_process_group` |
| `pseudofs/dev/tty/terminal/job.rs` | `set_foreground`/`set_session`（终端侧） |
| `pseudofs/dev/tty/mod.rs` | `bind_to`（要求 sid == pid） |
| `syscall/task/mod.rs` | 门面 |

## 6. 已知局限 / TODO

- 无作业控制信号（`SIGTSTP/SIGCONT`，见 `task/signal.rs`）。
- `setpgid` 未校验"子进程尚未 exec"等 Linux 限制。
- `setsid` 的失败回退（返回 pid）语义模糊。
- 无 `tcsetpgrp` 的完整前台组切换（只有 TIOCSPGRP）。

## 7. 变更影响面

改本文件 → `pseudofs/dev/tty/terminal/job.rs.md`、`pseudofs/dev/tty/mod.rs.md`、`syscall/task/mod.rs.md`、`ARCHITECTURE.md#4`。
