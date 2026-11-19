# `kernel/src/file/pidfd.rs`

> 对应源码：`kernel/src/file/pidfd.rs`
> 所属子系统：pidfd
> 依赖：`axpoll::{PollSet, Pollable}`、`task::{ProcessData, Thread}`
> 被依赖：`file/mod.rs`（重导出 `PidFd`）、`syscall/fs/pidfd.rs`、`syscall/task/clone.rs`（CLONE_PIDFD）

## 1. 文件职责

实现 `pidfd_open` / `CLONE_PIDFD` 得到的 pidfd：一个可 poll 的 fd，表示"进程/线程已退出"。

```rust
pub struct PidFd {
    proc_data: Weak<ProcessData>,
    exit_event: Arc<PollSet>,      // 进程：proc_data.exit_event；线程：thread.exit_event
    thread_exit: Option<Arc<AtomicBool>>,  // 线程模式才有
    non_blocking: AtomicBool,
}
```

## 2. 语义

| 方法 | 行为 |
|---|---|
| `new_process(&Arc<ProcessData>)` | `proc_data` 弱引用 + `proc_data.exit_event` |
| `new_thread(&Thread)` | 弱引用进程 + `thread.exit_event` + `thread.exit` |
| `process_data()` | 线程模式下若 `thread_exit` 已置位 → `NoSuchProcess`；否则 `proc_data.upgrade()` |
| `poll` | `IN` = 进程仍存活（`weak.strong_count() > 0`）且（线程未退出） |
| `register` | 注册 `exit_event` 的 waker |

注意：`poll` 的 `IN` 语义是**"仍然存活"**，与 Linux pidfd 一致（可读表示已退出？——Linux 中 pidfd 在进程退出时变为可读）。这里的实现把 `IN` 设为"存活"，因为 `exit_event` 在退出时 wake，读者被唤醒后再 poll 会看到 `IN=false`。使用方（`syscall/fs/pidfd.rs`）不直接依赖这个语义，只是把它当 `FileLike`。

## 3. 关键流程

```text
pidfd_open(pid)            → get_process_data(pid) → PidFd::new_process → add_to_fd_table(cloexec=true)
pidfd_open(pid, PIDFD_THREAD) → get_task(pid).as_thread() → PidFd::new_thread
clone(CLONE_PIDFD)         → PidFd::new_process/new_thread → add_file_like(true) → 写回用户 pidfd 指针
进程/线程退出              → do_exit 中 exit_event.wake()     → 唤醒 poll 者
```

## 4. 不变量与约束

- **C14**：退出路径必须 `exit_event.wake()`（`task/ops.rs::do_exit` 已做：`thr.exit_event.wake()` 与 `thr.proc_data.exit_event.wake()`）；否则 pidfd 永远阻塞。
- `proc_data` 用 `Weak`：pidfd 不应延长进程生命周期（否则无法回收 zombie）。
- 线程模式下 `thread_exit` 先于弱引用判断：线程退出但进程存活时，pidfd 立即"无效"（`process_data()` 返回 `NoSuchProcess`）。
- `non_blocking` 只影响本对象，不影响 `exit_event`。
- `poll` 用 `strong_count() > 0` 判断存活：`ProcessData` 被 `Arc` 持有（任务、表里是 Weak），最后一个任务退出后 `ProcessData` 可能仍被 pidfd 的 `Weak` 之外的地方持有（如父进程），因此这个判断是近似。

## 5. 与其他模块的关系

| 模块 | 关系 |
|---|---|
| `syscall/fs/pidfd.rs` | `pidfd_open/pidfd_getfd/pidfd_send_signal` |
| `task/ops.rs` | `do_exit` 唤醒 `exit_event` |
| `task/mod.rs` | `ProcessData.exit_event`、`Thread.exit_event/exit` |
| `syscall/task/clone.rs` | `CLONE_PIDFD` 创建并写 fd |
| `syscall/io_mpx/*` | 通过 `Pollable` 等待 |

## 6. 已知局限 / TODO

- `poll` 的 IN 语义（"存活"）与 Linux（"已退出"）相反；当前无调用方依赖，但若实现 `waitid(P_PIDFD)` 需要修正。
- `pidfd_getfd` 未做权限检查（Linux 要求 ptrace 权限）。
- 无 `PIDFD_NONBLOCK` 之外的 flags。
- `process_data()` 在进程已退出但 `ProcessData` 仍被持有时返回 `Ok`，可能让 `pidfd_send_signal` 发信号给已退出进程（`Process::exit` 后信号无意义）。

## 7. 变更影响面

改本文件 → `syscall/fs/pidfd.rs.md`、`task/ops.rs.md`、`syscall/task/clone.rs.md`、`CONSTRAINTS.md` C14。
