# `kernel/src/syscall/net/name.rs`

> 对应源码：`kernel/src/syscall/net/name.rs`
> 所属子系统：socket 名称查询
> 依赖：`axnet::SocketOps`、`file::Socket`、`mm::UserPtr`、`super::addr::SocketAddrExt`
> 被依赖：`syscall/net/mod.rs`

## 1. 文件职责

实现 `getsockname(fd, addr, addrlen)` 与 `getpeername(fd, addr, addrlen)`。

```rust
pub fn sys_getsockname(fd, addr: UserPtr<sockaddr>, addrlen: UserPtr<socklen_t>) -> AxResult<isize> {
    let socket = Socket::from_fd(fd)?;
    let local_addr = socket.local_addr()?;
    local_addr.write_to_user(addr, addrlen.get_as_mut()?)?;
    Ok(0)
}
// getpeername 同，改用 peer_addr()
```

## 2. 不变量与约束

- **C6**：`addr`/`addrlen` 都经 `UserPtr`；`addrlen` 必须可写（`get_as_mut`）。
- **C24**：`write_to_user` 内部按 `*addrlen` 截断并更新（见 `syscall/net/addr.rs`）。
- `Socket::from_fd` 要求 fd 是 socket，否则 `NotASocket`。
- `local_addr()`/`peer_addr()` 失败（未 bind/未 connect）→ `axnet` 错误码透传。
- 返回 0；真实地址长度通过 `*addrlen` 返回。

## 3. 与其他模块的关系

| 模块 | 关系 |
|---|---|
| `syscall/net/addr.rs` | `write_to_user` |
| `file/net.rs` | `Socket` |
| `axnet` | `local_addr/peer_addr` |
| `syscall/net/socket.rs` | bind/connect/accept |

## 4. 已知局限 / TODO

- 无 `SO_DOMAIN/SO_PROTOCOL` 查询（在 `opt.rs` 也未实现）。
- IPv6 地址的 scope id 透传依赖 `axnet`。

## 5. 变更影响面

改本文件 → `syscall/net/addr.rs.md`、`file/net.rs.md`。
