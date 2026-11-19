# `kernel/src/task/user.rs`

> 对应源码：`kernel/src/task/user.rs`
> 所属子系统：用户态执行循环
> 依赖：`axhal::uspace::{UserContext, ReturnReason, ExceptionKind}`、`task::{check_signals, raise_signal_fatal, set_timer_state, unblock_next_signal, TimerState, AsThread}`
> 被依赖：`task/mod.rs`（重导出）、`entry.rs`、`syscall/task/clone.rs`

## 1. 文件职责

`new_user_task(name, uctx, set_child_tid)`：创建一个用户任务，其入口函数是**用户态主循环**——反复 `uctx.run()`，按返回原因分派 syscall / 缺页 / 异常，处理信号，直到 `pending_exit()`。

## 2. 关键流程（主循环）

```rust
TaskInner::new(move || {
    let curr = axtask::current();
    if let Some(tid) = (set_child_tid as *mut Pid).nullable() {
        tid.vm_write(curr.id().as_u64() as Pid).ok();   // CLONE_CHILD_SETTID
    }
    let thr = curr.as_thread();
    while !thr.pending_exit() {
        let reason = uctx.run();                        // 进入用户态
        set_timer_state(&curr, TimerState::Kernel);     // 回到内核，开始计 stime

        match reason {
            ReturnReason::Syscall => handle_syscall(&mut uctx),
            ReturnReason::PageFault(addr, flags) => {
                if !thr.proc_data.aspace.lock().handle_page_fault(addr, flags) {
                    raise_signal_fatal(SIGSEGV)          // 用户态缺页失败 → SIGSEGV
                }
            }
            ReturnReason::Interrupt => {}
            ReturnReason::Exception(exc) => { ... SIGBUS/SIGTRAP/SIGILL ... }
            r => { warn!; raise_signal_fatal(SIGSEGV) }
        }

        if !unblock_next_signal() {
            while check_signals(thr, &mut uctx, None) {}     // 清空待处理信号
        }

        set_timer_state(&curr, TimerState::User);            // 回用户态，开始计 utime
        curr.clear_interrupt();
    }
}, name, KERNEL_STACK_SIZE)
```

### 2.1 异常分派

| 异常 | 处理 |
|---|---|
| `Misaligned`（LoongArch） | 先尝试 `uctx.emulate_unaligned()`；成功则跳过，失败 `SIGBUS` |
| `Breakpoint` | `SIGTRAP` |
| `IllegalInstruction` | `SIGILL` |
| 其他 | `SIGTRAP` |

### 2.2 缺页

- **用户态**缺页（非 `access_user_memory` 触发）：走这里，`AddrSpace::handle_page_fault`，失败 `SIGSEGV`。
- **内核态**访问用户内存的缺页：由 `mm/access.rs` 的 trap handler 处理（不经过本分支）。

## 3. 不变量与约束

- **循环顺序不可改**：`run` → 计时器置 Kernel → 处理 → 信号 → 计时器置 User → `clear_interrupt`。顺序错了会导致 `utime/stime` 统计错误（C18）或信号在错误时机递送（C17）。
- `unblock_next_signal()` 返回 true 时**跳过**本次信号检查（`rt_sigreturn` 后抑制），否则会重新进入刚返回的信号。
- `set_child_tid` 写入使用 `nullable()`：0 表示不写；写失败被忽略（Linux 行为：CLONE_CHILD_SETTID 失败不报错）。
- 主循环只在 `pending_exit()` 为 false 时运行；`do_exit` 设置 `exit` 后，当前 `uctx.run()` 返回时循环退出，任务函数返回，`axtask` 回收。
- 所有 `raise_signal_fatal(...).expect(...)`：这里 panic 意味着 starry_signal 内部错误，属于不可恢复。
- 栈大小用 `config::KERNEL_STACK_SIZE`（256KiB）。

## 4. 与其他模块的关系

| 模块 | 关系 |
|---|---|
| `syscall/mod.rs` | `handle_syscall(&mut uctx)` |
| `mm/aspace/mod.rs` | `handle_page_fault` |
| `mm/access.rs` | 注册 `PAGE_FAULT` trap handler（内核访问用户内存的缺页） |
| `task/signal.rs` | `check_signals`/`raise_signal_fatal` |
| `task/timer.rs` | `TimerState` 切换 |
| `task/ops.rs` | `pending_exit`/`do_exit` |
| `entry.rs` | init 任务由本函数创建 |
| `syscall/task/clone.rs` | 新任务由本函数创建 |

## 5. 已知局限 / TODO

- `ReturnReason::Interrupt` 直接忽略（只在循环顶部通过 `pending_exit`/信号检查生效）。
- `// TODO: detailed handling`：异常只映射到少数信号，无 `SIGFPE`/`SIGSEGV` 错误码区分。
- 没有 `SIGSEGV` 的 `si_addr`（`SignalInfo::new_kernel` 不带地址）。
- 用户态被抢占时不改 `TimerState`（见 `task/timer.rs` TODO）。
- 单核假设：`uctx` 是任务私有，无并发问题。

## 6. 变更影响面

改本文件 → `task/timer.rs.md`、`task/signal.rs.md`、`mm/aspace/mod.rs.md`、`syscall/mod.rs.md`、`ARCHITECTURE.md#3/#6`、`CONSTRAINTS.md` C17/C18。
