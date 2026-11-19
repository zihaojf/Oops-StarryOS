# `kernel/src/syscall/net/addr.rs`

> 对应源码：`kernel/src/syscall/net/addr.rs`
> 所属子系统：socket 地址 ABI 转换
> 依赖：`axnet::{SocketAddrEx, unix::UnixSocketAddr}`、`linux_raw_sys::net::*`、`mm::{UserConstPtr, UserPtr}`
> 被依赖：`syscall/net/{socket,io,name,opt}.rs`、`syscall/fs/pidfd.rs`（无）

## 1. 文件职责

定义 `SocketAddrExt` trait 并为 4 种地址实现 `sockaddr ↔ Rust` 转换：

| 类型 | 家族 | 实现 |
|---|---|---|
| `SocketAddr`（V4/V6） | `AF_INET/AF_INET6` | 校验 `addrlen == sizeof(sockaddr_in/in6)`、`sin_family`；字节序转换 |
| `UnixSocketAddr` | `AF_UNIX` | 空 → Unnamed；首字节 0 → Abstract；否则 Path（NUL 截断） |
| `VsockAddr` | `AF_VSOCK` | 自定义 `sockaddr_vm`（linux_raw_sys 缺） |
| `SocketAddrEx` | 按 family 分派 | 统一入口 |

## 2. 关键函数

```rust
fn read_family(addr, addrlen) -> AxResult<u16>   // 至少 sizeof(sa_family_t)
fn fill_addr(addr, addrlen, data) -> AxResult<()> // 写 min(*addrlen, data.len())，并 *addrlen = data.len()
```

- `SocketAddrV4::read_from_user`：`sin_port`/`s_addr` 用 `from_be`/`from_bits(u32::from_be(...))`。
- `SocketAddrV4::write_to_user`：`sin_port.to_be()`、`s_addr = u32::from_ne_bytes(octets)`。
- `UnixSocketAddr::write_to_user`：手动拼 `[family(2B)][path][NUL]` 或 `[family][0][name]`（abstract）。
- `VsockAddr`：`svm_cid/svm_port`。

## 3. 不变量与约束

- **C6**：地址缓冲经 `get_as_ref`/`get_as_mut_slice`，不能裸读。
- **C24**：`addrlen` 必须校验（`AF_INET` 要求精确 `sizeof(sockaddr_in)`；`AF_UNIX` 允许变长）。
- `fill_addr` 的语义：只写 `*addrlen` 指定的长度，并把 `*addrlen` 更新为实际长度（Linux 语义：返回真实长度）。
- `UnixSocketAddr` 的抽象地址（首字节 0）与路径地址（NUL 结尾）区分必须正确；空数据 = Unnamed。
- `SocketAddrEx::family()` 恒返回 `AF_INET`（**bug**：应分派），但当前没有调用方依赖。
- `VsockAddr` 的 `sockaddr_vm` 布局必须与 Linux 一致。
- 字节序：IP 地址用 `from_ne_bytes(octets)`（内部表示），端口用 `to_be/from_be`（网络序）。

## 4. 与其他模块的关系

| 模块 | 关系 |
|---|---|
| `syscall/net/socket.rs` | bind/connect/accept 用 `read_from_user`/`write_to_user` |
| `syscall/net/io.rs` | sendto/recvfrom 的地址 |
| `syscall/net/name.rs` | getsockname/getpeername |
| `file/net.rs` | `Socket` 包装 |
| `axnet` | `SocketAddrEx/UnixSocketAddr/VsockAddr` |

## 5. 已知局限 / TODO

- `SocketAddrEx::family()` 恒 `AF_INET`（bug）。
- `UnixSocketAddr` 的 abstract 名字长度/截断未做上限检查。
- 不支持 `AF_NETLINK`/`AF_PACKET`/`AF_ALG`。
- `read_family` 只检查最小长度，不检查 `addrlen` 是否超过家族特定结构（由各实现检查）。

## 6. 变更影响面

改本文件 → `syscall/net/{socket,io,name}.rs.md`、`file/net.rs.md`、`CONSTRAINTS.md` C6/C24。
