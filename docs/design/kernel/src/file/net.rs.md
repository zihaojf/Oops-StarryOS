# `kernel/src/file/net.rs`

> 对应源码：`kernel/src/file/net.rs`
> 所属子系统：网络 socket（fd 适配）
> 依赖：`axnet::{Socket, SocketOps, RecvOptions, SendOptions, options::{Configurable, GetSocketOption, SetSocketOption}}`、`axpoll`
> 被依赖：`file/mod.rs`（重导出 `Socket`）、`syscall/net/*`、`syscall/fs/io.rs`（sendfile/splice 的 FileLike 路径）

## 1. 文件职责

把 `axnet::Socket` 包装成 `FileLike`：

```rust
pub struct Socket(pub SocketInner);   // SocketInner = axnet::Socket
impl Deref for Socket { type Target = SocketInner; }
```

- `read` → `recv(dst, RecvOptions::default())`
- `write` → `send(src, SendOptions::default())`
- `stat` → 伪造 `S_IFSOCK | 0o777`（`TODO(mivik): implement stat for sockets`）
- `nonblocking`/`set_nonblocking` → 通过 `Get/SetSocketOption::NonBlocking`
- `path` → `socket:[ptr]`
- `from_fd` → downcast，失败 `NotASocket`
- `Pollable` → 直接转发给 `axnet::Socket::poll/register`

## 2. 设计要点

- **零成本包装**：`Deref` 让 `syscall/net/*` 可直接调用 `socket.bind/connect/accept/...`，不必再包一层方法。
- **非阻塞状态存在 axnet 内部**（不在本结构），因此 `set_nonblocking` 是委托而非本地 `AtomicBool`；`nonblocking()` 里 `unwrap()` 假设 `GetSocketOption` 不会失败（底层实现保证）。
- `read/write` 用默认 `RecvOptions/SendOptions`：无 `MSG_PEEK/TRUNC`、无目标地址——这些由 `syscall/net/io.rs` 的 `send_impl/recv_impl` 直接调用 `socket.send/recv` 并传 options，从而绕过 `FileLike::read/write`。
- 因此 **`read/write` 只服务 `read(2)/write(2)` 与 `sendfile/splice` 等通用路径**；带 flags 的 `sendto/recvfrom/sendmsg/recvmsg` 不走这里。

## 3. 不变量与约束

- **C12**：`Socket` 必须实现 `path()`（`/proc/<pid>/fd` 显示 `socket:[inode]`）与 `Pollable`。
- **C13**：非阻塞语义委托 `axnet`；不要在本结构再存一份 `nonblocking` 标志（会与 `F_SETFL`/`fcntl` 不同步）。
- `stat` 返回伪造 mode，`dev/ino` 为默认值——`fstat` 在 socket 上不可靠，但足够让程序不失败。
- `Socket::from_fd` 的 downcast 失败映射为 `NotASocket`，不要改成 `InvalidInput`。

## 4. 与其他模块的关系

| 模块 | 关系 |
|---|---|
| `syscall/net/socket.rs` | 创建 `Socket`、bind/connect/listen/accept/shutdown/socketpair |
| `syscall/net/io.rs` | `sendto/recvfrom/sendmsg/recvmsg` 直接调用 `axnet` API |
| `syscall/net/{addr,opt,name}.rs` | 地址/选项/名称转换 |
| `syscall/fs/io.rs` | `sys_sendfile/splice` 通过 `FileLike` 通用路径 |
| `pseudofs/dev/log.rs` | 用 `axnet::unix::UnixSocket` 直接建 `/dev/log` 服务器（不经本结构） |

## 5. 已知局限 / TODO

- `stat` 未实现真实 inode/dev（注释 TODO）。
- `read/write` 不支持 `MSG_*` flags（需用 `sendto/recvfrom` 系）。
- 无 `getsockopt(SO_ACCEPTCONN)` 等部分选项（由 `axnet` 决定）。
- `Socket` 的 `path` 用 `self as *const _`（`Socket` 自身地址），关闭后重开地址可能复用（仅用于显示）。

## 6. 变更影响面

改本文件 → `syscall/net/*.md`、`file/mod.rs.md`、`ARCHITECTURE.md#13`。
