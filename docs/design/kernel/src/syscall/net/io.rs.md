# `kernel/src/syscall/net/io.rs`

> 对应源码：`kernel/src/syscall/net/io.rs`
> 所属子系统：socket 数据收发 syscall
> 依赖：`axnet::{CMsgData, RecvFlags, RecvOptions, SendFlags, SendOptions, SocketAddrEx, SocketOps}`、`file::{FileLike, Socket, add_file_like}`、`mm::{IoVec, IoVectorBuf, UserConstPtr, UserPtr, VmBytes, VmBytesMut}`
> 被依赖：`syscall/net/mod.rs`

## 1. 文件职责

实现 `sendto/recvfrom/sendmsg/recvmsg`。

## 2. 关键流程

### 2.1 `send_impl(fd, src, flags, addr, addrlen, cmsg)`

```text
addr 空/addrlen 0 → None；否则 SocketAddrEx::read_from_user
socket = Socket::from_fd(fd)?
socket.send(&mut src, SendOptions { to: addr, flags: SendFlags::default(), cmsg })
```

**注意**：`flags`（`MSG_*`）目前没有转换为 `SendFlags`（默认空）——`MSG_DONTWAIT/MSG_NOSIGNAL` 等被忽略。

### 2.2 `sys_sendmsg(fd, msg, flags)`

```text
msg = msg.get_as_ref()?
若 msg_control 非空：
    ptr 从 msg_control 到 msg_control + msg_controllen
    循环：读 cmsghdr；若 ptr_end - ptr < cmsg_len → InvalidInput
          CMsg::parse(hdr)? → push 到 Vec<CMsgData>
          ptr += cmsg_len
send_impl(fd, IoVectorBuf::new(msg_iov, msg_iovlen)?.into_io(), flags, msg_name, msg_namelen, cmsg)
```

### 2.3 `recv_impl(fd, dst, flags, addr, addrlen, cmsg_builder)`

```text
socket = Socket::from_fd(fd)?
recv_flags = MSG_PEEK→PEEK | MSG_TRUNC→TRUNCATE（其他忽略）
remote_addr = addr 非空 ? Some(SocketAddrEx::Ip((0,0).into())) : None
recv = socket.recv(&mut dst, RecvOptions { from: remote_addr.as_mut(), flags: recv_flags, cmsg: Some(&mut cmsg) })?
若有 remote_addr → write_to_user(addr, addrlen.get_as_mut()?)
若有 cmsg_builder：
    for cmsg in cmsg：
        downcast::<CMsg>() 失败 → warn 跳过
        Rights{fds} → builder.push(SOL_SOCKET, SCM_RIGHTS, |data| {
            逐个 fd：add_file_like(f, false) → 写入 i32
        })
若 pushed == false → break
返回 recv
```

### 2.4 入口

- `sys_sendto` → `send_impl(..., VmBytes::new(buf,len), ..., Vec::new())`。
- `sys_recvfrom` → `recv_impl(..., VmBytesMut::new(buf,len), ..., None)`。
- `sys_recvmsg` 的 `cmsg_builder` 用 `msg_controllen` 作为容量，结束后 `msg_controllen` 为写入长度。

## 3. 不变量与约束

- **C6**：所有用户缓冲经 `VmBytes/VmBytesMut/IoVectorBuf`；`msghdr` 经 `get_as_ref/get_as_mut`。
- **C24**：`msg_controllen`/`cmsg_len` 必须校验（防止越界）。
- `MSG_PEEK/MSG_TRUNC` 是仅有的被转换的 flag；`SendFlags` 完全未映射（发送 flags 丢失）。
- `recv_impl` 的 `remote_addr` 用 `Ipv4Addr::UNSPECIFIED` 占位，实际地址由 `axnet` 填。
- `CMsgBuilder` 的 `msg_controllen` 必须可写（`get_as_mut` 返回 `&mut msghdr`）。
- `cmsg` 的 fd 通过 `add_file_like(f, false)` 分配（不带 cloexec；Linux 的 `MSG_CMSG_CLOEXEC` 未实现）。
- `recvmsg` 的 `msg_iovlen` 必须可转 `usize`（`IoVectorBuf::new` 内部校验）。

## 4. 与其他模块的关系

| 模块 | 关系 |
|---|---|
| `syscall/net/addr.rs` | 地址转换 |
| `syscall/net/cmsg.rs` | 控制消息 |
| `file/net.rs` | `Socket` |
| `mm/io.rs` | `IoVectorBuf` |
| `mm/access.rs` | `VmBytes/VmBytesMut` |
| `file/mod.rs` | `add_file_like` |
| `axnet` | `SocketOps::send/recv` |

## 5. 已知局限 / TODO

- 发送 flags（`MSG_DONTWAIT/MSG_NOSIGNAL/MSG_MORE`）未映射。
- `MSG_CMSG_CLOEXEC` 未实现。
- `MSG_WAITALL`/`MSG_ERRQUEUE`/`MSG_OOB` 未实现。
- `sendmsg` 的 `msg_flags` 输出未写回。
- `recvfrom` 的 `addr` 与 `addrlen` 空指针处理（`addr` 空但 `addrlen` 非空 → 不写）。
- cmsg 对齐未处理。

## 6. 变更影响面

改本文件 → `syscall/net/{addr,cmsg}.rs.md`、`file/net.rs.md`、`mm/io.rs.md`、`CONSTRAINTS.md` C6/C24。
