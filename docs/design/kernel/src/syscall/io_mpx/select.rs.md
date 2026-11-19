# `kernel/src/syscall/io_mpx/select.rs`

> 对应源码：`kernel/src/syscall/io_mpx/select.rs`
> 所属子系统：select/pselect syscall
> 依赖：`bitmaps::Bitmap`、`linux_raw_sys::{select_macros::{FD_ISSET, FD_SET, FD_ZERO}, general::{__FD_SETSIZE, ...}}`、`super::FdPollSet`、`task::with_blocked_signals`
> 被依赖：`syscall/io_mpx/mod.rs`

## 1. 文件职责

实现 `select`（x86_64）与 `pselect6`。

## 2. 关键类型与流程

```rust
struct FdSet(Bitmap<{__FD_SETSIZE as usize}>);         // fd 集合位图
fn FdSet::new(nfds, fds: Option<&__kernel_fd_set>)      // 用 libc 的 FD_ISSET 逐位读
struct SignalSetWithSize { set: UserConstPtr<SignalSet>, sigsetsize: usize }
```

`do_select(nfds, readfds, writefds, exceptfds, timeout, sigmask)`：

```text
nfds > FD_SETSIZE → InvalidInput
解析 sigmask（pselect 才有）：check_sigset_size + 取 set
读三个 fd_set（可能为 NULL）：get_as_mut → Option<&mut __kernel_fd_set>
构造 read/write/except 位图
取 FD_TABLE.read()，对 union 的每个 fd：
    取 FileLike；按位图设置 events（IN/OUT/ERR）；非空则 push (file, events)
drop(fd_table)
把三个用户 fd_set 清零（FD_ZERO）
with_blocked_signals(sigmask, || block_on(timeout(timeout, poll_io(&FdPollSet, empty, false, || {
    for ((fd, interested), index) in ...:
        events = fd.poll() & *interested
        IN → FD_SET(index, readfds); res += 1
        OUT → FD_SET(index, writefds); res += 1
        ERR → FD_SET(index, exceptfds); res += 1
    res > 0 ? Ok(res) : Err(WouldBlock)
}))))
```

- `select` 的 timeout 是 `timeval`；`pselect6` 是 `timespec` + sigmask 指针。
- 返回值是"就绪的 fd 数"（不是 fd 值）。

## 3. 关键设计点

- **`FD_SET/FD_ZERO` 直接操作用户内存**（`&mut __kernel_fd_set`），因此必须保证 `get_as_mut` 的引用有效（在 `poll_io` 闭包内使用 `unsafe`）。
- `FdSet` 位图只用于**输入解析**；输出直接写用户 fd_set。
- `nfds > __FD_SETSIZE` → `InvalidInput`（Linux 允许更大但会截断/返回 EINVAL，这里直接拒绝）。
- 无效 fd → `BadFileDescriptor`（`fd_table.get(fd).ok_or`），而不是像 poll 那样设置 `POLLNVAL`。
- `exceptfds` 映射到 `IoEvents::ERR`。

## 4. 不变量与约束

- **C6**：`fd_set` 通过 `UserPtr::get_as_mut` 访问；`FD_SET` 的 `unsafe` 依赖引用有效。
- **C24**：`nfds` 上限校验；`sigsetsize` 校验（仅 pselect）。
- **C13**：等待用 `poll_io` + `FdPollSet::register`。
- `FD_TABLE.read()` 必须在构造 `FdPollSet` 后立即释放（`drop(fd_table)`），否则 `fd.poll()` 内可能再次取 fd 表（如 epoll 的 `poll`）→ 死锁（`RwLock` 读锁可重入？`spin::RwLock` 读锁可重入但写锁会阻塞；仍应避免）。
- 空 fd_set（全 NULL）→ `nfds` 仍可能 > 0，此时 `fds` 为空，`poll_io` 会立即超时（Linux 行为：只是 sleep）。
- `select` 的 timeout 是"剩余时间"，返回后不更新（Linux 会更新 `timeval`；这里未写回——差异）。

## 5. 与其他模块的关系

| 模块 | 关系 |
|---|---|
| `syscall/io_mpx/mod.rs` | `FdPollSet` |
| `file/mod.rs` | `FD_TABLE`/`FileLike` |
| `task/signal.rs` | `with_blocked_signals` |
| `syscall/signal.rs` | `check_sigset_size` |
| `mm/access.rs` | `UserPtr`/`nullable!` |
| `linux_raw_sys::select_macros` | `FD_SET/FD_ZERO/FD_ISSET` |

## 6. 已知局限 / TODO

- timeout 的 `timeval` 不写回剩余时间。
- 无效 fd 返回 `BadFileDescriptor`（Linux 对 select 返回 `EBADF`，一致；但不设置任何位）。
- 不支持 `pselect6` 的 `sigmask` 为空但 `sigsetsize` 非 0 的组合（`check_sigset_size` 允许 0）。
- `FD_SETSIZE` 固定 1024。
- `exceptfds` 只有 `ERR`，无 `PRI`。

## 7. 变更影响面

改本文件 → `syscall/io_mpx/mod.rs.md`、`file/mod.rs.md`、`task/signal.rs.md`、`CONSTRAINTS.md` C24/C6。
