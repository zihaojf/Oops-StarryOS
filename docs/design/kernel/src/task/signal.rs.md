# `kernel/src/task/signal.rs`

> 对应源码：`kernel/src/task/signal.rs`
> 所属子系统：信号
> 依赖：`starry_signal`（`SignalInfo/SignalOSAction/SignalSet`）、`axtask`
> 被依赖：`task/mod.rs`（重导出）、`task/user.rs`、`syscall/signal.rs`、`file/pipe.rs`、`pseudofs/dev/tty/terminal/ldisc.rs`、`syscall/task/ops` 等

## 1. 文件职责

信号子系统的**投递与检查**入口：

1. `check_signals`：在用户态返回路径上检查并执行一个待处理信号；
2. 三个投递方向：线程 / 进程 / 进程组；
3. `raise_signal_fatal`：内核触发的致命信号（缺页失败、非法指令等）；
4. `block_next_signal`/`unblock_next_signal`：`rt_sigreturn` 后抑制一次检查；
5. `with_blocked_signals`：临时替换阻塞掩码执行闭包（`epoll_pwait/ppoll/pselect6` 用）。

## 2. 关键流程

### 2.1 `check_signals(thr, uctx, restore_blocked) -> bool`

```text
let Some((sig, os_action)) = thr.signal.check_signals(uctx, restore_blocked) else { return false };
match os_action {
    Terminate  => do_exit(signo as i32, true),
    CoreDump   => do_exit(128 + signo, true),      // TODO: 无 core dump
    Stop       => do_exit(1, true),                // TODO: 无 stop
    Continue   => {}                               // TODO
    Handler    => {}                               // starry_signal 已改好 uctx
}
true
```

- `restore_blocked`：`rt_sigtimedwait`/`rt_sigsuspend` 需要在处理完信号后恢复旧掩码。
- 返回值 `true` 表示"处理了一个信号"，调用方（`user.rs`）用 `while check_signals(...) {}` 清空所有待处理信号。

### 2.2 投递

```rust
send_signal_to_thread(tgid: Option<Pid>, tid, sig)  // 校验 tgid 归属；task.interrupt()
send_signal_to_process(pid, sig)                    // 进程级管理器选择线程；interrupt
send_signal_to_process_group(pgid, sig)             // 遍历组内进程逐个投递
raise_signal_fatal(sig)                             // 投递给当前进程；无人接收则 do_exit(signo, true)
```

`send_signal_thread_inner` 是内部实现：`thr.signal.send_signal(sig)` 成功才 `task.interrupt()`。

### 2.3 `with_blocked_signals`

```rust
let old_blocked = blocked.map(|set| sig.set_blocked(set));  // 若给了新掩码
f().inspect(|_| { if let Some(old) = old_blocked { sig.set_blocked(old); } })
```

注意：**只在 `Ok` 时恢复**（`inspect` 不处理 `Err`）——错误路径会保留新掩码。这是已知行为，调用方（`epoll_pwait` 等）的错误多为参数错误（在进入等待前就返回），影响有限，但改动时需注意。

## 3. 不变量与约束

- **C17**：实际的用户栈帧构造由 `starry_signal` 的 `check_signals` 完成；本文件只做 OS action 分派。不要在别处直接改 `UserContext`。
- `BLOCK_NEXT_SIGNAL_CHECK` 是**全局** `AtomicBool`（不是每线程）；多线程下 `rt_sigreturn` 可能抑制其他线程的一次检查。单核串行假设下可接受，SMP 下是隐患。
- `Terminate/CoreDump/Stop` 都调用 `do_exit`（`Stop` 实际是退出，不是暂停），`Continue` 无操作——这些是**未实现**的语义，返回给用户的行为与 Linux 不同。
- `send_signal_to_process` 只让 `proc_data.signal.send_signal` 选出的一个线程 `interrupt()`；其余线程若阻塞不会被唤醒（Linux 由信号投递时唤醒整个线程组）。单核下通常够用。
- `send_signal_to_thread(tgid,...)` 会校验 `proc.pid() == tgid`，防止跨进程 `tgkill`。

## 4. 与其他模块的关系

| 模块 | 关系 |
|---|---|
| `task/user.rs` | 主循环末尾 `while check_signals(...) {}` |
| `task/ops.rs` | `poll_timer`/`set_timer_state` 用 `send_signal_thread_inner`；`do_exit` 通知父进程 |
| `syscall/signal.rs` | `kill/tkill/tgkill/rt_sigqueueinfo/rt_sigtimedwait/rt_sigsuspend` |
| `file/pipe.rs` | 写端关闭时 `raise SIGPIPE` |
| `pseudofs/dev/tty/terminal/ldisc.rs` | 前台进程组发 `SIGINT/SIGQUIT` |
| `syscall/io_mpx/*` | `with_blocked_signals` |

## 5. 已知局限 / TODO

- `CoreDump` 未实现（直接退出码 128+signo）。
- `Stop`/`Continue` 未实现作业控制（`do_exit` 代替）。
- 无实时信号排队顺序保证（依赖 `starry_signal`）。
- `block_next_signal` 非 per-thread。

## 6. 变更影响面

改本文件 → `task/user.rs.md`、`task/ops.rs.md`、`syscall/signal.rs.md`、`PRINCIPLES.md`（P8 未实现标注）、`CONSTRAINTS.md` C17。
