# `kernel/src/pseudofs/dev/tty/terminal/job.rs`

> 对应源码：`kernel/src/pseudofs/dev/tty/terminal/job.rs`
> 所属子系统：终端作业控制（会话 / 前台进程组）
> 依赖：`kspin::SpinNoIrq`、`axpoll::{PollSet, Pollable}`、`starry_process::{ProcessGroup, Session}`
> 被依赖：`terminal/mod.rs`、`pseudofs/dev/tty/mod.rs`（`bind_to`/ioctl）、`terminal/ldisc.rs`（发信号给前台组）

## 1. 文件职责

维护一个 tty 的会话与前台进程组，并提供可 poll 的"前台"变化事件。

```rust
pub struct JobControl {
    foreground: SpinNoIrq<Weak<ProcessGroup>>,   // 弱引用：不延长进程组生命周期
    session: SpinNoIrq<Weak<Session>>,
    poll_fg: PollSet,
}
```

## 2. 关键流程

| 方法 | 行为 |
|---|---|
| `current_in_foreground()` | 前台为空（无 tty 会话）→ true；否则比较 `current().group()` 与前台组 `Arc::ptr_eq` |
| `foreground()` | 升级弱引用 |
| `set_foreground(pg)` | 若已是同一组 → `Ok`；要求已绑定 session 且 `pg.session() == session`；否则 `OperationNotPermitted`；更新后 `poll_fg.wake()` |
| `set_session(&Arc<Session>)` | 断言当前无 session；存弱引用 |
| `Pollable::poll` | `IN = current_in_foreground()` |
| `Pollable::register` | 注册 `poll_fg` |

## 3. 不变量与约束

- **`Weak` 而非 `Arc`**：tty 不应阻止进程组/会话被回收；`foreground()` 升级失败返回 `None`，调用方要处理。
- **同一会话约束**：`set_foreground` 要求目标组与已绑定 session 相同；`bind_to` 先 `set_session` 再 `set_foreground`。
- **`set_session` 只能一次**：断言 `guard.upgrade().is_none()`；`TIOCSCTTY` 重复设置会 panic——`Tty::bind_to` 通过 `session.set_terminal_with` 保证只成功一次。
- **`current_in_foreground` 依赖 `current()`**：在中断上下文调用会得到错误任务；只在 syscall/读路径使用。
- `poll_fg` 只在 `set_foreground` 时唤醒；会话无前台时读端直接判定前台（允许读）。
- **前台切换不发 `SIGTTOU/SIGTTIN`**（未实现作业控制信号）。

## 4. 与其他模块的关系

| 模块 | 关系 |
|---|---|
| `terminal/mod.rs` | `Terminal.job_control` |
| `pseudofs/dev/tty/mod.rs` | `bind_to`（set_session + set_foreground）、`TIOCGPGRP/TIOCSPGRP/TIOCSCTTY/TIOCNOTTY`、`read_at` 前台检查、`Pollable` |
| `terminal/ldisc.rs` | `check_send_signal` 用 `foreground()` 得到前台组，发 `SIGINT/SIGQUIT` |
| `starry_process` | `ProcessGroup/Session` |
| `task/mod.rs` | `current().as_thread().proc_data.proc.group()` |

## 5. 已知局限 / TODO

- 无 `SIGTTIN`/`SIGTTOU`/`SIGHUP` 发送（后台读写/挂断语义缺失）。
- `set_session` 用 `assert!`（重复设置 panic），没有错误返回。
- 无 `TIOCSPGRP` 的"组长检查"（Linux 要求进程组属于会话，已做；还要求 pgid 存在，已由 `get_process_group` 保证）。
- `current_in_foreground` 在 `foreground` 为 `None` 时返回 true（宽松，便于无会话终端可读）。

## 6. 变更影响面

改本文件 → `terminal/{mod,ldisc}.rs.md`、`pseudofs/dev/tty/mod.rs.md`、`syscall/task/job.rs.md`（会话/进程组）。
