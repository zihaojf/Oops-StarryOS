# `kernel/src/syscall/signal.rs`

> 对应源码：`kernel/src/syscall/signal.rs`
> 所属子系统：信号 syscall
> 依赖：`starry_signal`、`task::{check_signals, send_signal_to_*, block_next_signal}`、`time::TimeValueLike`、`linux_raw_sys::general::*`
> 被依赖：`syscall/mod.rs`

## 1. 文件职责

实现 POSIX 信号相关 syscall 的参数/ABI 层：

| syscall | 实现 |
|---|---|
| `rt_sigprocmask` | `check_sigset_size`；读 oldset；按 `how`（`SIG_BLOCK/UNBLOCK/SETMASK`）计算新掩码 |
| `rt_sigaction` | 校验 signo；拒绝 `SIGKILL/SIGSTOP`；读写 `ProcSignalManager.actions[signo]` |
| `rt_sigpending` | 写当前线程 `pending()` |
| `kill` | pid>0 → 进程；0 → 本进程组；-1 → 除 init 与自己外所有进程；< -1 → 进程组 `-pid` |
| `tkill` | 线程；`tgkill` | 线程 + 校验 tgid |
| `rt_sigqueueinfo`/`rt_sigtgsigqueueinfo` | 读取用户 `siginfo`，校验同进程/权限，投递 |
| `rt_sigreturn` | `block_next_signal()` + `signal.restore(uctx)` + 返回 `uctx.retval()` |
| `rt_sigtimedwait` | 临时解除 mask 等待指定信号（`poll_fn` + `dequeue_signal` + `check_signals`） |
| `rt_sigsuspend` | 替换 mask 并等待任意信号，恒返回 `EINTR` |
| `sigaltstack` | 读写 `SignalStack`；`size <= MINSIGSTKSZ` → `NoMemory` |

辅助：`check_sigset_size`（`sigsetsize` 必须 0 或 `size_of::<SignalSet>()`）、`parse_signo`、`make_siginfo`、`make_queue_signal_info`。

## 2. 关键流程

### 2.1 `rt_sigtimedwait`

```text
old_blocked = signal.blocked()
signal.set_blocked(old_blocked & !set)     // 临时解除被等待信号的屏蔽
uctx.set_retval(-EINTR)                    // 若被其他信号打断，返回值是 -EINTR
poll_fn:
  从 set 中 dequeue 到信号 → 恢复 blocked，Ready(Some(sig))
  check_signals(thr, uctx, Some(old_blocked)) → Ready(None)   // 被其他信号处理打断
  否则 poll_interrupt + Pending
timeout 到期 → 恢复 blocked，返回 WouldBlock
Some(sig) → 写 info（可选），返回 signo；None → Ok(0)
```

### 2.2 `kill` 的 pid 语义

| pid | 目标 |
|---|---|
| `> 0` | 该进程 |
| `0` | 调用者所在进程组 |
| `-1` | 除 init 与调用者外的所有进程 |
| `< -1` | 进程组 `-pid` |

## 3. 不变量与约束

- **C17**：`check_signals` 会修改 `uctx` 构造信号帧；本文件只做参数转换与投递，不直接改寄存器。
- **C24**：`sigsetsize` 必须校验（`check_sigset_size`），否则越界读写用户内存。
- `rt_sigaction` 必须拒绝 `SIGKILL/SIGSTOP`（`InvalidInput`）。
- `make_queue_signal_info` 的权限校验：非本进程且 `code >= 0 || code == SI_TKILL` → `OperationNotPermitted`（防止伪造信号来源）。
- `rt_sigreturn` 的 `block_next_signal()` 必须在这之后返回 `uctx.retval()`；漏掉会导致信号重复递送循环。
- `rt_sigsuspend` 恒返回 `Err(Interrupted)`（`-EINTR`），即使信号被处理。
- **`kill(-1)` 跳过 init 与自己**（POSIX 要求；init 不应被杀）。

## 4. 与其他模块的关系

| 模块 | 关系 |
|---|---|
| `task/signal.rs` | `check_signals`/投递/`send_signal_to_*` |
| `task/mod.rs` | `Thread.signal`、`ProcessData.signal.actions` |
| `syscall/mod.rs` | 分派 |
| `starry_signal` | `SignalSet/SignalInfo/SignalAction/SignalStack` |
| `syscall/fs/pidfd.rs` | 复用 `make_queue_signal_info` |
| `time.rs` | `rt_sigtimedwait` 的超时转换 |

## 5. 已知局限 / TODO

- `sigaltstack` 的 `SS_DISABLE/SS_ONSTACK` 标志未处理（只校验 size）。
- `rt_sigtimedwait` 的 timeout 语义与 `clock_nanosleep` 一致的 `EINTR` 细节未完全对齐。
- `rt_sigreturn` 后不再检查剩余信号（由 `task/user.rs` 的 `unblock_next_signal` 控制）。
- 实时信号的排队/`sigqueue` 参数（`si_ptr` 等）依赖 `starry_signal`。
- `kill(-1)` 用 `processes()` 快照，期间新进程不会被通知（可接受）。

## 6. 变更影响面

改本文件 → `task/signal.rs.md`、`task/user.rs.md`、`syscall/mod.rs.md`、`CONSTRAINTS.md` C17/C24。
