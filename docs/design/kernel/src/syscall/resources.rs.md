# `kernel/src/syscall/resources.rs`

> 对应源码：`kernel/src/syscall/resources.rs`
> 所属子系统：资源限制 / 资源使用
> 依赖：`task::{Rlimits, get_process_data, get_task, Thread}`、`time::TimeValueLike`、`linux_raw_sys::general::{rlimit64, rusage}`
> 被依赖：`syscall/mod.rs`

## 1. 文件职责

实现 `prlimit64` 与 `getrusage`。

## 2. `sys_prlimit64(pid, resource, new_limit, old_limit)`

```text
resource >= RLIM_NLIMITS → InvalidInput
proc_data = get_process_data(pid)?                 // pid==0 → 当前进程
若 old_limit 非空 → 写 { rlim_cur: limit.current, rlim_max: limit.max }
若 new_limit 非空：
    读 rlimit64；rlim_cur > rlim_max → InvalidInput
    limit = &mut proc_data.rlim.write()[resource]
    若 new_limit.rlim_max <= limit.max → limit.max = new
    否则 → return Ok(0)（静默不生效，TODO）
    limit.current = new_limit.rlim_cur
```

## 3. `sys_getrusage(who, usage)`

| who | 统计 |
|---|---|
| `RUSAGE_SELF` | 当前进程所有线程的 `utime/stime` 之和 |
| `RUSAGE_CHILDREN` | 当前线程所在进程的其他线程（**不是子进程**，已知语义错误） |
| `RUSAGE_THREAD` | 当前线程 |
| 其他 | `InvalidInput` |

`Rusage` 用 `TimeValue` 表示，`collate` 累加；`From<Rusage> for rusage` 用 `__kernel_old_timeval`。

## 4. 不变量与约束

- **C24**：`resource` 必须先校验；`rlim[resource]` 越界会 panic（`Rlimits` 用数组索引）。
- `pid == 0` 语义由 `get_process_data(0)` → 当前进程提供。
- `new_limit.rlim_max > limit.max` 时**返回 Ok(0) 但不生效**（FIXME/TODO 注释）：这是已知的宽松实现；改它需要确认不会破坏依赖"设置失败"的程序。
- `RUSAGE_CHILDREN` 目前统计同进程线程，不是子进程（因为 `Process::children()` 需要 `Process` 而非线程）；修复时用 `proc.children()` 聚合。
- `rusage` 的其他字段全 0（`unsafe zeroed`）。
- `getrusage` 的 `get_task` 失败时跳过该线程（不报错）。

## 5. 与其他模块的关系

| 模块 | 关系 |
|---|---|
| `task/resources.rs` | `Rlimit/Rlimits/AX_FILE_LIMIT` |
| `task/mod.rs` | `ProcessData.rlim` |
| `task/ops.rs` | `get_process_data/get_task` |
| `task/timer.rs` | `TimeManager::output` |
| `time.rs` | `TimeValueLike` |
| `syscall/mod.rs` | 分派 |

## 6. 已知局限 / TODO

- `RUSAGE_CHILDREN` 语义错误（统计线程而非子进程）。
- 提高硬上限静默不生效。
- `rusage` 的 `ru_maxrss/ru_minflt/...` 未填。
- 无 `getrlimit`（仅 `prlimit64` 覆盖）。

## 7. 变更影响面

改本文件 → `task/resources.rs.md`、`task/timer.rs.md`、`time.rs.md`、`syscall/mod.rs.md`、`CONSTRAINTS.md` C24。
