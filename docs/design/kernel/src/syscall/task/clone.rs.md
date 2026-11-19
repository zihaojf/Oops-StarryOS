# `kernel/src/syscall/task/clone.rs`

> 对应源码：`kernel/src/syscall/task/clone.rs`
> 所属子系统：进程/线程创建
> 依赖：`task::{ProcessData, Thread, new_user_task, add_task_to_table, AsThread}`、`file::{FD_TABLE, FileLike, PidFd, close_file_like}`、`axfs::FS_CONTEXT`、`mm::copy_from_kernel`、`starry_process::Pid`
> 被依赖：`syscall/task/mod.rs`、`syscall/mod.rs`、`clone3.rs`

## 1. 文件职责

定义 `CloneFlags`（Linux `CLONE_*` 全集位标志）、`CloneArgs`（统一参数）与 `do_clone`，并实现 `sys_clone`/`sys_fork`。

## 2. 关键类型

```rust
bitflags! CloneFlags: u64 { VM, FS, FILES, SIGHAND, PIDFD, PTRACE, VFORK, PARENT, THREAD,
    NEWNS, SYSVSEM, SETTLS, PARENT_SETTID, CHILD_CLEARTID, UNTRACED, CHILD_SETTID,
    NEWCGROUP, NEWUTS, NEWIPC, NEWUSER, NEWPID, NEWNET, IO, CLEAR_SIGHAND, INTO_CGROUP, DETACHED }

pub struct CloneArgs {
    pub flags: CloneFlags, pub exit_signal: u64, pub stack: usize, pub tls: usize,
    pub parent_tid: usize, pub child_tid: usize, pub pidfd: usize,
}
```

## 3. `CloneArgs::validate`

```text
exit_signal > 0 且 (THREAD|PARENT) → InvalidInput
THREAD 但缺 VM 或 SIGHAND → InvalidInput
SIGHAND 但缺 VM → InvalidInput
VFORK|THREAD → InvalidInput
PIDFD|DETACHED → InvalidInput
namespace flags → warn（stub）
```

## 4. `do_clone(uctx)`

```text
validate()
若 VFORK：去掉 VM（慢路径：不共享地址空间）
exit_signal → Option<Signo>
new_uctx = *uctx
stack != 0 → set_sp(stack)
SETTLS → set_tls(tls)
new_uctx.set_retval(0)
set_child_tid = CHILD_SETTID ? child_tid : 0
new_task = new_user_task(&curr.name(), new_uctx, set_child_tid)
tid = new_task.id()
PARENT_SETTID → 写 parent_tid
若 THREAD:
    page_table_root = 共享父 aspace 的根；new_proc_data = old_proc_data.clone()
否则:
    proc = (PARENT ? 父的父 : 父).fork(tid)
    aspace = VM ? 父 aspace.clone() : 父 aspace.try_clone() + copy_from_kernel
    set_page_table_root(aspace.root)
    signal_actions = SIGHAND ? 共享 : (CLEAR_SIGHAND ? 默认 : 拷贝)
    proc_data = ProcessData::new(proc, exe_path, cmdline, aspace, signal_actions, exit_signal)
    proc_data.set_umask(父 umask); set_heap_top(父 heap_top)
    scope:
        FILES ? FD_TABLE.scope_mut(new).clone_from(&FD_TABLE)         // 共享表
              : FD_TABLE.scope_mut(new).write().clone_from(&FD_TABLE.read())  // 复制表
        FS    ? FS_CONTEXT.scope_mut(new).clone_from(&FS_CONTEXT)      // 共享 FsContext
              : FS_CONTEXT.scope_mut(new).lock().clone_from(&FS_CONTEXT.lock())  // 复制 cwd/root
    proc_data
new_proc_data.proc.add_thread(tid)
thr = Thread::new(tid, new_proc_data.clone())
CHILD_CLEARTID → set_clear_child_tid(child_tid)
PIDFD && pidfd != 0 → PidFd::new_thread/new_process → add_to_fd_table(true) → 写回 pidfd
*new_task.task_ext_mut() = Thread
spawn_task + add_task_to_table
返回 tid
```

## 5. `sys_clone` 与 `sys_fork`

```rust
const FLAG_MASK: u32 = 0xff;
clone_flags = CloneFlags::from_bits_truncate((flags & !FLAG_MASK) as u64)
exit_signal = (flags & FLAG_MASK) as u64
if PIDFD | PARENT_SETTID → InvalidInput
pidfd = PIDFD ? parent_tid : 0
args.do_clone(uctx)
// fork(x86_64) = sys_clone(uctx, SIGCHLD, 0, 0, 0, 0)
```

## 6. 不变量与约束

- **C19**：`validate` 的标志组合约束不可放宽。
- **C16**：新任务必须走 `spawn_task` + `add_task_to_table`，否则 `get_task` 找不到。
- **C11**：`FILES`/`FS` 的共享/复制必须通过 `scope_mut`（不能直接操作全局 `FD_TABLE`）。
- `THREAD` 子任务与父共享 `ProcessData`（相同 `proc`/`aspace`/`scope`）；`tid` 加入同一 `Process`。
- `CHILD_SETTID` 的写入发生在子任务**首次运行时**（`task/user.rs` 的入口闭包），不是 `do_clone` 里。
- `PARENT_SETTID` 在父上下文写 `parent_tid`（`vm_write`，失败 `.ok()` 忽略）。
- `CLONE_PIDFD` 失败时关闭已分配 fd（`close_file_like`）。
- `VFORK` 去 VM 但**没有**实现"父阻塞直到子 exec/exit"（vfork 语义不完整）。
- `stack` 非 0 时 `set_sp`；`sys_clone` 的 `child_tid` 在 x86_64/loongarch64 是第 5 参，其余架构第 6 参（ABI 差异）。
- `CLONE_PARENT` 时新进程的父是祖父（`old_proc_data.proc.parent()`）。

## 7. 与其他模块的关系

| 模块 | 关系 |
|---|---|
| `task/mod.rs` | `ProcessData/Thread/new_user_task` |
| `task/ops.rs` | `add_task_to_table` |
| `mm/aspace/mod.rs` | `try_clone` |
| `mm/loader.rs` | `copy_from_kernel` |
| `file/mod.rs` | `FD_TABLE`/`PidFd` |
| `axfs` | `FS_CONTEXT` |
| `starry_process` | `Process::fork/add_thread` |
| `task/futex.rs` | `ProcessData.futex_table`（新进程独立） |

## 8. 已知局限 / TODO

- VFORK 语义不完整（父不等待）。
- `CLONE_NEWNS/NEWPID/...` 只 warn（无 namespace）。
- `scope` 的共享实现依赖 `clone_from`（`Arc` 共享）与"新表 clone"（`RwLock::clone_from`）两套语义，理解成本高。
- `CLONE_PIDFD` 与 `PARENT_SETTID` 在 `sys_clone` 中不能同时用（复用 parent_tid）。
- 无线程组 TLS 的 `CLONE_SETTLS` 之外的架构差异（aarch64 的 TPIDR）。

## 9. 变更影响面

改本文件 → `syscall/task/{clone3,execve,wait}.rs.md`、`task/mod.rs.md`、`mm/aspace/mod.rs.md`、`file/mod.rs.md`、`CONSTRAINTS.md` C11/C16/C19、`ARCHITECTURE.md#4`。
