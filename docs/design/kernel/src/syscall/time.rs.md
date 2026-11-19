# `kernel/src/syscall/time.rs`

> 对应源码：`kernel/src/syscall/time.rs`
> 所属子系统：时间类 syscall
> 依赖：`axhal::time::{TimeValue, monotonic_time, monotonic_time_nanos, nanos_to_ticks, wall_time}`、`linux_raw_sys::general::{timespec, timeval, itimerval, Tms}`、`time::TimeValueLike`
> 被依赖：`syscall/mod.rs`

## 1. 文件职责

实现 `clock_gettime/clock_getres/gettimeofday/times/getitimer/setitimer`。

## 2. 各函数行为

### `sys_clock_gettime(clock_id, ts)`

| clock_id | 时间源 |
|---|---|
| `CLOCK_REALTIME(_COARSE)` | `wall_time()` |
| `CLOCK_MONOTONIC(_RAW/_COARSE)/CLOCK_BOOTTIME` | `monotonic_time()` |
| `CLOCK_PROCESS_CPUTIME_ID/CLOCK_THREAD_CPUTIME_ID` | 当前线程 `TimeManager::output()` 的 `utime + stime` |
| 其他 | `warn!` + 回退 `wall_time()`（注释掉的 `EINVAL`） |

### 其他

| 函数 | 行为 |
|---|---|
| `sys_gettimeofday(ts)` | `timeval::from_time_value(wall_time())` |
| `sys_clock_getres(id, res)` | 非 MONOTONIC/REALTIME 只 `warn!`；写 `1µs` |
| `sys_times(tms)` | 当前线程 `utime/stime`（微秒），`tms_cutime/tms_cstime` 同；返回 `nanos_to_ticks(monotonic_time_nanos())` |
| `sys_getitimer(which, val)` | `ITimerType::from_repr` → `get_itimer` → 写 `itimerval` |
| `sys_setitimer(which, new, old)` | 解析 `it_interval/it_value`（ns）→ `set_itimer`；`new` 为 NULL 表示清空 |

## 3. 不变量与约束

- **C24**：`timespec/timeval/itimerval` 的读写必须经 `TimeValueLike`（`try_into_time_value` 校验非负/nsec 范围）。
- `clock_gettime` 的 "unsupported clock 回退 wall_time" 是有意的宽松（部分程序会传 `CLOCK_TAI` 等）；不要改成 `EINVAL` 除非确认不破坏用户程序。
- `CLOCK_PROCESS_CPUTIME_ID` 用**当前线程**的时间（不是进程所有线程之和）——已知不精确。
- `sys_times` 的返回是 tick 数（`nanos_to_ticks`），`tms_*` 是微秒；单位混用易错，改动时对照 Linux。
- `sys_setitimer` 的 `new_value == NULL` 必须清空定时器（`(0,0)`），这是 POSIX 语义。
- `sys_getitimer` 的 `which` 用 `ITimerType::from_repr` 校验（0/1/2）。

## 4. 与其他模块的关系

| 模块 | 关系 |
|---|---|
| `task/timer.rs` | `ITimerType`、`TimeManager::{output,get_itimer,set_itimer}` |
| `task/mod.rs` | `Thread.time` |
| `syscall/mod.rs` | 分派 |
| `time.rs`（mm 之外） | `TimeValueLike` |
| `syscall/resources.rs` | 同样用 `TimeValueLike` 转 `rusage` |

## 5. 已知局限 / TODO

- `CLOCK_PROCESS_CPUTIME_ID` 不汇总线程组。
- `clock_getres` 固定 1µs（实际 tick 10ms）。
- 无 `clock_settime`/`adjtimex`/`time`（秒级）。
- `times()` 的 children 时间是当前线程自身（未汇总子进程）。
- `CLOCK_TAI` 等未支持（回退 wall_time）。

## 6. 变更影响面

改本文件 → `task/timer.rs.md`、`task/stat.rs.md`（utime/stime 字段）、`time.rs.md`、`syscall/resources.rs.md`、`ARCHITECTURE.md#10`。
