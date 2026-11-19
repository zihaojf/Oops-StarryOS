# `kernel/src/task/ops.rs`

> 对应源码：`kernel/src/task/ops.rs`
> 所属子系统：任务管理（表、退出、robust list）
> 依赖：`weak_map`、`axtask`、`starry_process`、`starry_signal`、`futex`
> 被依赖：`task/mod.rs`（重导出）、`task/signal.rs`、`syscall/task/*`、`pseudofs/proc.rs`、`pseudofs/dev/memtrack.rs`

## 1. 文件职责

1. 维护四张全局弱引用表：任务 / 进程 / 进程组 / 会话；
2. 提供查找：`get_task`、`get_process_data`、`get_process_group`、`get_session`、`tasks()`、`processes()`；
3. 定时器轮询与状态设置（`poll_timer`、`set_timer_state`）；
4. robust futex list 处理（`exit_robust_list`、`handle_futex_death`）；
5. **统一退出路径** `do_exit`。

## 2. 关键类型与数据

```rust
static TASK_TABLE:    RwLock<WeakMap<Pid, WeakAxTaskRef>>;
static PROCESS_TABLE: RwLock<WeakMap<Pid, Weak<ProcessData>>>;
static PROCESS_GROUP_TABLE: RwLock<WeakMap<Pid, Weak<ProcessGroup>>>;
static SESSION_TABLE: RwLock<WeakMap<Pid, Weak<Session>>>;
```

- `WeakMap`：键 → 弱引用；不阻止对象释放，`cleanup()` 清理失效条目。
- `add_task_to_table(task)` 级联插入：task → process → process group → session（已存在则提前 return）。
- `pid == 0` 的特殊处理：`get_task(0)`/`get_process_data(0)` 返回 `current()`（Linux `waitpid(0)` 等语义）。

### robust list 结构（与 Linux ABI 一致）

```rust
#[repr(C)] struct RobustList { next: *mut RobustList }
#[repr(C)] struct RobustListHead { list: RobustList, futex_offset: c_long, list_op_pending: *mut RobustList }
```

## 3. 关键流程

### 3.1 `do_exit(exit_code, group_exit)`

```text
1. clear_child_tid：写 0（vm_write），若成功则按 FutexKey::new_current 找到 futex 并 wake(1, u32::MAX)，然后 yield_now()
2. robust_list_head 非空 → exit_robust_list(head)
3. process.exit_thread(tid, exit_code) 返回 true（最后一个线程）时：
     process.exit()
     通知父进程：send_signal_to_process(parent, exit_signal) + parent.child_exit_event.wake()
     自身 exit_event.wake()
     SHM_MANAGER.lock().clear_proc_shm(pid)
4. thr.exit_event.wake()（线程级）
5. group_exit 且尚未 group_exited：
     process.group_exit()
     向同进程其他线程发送 SIGKILL
6. thr.set_exit()  —— 主循环据此退出
```

### 3.2 `exit_robust_list(head)`

- 读取用户内存的 `RobustListHead`（`vm_read`），遍历 `list.next` 直到回到 `&head.list`；
- 跳过 `list_op_pending`；对每个 entry 调 `handle_futex_death(entry, futex_offset)`：
  - `address = entry + offset`，取 `FutexKey::new_current(address)`；
  - 找到 futex entry → `owner_dead = true` 并 `wq.wake(1, u32::MAX)`。
- 循环上限 `ROBUST_LIST_LIMIT`（`linux_raw_sys`），超出返回 `FilesystemLoop`（防恶意环）；
- 每次迭代 `axtask::yield_now()`（避免长时间独占）。

### 3.3 `poll_timer` / `set_timer_state`

```rust
let Ok(mut time) = thr.time.try_borrow_mut() else { return; }; // 重入（IRQ）直接跳过
time.poll(|signo| send_signal_thread_inner(task, thr, SignalInfo::new_kernel(signo)));
time.set_state(state); // 仅 set_timer_state
```

## 4. 不变量与约束

- **C16**：任务退出只能走 `do_exit`；任何自行 `set_exit()` 都会跳过父进程通知/SHM 清理。
- **C34**：四张表必须存 `Weak`；改 `Arc` 会导致进程/任务永不释放。
- `add_task_to_table` 的级联 return 依赖"进程/组/会话已存在则其成员也已登记"的假设；若将来支持 `setpgid` 到新组，需重新审视。
- `do_exit` 中 `SHM_MANAGER` 与 `process` 的操作不能同时持有 `PROCESS_TABLE` 读锁（当前实现先取 `proc` 的 `Arc` 再释放表锁）。
- robust list 遍历必须限制次数且可被调度（yield），否则恶意用户可制造无限循环。
- `poll_timer`/`set_timer_state` 在 IRQ 重入时静默返回，**不能**改成 panic。

## 5. 与其他模块的关系

- `task/signal.rs`：`send_signal_thread_inner` 被 `poll_timer`/`set_timer_state` 复用；退出时父进程通知。
- `task/futex.rs`：`FutexKey`、`futex_table_for`。
- `task/user.rs`：主循环检查 `pending_exit()`。
- `syscall/task/exit.rs`：`sys_exit` → `do_exit(code<<8, false)`；`sys_exit_group` → `true`。
- `syscall/task/wait.rs`：`child_exit_event` 唤醒来源。
- `pseudofs/proc.rs`：`tasks()`/`get_task`。
- `pseudofs/dev/memtrack.rs`：`cleanup_task_tables()`。

## 6. 已知局限 / TODO

- `do_exit` 的 `group_exit` 只对"尚未 group_exited"的进程广播一次 `SIGKILL`；被阻塞在不可中断等待的线程无法退出。
- `exit_robust_list` 的 `FutexKey::new_current` 使用当前进程地址空间；对共享映射的 robust futex 处理不完整。
- 没有 `PF_EXITING` 类似的"退出中"状态，重入 `do_exit` 未防护。
- 表清理依赖 `WeakMap::cleanup`（仅在 memtrack 时主动调用）。

## 7. 变更影响面

改本文件 → `task/mod.rs.md`、`task/signal.rs.md`、`syscall/task/{exit,wait,clone}.rs.md`、`pseudofs/proc.rs.md`、`ARCHITECTURE.md#4.3`、`CONSTRAINTS.md` C16/C34。
