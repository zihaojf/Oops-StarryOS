# `kernel/src/time.rs`

> 对应源码：`kernel/src/time.rs`
> 所属：时间/ABI 转换
> 依赖：`axhal::time::TimeValue`、`linux_raw_sys` 时间结构体
> 被依赖：`syscall/time.rs`、`syscall/net/opt.rs`、`syscall/resources.rs`、`syscall/signal.rs`、`syscall/io_mpx/*`、`syscall/mm/*` 等

## 1. 文件职责

提供 `TimeValueLike` trait：在内核的 `TimeValue` 与 Linux 的各种时间结构体之间**双向转换并校验**。这是 ABI 边界上的"类型胶水"，不涉及任何时钟读取。

## 2. 关键类型

```rust
pub trait TimeValueLike {
    fn from_time_value(tv: TimeValue) -> Self;
    fn try_into_time_value(self) -> AxResult<TimeValue>;
}
```

实现的 7 种类型：

| 类型 | 精度/字段 | 校验 |
|---|---|---|
| `TimeValue`（恒等） | — | 无 |
| `timespec`（64 位） | `tv_sec/tv_nsec` | `nsec ∈ [0, 1e9)`、`sec >= 0` |
| `__kernel_timespec` | 同上 | 同上 |
| `__kernel_old_timespec` | 同上（旧 ABI） | 同上 |
| `timeval` | `tv_sec/tv_usec` | `usec ∈ [0, 1e6)`、`sec >= 0`；`nsec = usec*1000` |
| `__kernel_old_timeval` | 同上 | 同上 |
| `__kernel_sock_timeval` | 同上（socket 超时） | 同上 |

## 3. 关键设计点

- **单向创建 + 双向校验**：`from_time_value` 永远成功（内核值合法），`try_into_time_value` 必须拒绝非法值（负秒/超范围纳秒），返回 `InvalidInput`（→ `EINVAL`）。这是 `nanosleep`、`clock_nanosleep`、`utimensat`、`poll/select/epoll` 超时参数正确性的唯一防线。
- **微秒/纳秒缩放**：`timeval` 用 `subsec_micros()` 与 `usec*1000`；避免引入除零或溢出（`usec < 1e6` 已校验）。
- **秒字段是 `i64`/`c_long`**：转换前先判 `< 0`，避免负数转 `u64` 变成巨大值。
- **不处理 `tv_nsec` 的特殊值**：`UTIME_NOW/UTIME_OMIT` 由调用方（`sys_utimensat`）在转换前判断，本 trait 不感知。

## 4. 不变量与约束

- **C24**：任何从用户结构体取时间的路径必须走 `try_into_time_value()`，不得手写字段读取后直接 `TimeValue::new`。
- `from_time_value` 不得 panic（纳秒部分来自内核，必然合法）。
- 新增时间结构体（如 `itimerspec`）时，在此实现并加入本文档表格。

## 5. 与其他模块的关系

- `syscall/time.rs`：`clock_gettime/gettimeofday/setitimer/getitimer`。
- `syscall/io_mpx/{poll,select,epoll}.rs`：超时参数。
- `syscall/signal.rs`：`rt_sigtimedwait` 超时。
- `syscall/net/opt.rs`：`SO_RCVTIMEO/SO_SNDTIMEO`。
- `syscall/resources.rs`：`rusage` 的 `__kernel_old_timeval`。
- `syscall/mm/mmap.rs` 等：无直接依赖，但 `TimeValue` 是 `alarm` 的基础。

## 6. 已知局限 / TODO

- 不支持 `timespec64` 之外的 32 位 `time_t` ABI（`__kernel_old_time_t` 直接 `as` 转换，假设 64 位平台）。
- 没有 `itimerspec` 的 `TimeValueLike`（`sys_timer_*` 目前是 stub）。

## 7. 变更影响面

改本文件 → 所有使用 `TimeValueLike` 的 syscall 文档（在各自"依赖"中列出），尤其 `syscall/time.rs.md`、`syscall/io_mpx/*.md`。
