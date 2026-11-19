# `kernel/src/syscall/net/socket.rs`

> 对应源码：`kernel/src/syscall/net/socket.rs`
> 所属子系统：socket 生命周期
> 依赖：`axnet::{Shutdown, Socket as SocketInner, SocketAddrEx, SocketOps, tcp::TcpSocket, udp::UdpSocket, unix::*}`、`file::Socket`、`mm::{UserConstPtr, UserPtr}`
> 被依赖：`syscall/net/mod.rs`

## 1. 文件职责

实现 `socket/socketpair/bind/connect/listen/accept/accept4/shutdown`。

## 2. 关键流程

### 2.1 `sys_socket(domain, raw_ty, proto)`

```text
ty = raw_ty & 0xFF
match (domain, ty):
    (AF_INET, SOCK_STREAM) → proto 0/TCP → TcpSocket::new()；否则 EPROTONOSUPPORT
    (AF_INET, SOCK_DGRAM)  → proto 0/UDP → UdpSocket::new()
    (AF_UNIX, SOCK_STREAM) → UnixSocket::new(StreamTransport::new(pid))
    (AF_UNIX, SOCK_DGRAM)  → UnixSocket::new(DgramTransport::new(pid))
    (AF_VSOCK, SOCK_STREAM) → VsockSocket（feature vsock）
    (AF_INET/AF_UNIX/AF_VSOCK, _) → ESOCKTNOSUPPORT
    _ → EAFNOSUPPORT
raw_ty & O_NONBLOCK → set_nonblocking
cloexec = raw_ty & O_CLOEXEC
add_to_fd_table(cloexec)
```

### 2.2 `bind/connect/listen/accept(4)/shutdown`

| syscall | 行为 |
|---|---|
| `bind(fd, addr, addrlen)` | `SocketAddrEx::read_from_user` → `socket.bind(addr)` |
| `connect(fd, addr, addrlen)` | 同上；`WouldBlock` → 映射成 `InProgress`（`EINPROGRESS`） |
| `listen(fd, backlog)` | `backlog < 0 && != -1` → `InvalidInput`；`socket.listen()` |
| `accept(fd, addr, addrlen)` | 转 `accept4(...,0)` |
| `accept4(fd, addr, addrlen, flags)` | `socket.accept()`；`O_NONBLOCK` → `set_nonblocking`；**取 `socket.local_addr()` 作为对端地址写回**（`remote_addr` 命名但取 local——见局限） |
| `shutdown(fd, how)` | `SHUT_RD/WR/RDWR` → `Shutdown::{Read,Write,Both}` |
| `socketpair(domain, raw_ty, proto, fds)` | 仅 `AF_UNIX`；`SOCK_STREAM` → `StreamTransport::new_pair(pid)`；`SOCK_DGRAM/SEQPACKET` → `DgramTransport::new_pair(pid)`；写两个 fd |

## 3. 不变量与约束

- **C6**：地址参数经 `UserConstPtr`，`accept` 的输出经 `UserPtr`。
- **C12**：新 socket 必须 `add_to_fd_table`（受 `RLIMIT_NOFILE`）。
- `raw_ty & 0xFF` 取类型，`O_NONBLOCK/O_CLOEXEC` 在高位；不要直接把整个 `raw_ty` 当类型。
- `connect` 的 `WouldBlock → InProgress` 映射：非阻塞 connect 的正确 errno（`EINPROGRESS`）。
- `accept4` 的 `addr` 可空：空则不写对端地址。
- `socketpair` 的两个 fd 必须成对创建，失败时（第二个 add 失败）**未回滚**第一个（泄漏点）。
- `vsock` 用 `#[cfg(feature = "vsock")]` 隔离。
- `SOCK_SEQPACKET` 只在 `socketpair` 支持（`socket` 不支持）。

## 4. 与其他模块的关系

| 模块 | 关系 |
|---|---|
| `syscall/net/addr.rs` | 地址转换 |
| `file/net.rs` | `Socket` 包装 |
| `file/mod.rs` | `add_to_fd_table` |
| `axnet` | TCP/UDP/Unix/Vsock |
| `task/mod.rs` | `current().as_thread().proc_data.proc.pid()`（Unix socket 身份） |
| `syscall/net/opt.rs` | socket 选项 |

## 5. 已知局限 / TODO

- `accept4` 写回的是 `local_addr()`（应为 `peer_addr()`）——**bug**，影响客户端地址显示。
- `socketpair` 第二个 fd 失败时不关闭第一个。
- `socket` 不支持 `SOCK_SEQPACKET`（AF_UNIX）。
- `listen` 忽略 backlog（`axnet` 内部可能固定）。
- 无 `SO_REUSEPORT` 等 bind 前选项。
- `bind` 的 Unix socket 路径需要 `axnet` 支持（依赖 `UnixSocketAddr`）。

## 6. 变更影响面

改本文件 → `syscall/net/{addr,opt}.rs.md`、`file/net.rs.md`、`file/mod.rs.md`、`CONSTRAINTS.md` C6/C12。
