# `kernel/src/syscall/task/schedule.rs`

> 对应源码：`kernel/src/syscall/task/schedule.rs`
> 所属子系统：调度/睡眠/亲和性/优先级
> 依赖：`axtask::{AxCpuMask, current, future::{block_on, interruptible, sleep}}`、`linux_raw_sys::general::{CLOCK_*, TIMER_ABSTIME, PRIO_*, SCHED_RR, timespec}`、`starry_vm::{VmMutPtr, VmPtr, vm_load, vm_write_slice}`
> 被依赖：`syscall/task/mod.rs`、`syscall/mod.rs`

## 1. 文件职责

实现 `sched_yield/nanosleep/clock_nanosleep/sched_getaffinity/sched_setaffinity/sched_getscheduler/sched_setscheduler/sched_getparam/getpriority`。

## 2. 关键实现

### 2.1 睡眠

```rust
fn sleep_impl(clock, dur) -> TimeValue {
    let start = clock();
    let _ = block_on(interruptible(sleep(dur)));   // 被打断也继续
    clock() - start
}

sys_nanosleep(req, rem):
    req = try_into_time_value()?
    actual = sleep_impl(monotonic_time, req)
    若 req > actual（未睡够）：
        写 rem = req - actual（若有）
        Err(Interrupted)                          // EINTR
    否则 Ok(0)

sys_clock_nanosleep(clock_id, flags, req, rem):
    clock = REALTIME→wall_time | MONOTONIC→monotonic_time | 其他 InvalidInput
    dur = TIMER_ABSTIME ? req - clock() : req
    actual = sleep_impl(clock, dur)
    未睡够 → 写 rem + Interrupted
```

### 2.2 亲和性

| syscall | 行为 |
|---|---|
| `sched_getaffinity(pid, cpusetsize, mask)` | `cpusetsize*8 < cpu_num` → `InvalidInput`；`pid != 0` → `OperationNotPermitted`；写 `current().cpumask().as_bytes()`，返回字节数 |
| `sched_setaffinity(pid, cpusetsize, mask)` | 读 `min(cpusetsize, cpu_num.div_ceil(8))` 字节 → 构造 `AxCpuMask` → `set_current_affinity`（pid 忽略） |

### 2.3 其他

| syscall | 行为 |
|---|---|
| `sched_yield` | `axtask::yield_now()` |
| `sched_getscheduler` | 恒 `SCHED_RR` |
| `sched_setscheduler` | 返回 0（不生效） |
| `sched_getparam` | 返回 0（不写参数） |
| `getpriority(which, who)` | `PRIO_PROCESS/PGRP` 校验存在性后返回 20；`PRIO_USER` who=0 → 20，否则 `NoSuchProcess` |

## 3. 不变量与约束

- **C24**：`cpusetsize` 必须足够容纳 CPU 数（`*8 >= cpu_num`）；`nanosleep` 的 `timespec` 经 `TimeValueLike` 校验。
- `sleep_impl` 忽略 `interruptible` 的返回（被打断不提前返回），通过"实际睡了多少"判断 EINTR——这是**近似**实现（信号处理在睡眠期间可能发生）。
- `TIMER_ABSTIME` 的 `req - clock()` 用 `saturating_sub`（过去时间 → 0）。
- `sched_getaffinity` 只支持 `pid == 0`（当前线程）。
- `sched_setaffinity` 的 pid 被忽略（总是设置当前线程）。
- `getpriority` 恒 20（nice 的最小值），不反映真实调度优先级。

## 4. 与其他模块的关系

| 模块 | 关系 |
|---|---|
| `axtask::future` | `sleep`/`interruptible`/`block_on` |
| `task/signal.rs` | 打断睡眠的信号（`interruptible`） |
| `task/ops.rs` | `get_process_data/get_process_group` |
| `axhal::time` | `monotonic_time/wall_time` |
| `time.rs` | `TimeValueLike` |
| `syscall/task/mod.rs` | 门面 |

## 5. 已知局限 / TODO

- 调度策略/参数是占位（`SCHED_RR`、20）。
- `sched_setaffinity` 只对当前线程生效（pid 忽略）。
- `nanosleep` 的 EINTR 近似（睡眠被打断但实际时长可能不足；`interruptible` 的返回值被丢弃）。
- 无 `sched_getrlimit/sched_setrlimit`、`sched_rr_get_interval`。
- `PRIO_USER` 非 0 → `NoSuchProcess`（简化）。

## 6. 变更影响面

改本文件 → `axtask` 使用点、`task/signal.rs.md`、`time.rs.md`、`syscall/task/mod.rs.md`、`CONSTRAINTS.md` C24。
