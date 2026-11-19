# `kernel/src/syscall/io_mpx/epoll.rs`

> 对应源码：`kernel/src/syscall/io_mpx/epoll.rs`
> 所属子系统：epoll syscall
> 依赖：`file::epoll::{Epoll, EpollEvent, EpollFlags}`、`axtask::future::{block_on, poll_io, timeout}`、`task::with_blocked_signals`、`mm::{UserConstPtr, UserPtr}`
> 被依赖：`syscall/io_mpx/mod.rs`

## 1. 文件职责

实现 `epoll_create1`、`epoll_ctl`、`epoll_pwait`、`epoll_pwait2`。

## 2. 关键实现

### 2.1 `epoll_create1`

`EpollCreateFlags::from_bits` 严格校验（只允许 `EPOLL_CLOEXEC`），`Epoll::new().add_to_fd_table(cloexec)`。

### 2.2 `epoll_ctl(epfd, op, fd, event)`

```text
parse_event = || {
    event = event.get_as_ref()?;                  // 用户结构
    events = IoEvents::from_bits_truncate(event.events);
    flags = EpollFlags::from_bits(event.events & !events.bits())?;  // 剩余位当 flags
    Ok((EpollEvent{events, user_data: event.data}, flags))
}
ADD → epoll.add(fd, event, flags)
MOD → epoll.modify(fd, event, flags)
DEL → epoll.delete(fd)
其他 → InvalidInput
```

### 2.3 `do_epoll_wait`

```text
check_sigset_size(sigsetsize)?;
epoll = Epoll::from_fd(epfd)?;
maxevents <= 0 → InvalidInput
events = events.get_as_mut_slice(maxevents)?
with_blocked_signals(sigmask, || {
    block_on(future::timeout(timeout, poll_io(epoll, IN, false, || epoll.poll_events(events))))
    Ok(r) → r.map(isize)
    Err(timeout) → Ok(0)
})
```

- `epoll_pwait` 的 timeout 是 `i32` 毫秒：`-1` → `None`，`>=0` → `Duration::from_millis`，其他 → `InvalidInput`。
- `epoll_pwait2` 的 timeout 是 `timespec*`（可空）。

## 3. 不变量与约束

- **C24**：`sigsetsize` 用 `check_sigset_size`；`maxevents` 必须 > 0；`events` 数组长度用 `get_as_mut_slice(maxevents)` 校验。
- **C6**：`epoll_event` 结构经 `get_as_ref`/`get_as_mut_slice`。
- `EpollFlags::from_bits` 对未知位返回 `InvalidInput`（严格）；`IoEvents::from_bits_truncate` 丢弃未知事件位（宽松）。
- `with_blocked_signals` 在 `Err` 时不恢复旧掩码（见 `task/signal.rs.md`）——`do_epoll_wait` 返回错误前可能已设置掩码，是已知缺陷。
- 超时返回 0（不是 `WouldBlock`）；`poll_io` 的 `WouldBlock` 由 `future::timeout` 转成 `Err` 再映射为 0。
- `Epoll::from_fd` 失败（不是 epoll fd）→ downcast 错误 `InvalidInput`。

## 4. 与其他模块的关系

| 模块 | 关系 |
|---|---|
| `file/epoll.rs` | `Epoll` 实例（interest 表/就绪队列/触发模式） |
| `syscall/io_mpx/mod.rs` | 门面 |
| `task/signal.rs` | `with_blocked_signals` |
| `syscall/signal.rs` | `check_sigset_size` |
| `mm/access.rs` | `UserPtr`/`UserConstPtr` |
| `axtask::future` | `timeout`/`poll_io` |

## 5. 已知局限 / TODO

- `with_blocked_signals` 错误路径不恢复掩码。
- 不支持 `EPOLL_CTL_ADD` 的 `EPOLLEXCLUSIVE`。
- `epoll_pwait` 的 `sigmask` 是 `UserConstPtr<SignalSet>`，NULL 时 `nullable!` 返回 None（正确）。
- 未实现 `epoll_create`（只有 create1）。

## 6. 变更影响面

改本文件 → `file/epoll.rs.md`、`syscall/io_mpx/mod.rs.md`、`task/signal.rs.md`、`CONSTRAINTS.md` C24。
