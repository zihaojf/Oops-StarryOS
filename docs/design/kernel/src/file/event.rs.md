# `kernel/src/file/event.rs`

> 对应源码：`kernel/src/file/event.rs`
> 所属子系统：eventfd
> 依赖：`axpoll::{PollSet, Pollable}`、`axtask::future::{block_on, poll_io}`
> 被依赖：`file/mod.rs`（`event` 模块）、`syscall/fs/event.rs`（`sys_eventfd2`）

## 1. 文件职责

实现 Linux `eventfd`：一个 64 位无符号计数器，可被 read/write、poll/epoll 等待，用于进程/线程间事件通知。

```rust
pub struct EventFd {
    count: AtomicU64,
    semaphore: bool,          // EFD_SEMAPHORE
    non_blocking: AtomicBool,
    poll_rx: PollSet, poll_tx: PollSet,
}
```

## 2. 语义

| 操作 | 普通模式 | semaphore 模式 |
|---|---|---|
| `read` | 读出当前值并**清零** | 读出 1 并将计数减 1 |
| `write(v)` | 计数 += v（溢出检查） | 同左 |
| `poll IN` | `count > 0` | 同左 |
| `poll OUT` | `u64::MAX - 1 > count` | 同左 |

- `read` 必须至少 8 字节缓冲，返回 8（`count.to_ne_bytes()`）。
- `write` 必须至少 8 字节输入；`value == u64::MAX` 返回 `InvalidInput`（Linux 语义）。
- 计数加满检查用 `u64::MAX - count > value`，超出返回 `WouldBlock`（阻塞模式下会等待）。

## 3. 关键流程

```text
read:  block_on(poll_io(self, IN, nonblocking, || {
           fetch_update: count>0 时返回 count - (semaphore?1:count)
           Ok(old) → dst.write(old.to_ne_bytes()); poll_tx.wake(); Ok(8)
           Err    → Err(WouldBlock)
       }))
write: 读取 8 字节；value==MAX → InvalidInput
       block_on(poll_io(self, OUT, nonblocking, || {
           fetch_update: u64::MAX - count > value 时 count+value
           Ok(_) → poll_rx.wake(); Ok(8)
           Err   → Err(WouldBlock)
       }))
```

`Pollable::poll` 用 `count.load(Acquire)` 计算 IN/OUT；`register` 按事件注册对应 `PollSet`。

## 4. 不变量与约束

- **C13**：阻塞/非阻塞语义由 `poll_io` + `nonblocking()` 决定；`nonblocking=false` 时 `read` 在计数为 0 时会挂起。
- `fetch_update` 的 `Ordering::Release/Acquire` 是配对（写端 Release、读端 Acquire），不要改。
- 计数器溢出检查 `u64::MAX - count > value`（严格大于）：允许 `count+value == u64::MAX`，与 Linux 一致。
- `value == u64::MAX` 必须拒绝（Linux 保留该值）。
- `Pollable::poll` 的 OUT 条件是 `u64::MAX - 1 > count`（不是 `u64::MAX - count > 0`），与写检查略有差异，属于现状（保守：接近满时不报告可写）。

## 5. 与其他模块的关系

| 模块 | 关系 |
|---|---|
| `syscall/fs/event.rs` | `sys_eventfd2(initval, flags)`：解析 `EFD_CLOEXEC/NONBLOCK/SEMAPHORE`，`add_file_like` |
| `file/mod.rs` | `EventFd implements FileLike`（`path = anon_inode:[eventfd]`） |
| `syscall/fs/fd_ops.rs` | `F_SETFL` 设置非阻塞 |
| epoll/poll/select | 通过 `Pollable` 等待 |

## 6. 已知局限 / TODO

- 无 `EFD_NONBLOCK` 之外的 flags（如 `EFD_SEMAPHORE` 已支持）。
- `read` 的 `dst.remaining_mut() < 8` 返回 `InvalidInput`（Linux 返回 `EINVAL`，一致）。
- 未实现 `eventfd` 的 `poll` 在跨线程关闭时的 HUP（eventfd 没有对端关闭概念，不需要）。
- 与 `signalfd`/`timerfd` 的语义差异未做统一抽象。

## 7. 变更影响面

改本文件 → `syscall/fs/event.rs.md`、`file/mod.rs.md`、`CONSTRAINTS.md` C13。
