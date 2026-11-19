# `kernel/src/task/mod.rs`

> 对应源码：`kernel/src/task/mod.rs`
> 所属子系统：任务 / 线程 / 进程数据
> 依赖：`starry_process`、`starry_signal`、`scope_local`、`axtask`、`axpoll`
> 被依赖：`entry.rs`、全部 `syscall/**`、`file/**`、`pseudofs/proc.rs`、`mm/**`

## 1. 文件职责

任务子系统的**类型中枢**：

1. 声明并重导出子模块（`futex/ops/resources/signal/stat/timer/user`）；
2. 定义 `Thread`（线程扩展）与 `ProcessData`（进程共享数据）；
3. 通过 `extern_trait` 为 `Box<Thread>` 实现 `axtask::TaskExt`，实现任务切入/切出时的 scope 切换；
4. 提供 `AsThread` 便捷 trait。

## 2. 关键类型

### 2.1 `Thread`（每线程，存于 `TaskInner::task_ext`）

| 字段 | 类型 | 作用 |
|---|---|---|
| `proc_data` | `Arc<ProcessData>` | 所属进程的共享数据 |
| `clear_child_tid` | `AtomicUsize` | `set_tid_address`/`CLONE_CHILD_CLEARTID` 的地址；退出时清零并唤醒 futex |
| `robust_list_head` | `AtomicUsize` | `set_robust_list` 的链表头 |
| `signal` | `Arc<ThreadSignalManager>` | 线程级信号（阻塞掩码/待处理/信号栈） |
| `time` | `AssumeSync<RefCell<TimeManager>>` | 时间统计；`AssumeSync` 因"只在上下文切换时独占借用"而声明 Sync |
| `oom_score_adj` | `AtomicI32` | `/proc/<pid>/oom_score_adj`，默认 200 |
| `exit` | `Arc<AtomicBool>` | 待退出标志；`pending_exit()` 在主循环里检查 |
| `accessing_user_memory` | `AtomicBool` | 是否处于 `access_user_memory` 作用域（page fault 门禁） |
| `exit_event` | `Arc<PollSet>` | 线程退出事件（`PidFd::new_thread` 用） |

### 2.2 `ProcessData`（每进程，`Arc` 共享）

| 字段 | 类型 | 作用 |
|---|---|---|
| `proc` | `Arc<Process>` | `starry_process` 进程对象（pid/父子/线程集合/退出码） |
| `exe_path` | `RwLock<String>` | 可执行文件绝对路径（`/proc/<pid>/exe`） |
| `cmdline` | `RwLock<Arc<Vec<String>>>` | 命令行（`/proc/<pid>/cmdline`） |
| `aspace` | `Arc<Mutex<AddrSpace>>` | 用户地址空间 |
| `scope` | `RwLock<Scope>` | 资源作用域（`FD_TABLE`、`FS_CONTEXT` 的宿主） |
| `heap_top` | `AtomicUsize` | `brk` 当前堆顶，初值 `USER_HEAP_BASE` |
| `rlim` | `RwLock<Rlimits>` | 资源限制（NOFILE/STACK 有初值） |
| `child_exit_event` | `Arc<PollSet>` | 子进程退出事件（`wait4` 等待） |
| `exit_event` | `Arc<PollSet>` | 进程退出事件（`PidFd::new_process`） |
| `exit_signal` | `Option<Signo>` | 退出时发给父进程的信号；`None` 表示 init 或 clone 子进程 |
| `signal` | `Arc<ProcessSignalManager>` | 进程级信号（动作表/待处理/trampoline） |
| `futex_table` | `Arc<FutexTable>` | 私有 futex 表（每进程一张） |
| `umask` | `AtomicU32` | 文件权限掩码，默认 `0o022` |

### 2.3 `AsThread` trait

```rust
try_as_thread(&self) -> Option<&Thread>   // 内核任务返回 None
as_thread(&self) -> &Thread               // 内核任务 panic "kernel task"
```

所有需要访问进程数据的代码都通过 `current().as_thread()`。

## 3. 关键流程

### 3.1 scope 切换（`TaskExt for Box<Thread>`）

```rust
fn on_enter(&self) {
    let scope = self.proc_data.scope.read();
    unsafe { ActiveScope::set(&scope) };
    core::mem::forget(scope);        // 故意泄漏读锁：on_leave 手工递减
}
fn on_leave(&self) {
    ActiveScope::set_global();
    unsafe { self.proc_data.scope.force_read_decrement() };
}
```

- 进入任务：把该进程的 `Scope` 设为 active，此后 `FD_TABLE`/`FS_CONTEXT` 自动指向它。
- 离开任务：恢复 global scope 并手工释放读锁。
- **`mem::forget` + `force_read_decrement` 是配对契约**，改动一处必须改另一处，否则 scope 锁计数错乱。

### 3.2 创建

`Thread::new(tid, proc_data)` 初始化所有原子字段与信号管理器；`ProcessData::new` 用 `Arc::new` 构造并初始化 `ProcessSignalManager`（trampoline 地址取自 `config::SIGNAL_TRAMPOLINE`）。

## 4. 不变量与约束

- `on_enter`/`on_leave` **必须成对**，且 `on_leave` 的 `force_read_decrement` 恰好对应 `on_enter` 的 `mem::forget`（C34 相关）。
- `Thread.time` 是 `RefCell`，只在"当前任务独占"时借用；禁止跨任务借用（`task/ops.rs` 的 `try_borrow_mut` 会静默跳过 IRQ 重入）。
- `ProcessData` 的字段用 `RwLock`/`Atomic` 单独保护，**没有**整体 `Mutex`；修改多字段要考虑一致性（如 `cmdline` 与 `exe_path` 在 execve 中一起改）。
- `umask` 用 `SeqCst` 原子；`replace_umask` 返回旧值（`sys_umask` 语义）。
- `is_clone_child()` 定义为 `exit_signal != Some(SIGCHLD)`（Linux 对 "clone child" 的定义）。
- `futex_table` 每进程一张；共享 futex 走全局表（见 `task/futex.rs`）。

## 5. 与其他模块的关系

| 模块 | 关系 |
|---|---|
| `entry.rs` | 创建 init 的 `ProcessData`/`Thread`，绑定 N_TTY |
| `task/ops.rs` | 表管理（task/process/pg/session）、`do_exit` |
| `task/user.rs` | `new_user_task` 主循环 |
| `task/signal.rs` | 使用 `Thread.signal` / `ProcessData.signal` 投递 |
| `task/futex.rs` | `ProcessData.futex_table` |
| `task/timer.rs` | `Thread.time` |
| `file/mod.rs` | `FD_TABLE.scope_mut(&proc_data.scope)` |
| `syscall/task/clone.rs` | 构造新 `ProcessData`（决定 FILES/FS/SIGHAND 共享） |
| `pseudofs/proc.rs` | 读 `exe_path/cmdline/oom_score_adj`、fd 表 |

## 6. 已知局限 / TODO

- `// TODO: scopify`（`aspace` 注释）：地址空间尚未纳入 scope，`CLONE_VM` 通过 `Arc` 共享实现。
- `TimeManager` 的 "preempting does not change the timer state"（`timer.rs` 注释）。
- `oom_score_adj` 只存不用（没有 OOM killer）。
- 无 per-thread 名字的独立存储（名字在 `TaskInner`）。

## 7. 变更影响面

改本文件 → `task/*.md` 全部、`entry.rs.md`、`syscall/task/clone.rs.md`、`syscall/task/execve.rs.md`、`ARCHITECTURE.md#4`、`CONSTRAINTS.md`（若改 scope/退出契约）。
