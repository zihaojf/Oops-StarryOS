# `kernel/src/syscall/io_mpx/poll.rs`

> 对应源码：`kernel/src/syscall/io_mpx/poll.rs`
> 所属子系统：poll/ppoll syscall
> 依赖：`super::FdPollSet`、`axtask::future::{block_on, poll_io, timeout}`、`task::with_blocked_signals`、`mm::{UserConstPtr, UserPtr}`
> 被依赖：`syscall/io_mpx/mod.rs`

## 1. 文件职责

实现 `poll`（x86_64）与 `ppoll`。

## 2. `do_poll(poll_fds, timeout, sigmask)`

```text
for fd in poll_fds:
    fd == -1 → 跳过（Linux：忽略）
    查 get_file_like(fd.fd)：
      Err → fd.revents = POLLNVAL; res += 1
      Ok(f) → fds.push((f, IoEvents::from_bits(fd.events)? | ALWAYS_POLL)); revents.push(&mut fd.revents)
若 res > 0 → 立即返回 res（有无效 fd）
with_blocked_signals(sigmask, || block_on(timeout(timeout, poll_io(&FdPollSet, empty, false, || {
    for ((fd, events), revents) in ...:
        result = fd.poll()
        IN → result |= RDNORM; OUT → result |= WRNORM
        result &= *events
        **revents = result.bits()
        **revents != 0 → res += 1
    res > 0 ? Ok(res) : Err(WouldBlock)
}))))
```

- `poll`：timeout 是 `i32` 毫秒（`<0` → `None`），无 sigmask。
- `ppoll`：timeout 是 `timespec*`，sigmask 可选；`nfds` 必须可转 `usize`。

## 3. 关键设计点

- **`POLLNVAL`**：无效 fd 立即返回，不进入等待（Linux 语义）。
- **`ALWAYS_POLL`**：`IoEvents` 的内部标志，用于让 `poll_io` 不因"没有关注事件"而短路。
- **`RDNORM/WRNORM`**：Linux `poll` 的普通数据读写事件，映射自 `IN/OUT`。
- **`result &= *events`**：只报告调用方关注的事件。
- `revents` 直接指向用户数组元素（`get_as_mut_slice` 返回的 `&mut [pollfd]`，元素是 `&mut i16`）。

## 4. 不变量与约束

- **C24**：`nfds` 转 `usize` 失败（负）→ `InvalidInput`。
- **C6**：`fds` 经 `UserPtr::get_as_mut_slice`；`revents` 写入用户内存。
- **C13**：等待由 `poll_io` + 各 fd 的 `register` 完成；本文件不轮询。
- 无效 fd 的 `POLLNVAL` 必须在进入等待前返回（否则永远不唤醒）。
- `fd == -1` 条目必须跳过且 `revents = 0`（`get_as_mut_slice` 得到的原值可能非 0，但 Linux 要求 0；当前未清零——潜在差异）。
- `with_blocked_signals` 的错误路径不恢复掩码（已知问题）。
- `ppoll` 的 `sigsetsize` 必须 `check_sigset_size`。

## 5. 与其他模块的关系

| 模块 | 关系 |
|---|---|
| `syscall/io_mpx/mod.rs` | `FdPollSet` |
| `file/mod.rs` | `get_file_like` |
| `file/*.rs` | 各 `Pollable::poll/register` |
| `task/signal.rs` | `with_blocked_signals` |
| `syscall/signal.rs` | `check_sigset_size` |
| `mm/access.rs` | `UserPtr`/`UserConstPtr`/`nullable!` |

## 6. 已知局限 / TODO

- `fd == -1` 的 `revents` 未清零。
- `POLLPRI/POLLRDHUP` 等事件位未映射（`IoEvents::from_bits` 会返回 `InvalidInput`？——`from_bits` 对未知位报错，可能拒绝合法 flags）。
- 错误路径的 sigmask 恢复缺失。
- 无 `pselect`/`poll` 的 64 位 time_t 变体。

## 7. 变更影响面

改本文件 → `syscall/io_mpx/mod.rs.md`、`task/signal.rs.md`、`file/mod.rs.md`、`CONSTRAINTS.md` C24/C13。
