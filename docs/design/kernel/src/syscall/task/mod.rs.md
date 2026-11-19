# `kernel/src/syscall/task/mod.rs`

> 对应源码：`kernel/src/syscall/task/mod.rs`
> 所属子系统：任务/进程 syscall（门面）
> 依赖：`clone/clone3/ctl/execve/exit/job/schedule/thread/wait`

## 1. 文件职责

```rust
mod clone; mod clone3; mod ctl; mod execve; mod exit; mod job; mod schedule; mod thread; mod wait;
pub use self::{clone::*, clone3::*, ctl::*, execve::*, exit::*, job::*, schedule::*, thread::*, wait::*};
```

| 文件 | syscall |
|---|---|
| `clone.rs` | `clone`、`fork`(x86_64)、`CloneArgs`/`CloneFlags` |
| `clone3.rs` | `clone3` |
| `ctl.rs` | `capget/capset/umask/setreuid/setresuid/setresgid/get_mempolicy/prctl` |
| `execve.rs` | `execve` |
| `exit.rs` | `exit/exit_group` |
| `job.rs` | `getsid/setsid/getpgid/setpgid` |
| `schedule.rs` | `sched_yield/nanosleep/clock_nanosleep/sched_*affinity/sched_get/setscheduler/sched_getparam/getpriority` |
| `thread.rs` | `getpid/getppid/gettid/set_tid_address/arch_prctl` |
| `wait.rs` | `wait4` |

## 2. 不变量与约束

- 进程/线程的创建/退出必须走 `CloneArgs::do_clone` / `do_exit`（C16）。
- 所有 pid 参数必须是 `Pid`（`starry_process`），`0`/`-1` 的语义在各实现中单独处理。
- 新增 task 类 syscall 时同步 `syscall/mod.rs`。

## 3. 变更影响面

改本文件 → `syscall/task/*.md`、`syscall/mod.rs.md`、`TRACEABILITY.md`。
