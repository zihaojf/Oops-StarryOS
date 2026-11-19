# `kernel/src/task/timer.rs`

> 对应源码：`kernel/src/task/timer.rs`
> 所属子系统：时间 / 定时器 / 时间统计
> 依赖：`axhal::time`、`axtask::future`、`event_listener`、`BinaryHeap`、`starry_signal`
> 被依赖：`task/mod.rs`（重导出）、`task/ops.rs`、`task/user.rs`、`syscall/time.rs`、`syscall/resources.rs`、`entry.rs`

## 1. 文件职责

1. `TimeManager`：每线程的用户/系统时间统计与 3 个 interval timer；
2. `ITimerType`：`Real/Virtual/Prof` → 信号 `SIGALRM/SIGVTALRM/SIGPROF`；
3. 全局 alarm 定时器堆与驱动任务：到期时在目标任务上下文里推进时间并发信号；
4. `spawn_alarm_task()`：启动驱动任务。

## 2. 关键类型

```rust
pub enum ITimerType { Real = 0, Virtual = 1, Prof = 2 }   // 值与 Linux ITIMER_* 一致
struct ITimer { interval_ns: usize, remained_ns: usize }
pub enum TimerState { None, User, Kernel }
pub struct TimeManager {
    utime_ns: usize, stime_ns: usize, last_wall_ns: usize,
    state: TimerState, itimers: [ITimer; 3],
}
struct Entry { deadline: Duration, task: WeakAxTaskRef }   // BinaryHeap 按 deadline 小顶堆
lazy_static! {
    static ref ALARM_LIST: Mutex<BinaryHeap<Entry>> = ...;
    static ref EVENT_NEW_TIMER: Event = ...;
}
```

## 3. 关键流程

### 3.1 `TimeManager::poll(emitter)`

```text
now_ns = monotonic_time_nanos()
delta = now_ns - last_wall_ns
match state {
  User   => utime += delta; update_itimer(Virtual, delta); update_itimer(Prof, delta);
  Kernel => stime += delta; update_itimer(Prof, delta);
  None   => {}
}
update_itimer(Real, delta)          // 任何状态都推进 Real
last_wall_ns = now_ns
```

`update_itimer`：`ITimer::update(delta)` 到期则 `emitter(signo)`（由 `task/ops.rs` 转成 `send_signal_thread_inner`），并 `renew_timer()` 把下一次 deadline 推进堆。

### 3.2 `ITimer::renew_timer()`

```text
if remained_ns > 0 {
    deadline = wall_time() + remained_ns
    push Entry{deadline, WeakAxTaskRef(current())}
    若新 deadline 是堆顶 → EVENT_NEW_TIMER.notify(1)
}
```

- 用 **wall_time**（而非 monotonic）作为堆排序键；`TimeManager::poll` 用 monotonic 计算 delta。两者混用是现状（对绝对/相对定时器的语义差异不敏感）。

### 3.3 `alarm_task()`

```text
loop {
  取堆顶：
    空 → 等 EVENT_NEW_TIMER（先注册 listener，再复查，避免丢事件）
    到期 → 升级 WeakAxTaskRef；drop 锁；poll_timer(&task)；再锁 pop（断言弹出的是同一 deadline）
    未到期 → 等 EVENT_NEW_TIMER 或 timeout_at(deadline)（用 event_listener）
}
```

- `poll_timer(task)`（`task/ops.rs`）用 `try_borrow_mut` 借用目标任务 `TimeManager`，重入/IRQ 时跳过。
- `WeakAxTaskRef` 保证已退出任务不会因定时器泄漏。

### 3.4 `spawn_alarm_task()`

`axtask::spawn_raw(|| block_on(alarm_task()), "alarm_task", axconfig::TASK_STACK_SIZE)`；由 `entry.rs` 在创建 init 之前调用。

## 4. 不变量与约束

- **C18**：`TimerState` 必须由 `task/user.rs` 成对设置（进内核 `Kernel`，回用户 `User`）；`TimeManager::poll` 只有在 `User/Kernel` 状态才累计对应时间。
- `ALARM_LIST` 的 `Entry` 用 `Weak`，`poll_timer` 前必须 `upgrade()`；失败则丢弃该 entry。
- `alarm_task` 的"先注册 listener 再复查堆"模式不能简化成"先查再等"，否则会漏掉新定时器（经典 lost-wakeup）。
- `wall_time()` 与 `monotonic_time_nanos()` 混用：若系统时间被设置（`settimeofday` 尚未实现），堆排序可能异常；改动需评估。
- `ITimer::update` 在到期时重置 `remained_ns = interval_ns`，`interval_ns == 0` 时不会再次入堆（一次性定时器语义）。
- `spawn_alarm_task` 只能调用一次（`lazy_static` 与堆的全局性）。

## 5. 与其他模块的关系

| 模块 | 关系 |
|---|---|
| `task/mod.rs` | `Thread.time: AssumeSync<RefCell<TimeManager>>` |
| `task/ops.rs` | `poll_timer`/`set_timer_state` |
| `task/user.rs` | 每次 syscall/page fault 前后切换状态 |
| `syscall/time.rs` | `setitimer/getitimer`、`clock_gettime(PROCESS/THREAD_CPUTIME_ID)`、`times()` |
| `syscall/resources.rs` | `getrusage` 读 `utime/stime` |
| `entry.rs` | `spawn_alarm_task()` |

## 6. 已知局限 / TODO

- `// TODO(mivik): preempting does not change the timer state currently`：抢占不切换状态，用户态被抢占时时间可能计入内核。
- `wall_time` 与 `monotonic_time` 混用（见上）。
- 无纳秒精度保证（`ticks-per-sec = 100`）。
- `ITIMER_VIRTUAL/PROF` 只在 `poll` 时机更新，没有高精度保证。
- 没有 `timer_create/timer_settime`（POSIX timer，syscall 层是 stub）。

## 7. 变更影响面

改本文件 → `task/ops.rs.md`、`task/user.rs.md`、`syscall/time.rs.md`、`syscall/resources.rs.md`、`ARCHITECTURE.md#10`、`CONSTRAINTS.md` C18。
