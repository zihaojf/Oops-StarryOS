# `kernel/src/syscall/net/opt.rs`

> 对应源码：`kernel/src/syscall/net/opt.rs`
> 所属子系统：socket 选项
> 依赖：`axnet::options::{Configurable, GetSocketOption, SetSocketOption}`、`linux_raw_sys::net::*`、`mm::{UserConstPtr, UserPtr}`、`time::TimeValueLike`
> 被依赖：`syscall/net/mod.rs`

## 1. 文件职责

实现 `getsockopt/setsockopt`：把 Linux 的 `(level, optname)` 分派到 `axnet` 的选项 API，并做类型转换。

## 2. 结构

```rust
const PROTO_TCP = IPPROTO_TCP;  const PROTO_IP = IPPROTO_IP;
mod conv {
    pub struct Int<T>;       // TryFrom<i32> + TryInto<i32>
    pub struct IntBool;      // i32 ↔ bool
    pub struct Duration;     // timeval ↔ Duration（TimeValueLike）
    pub struct Ucred;        // ucred ↔ UnixCredentials
}
macro_rules! call_dispatch { ... }   // 两层：匹配 (level, optname) → dispatch!(Which as Conv)
```

### 支持的选项（`call_dispatch` 列表）

| level | optname | 转换 |
|---|---|---|
| `SOL_SOCKET` | `SO_REUSEADDR` | `IntBool` |
| `SOL_SOCKET` | `SO_ERROR` | `Error` |
| `SOL_SOCKET` | `SO_DONTROUTE` | `IntBool` |
| `SOL_SOCKET` | `SO_SNDBUF`/`SO_RCVBUF` | `Int<usize>` |
| `SOL_SOCKET` | `SO_KEEPALIVE` | `IntBool` |
| `SOL_SOCKET` | `SO_RCVTIMEO`/`SO_SNDTIMEO` | `Duration` |
| `SOL_SOCKET` | `SO_PASSCRED` | `IntBool` |
| `SOL_SOCKET` | `SO_PEERCRED` | `Ucred` |
| `PROTO_TCP` | `TCP_NODELAY` | `IntBool` |
| `PROTO_TCP` | `TCP_MAXSEG` | `Int<usize>` |
| `PROTO_TCP` | `TCP_INFO` | 直接 `get::<TcpInfo>` |
| `PROTO_IP` | `IP_TTL` | `Int<u8>` |

其他 `(level, optname)` → `ENOPROTOOPT`。

## 3. 关键流程

- `getsockopt`：`optlen = optlen.get_as_mut()?`；`get::<T>(optval, len)` 要求 `*len >= size_of::<T>()`，写回 `*len = size_of::<T>()`；`dispatch!` 宏分两种（直接传 `&mut T` / 先读 `val` 再转换）。
- `setsockopt`：`get::<T>(optval, len)` 要求 `len == size_of::<T>()`（精确）；`sys_to_rust` 后 `set_option`。
- `Default::default()` 用于 `as $conv` 分支的中间值（`Option<T>` 类型推断）。

## 4. 不变量与约束

- **C6**：`optval` 经 `UserPtr`/`UserConstPtr`；`optlen` 必须可写。
- **C24**：`getsockopt` 的 `*len >= size`、`setsockopt` 的 `len == size` 必须校验（防止越界）。
- `ENOPROTOOPT` 用于未知选项（不是 `EINVAL`）；调用方（如 `ss`）依赖它判断"不支持"。
- `Duration` 转换用 `TimeValueLike`：`timeval` 的 `tv_usec` 越界会 `InvalidInput`。
- `Ucred` 的结构必须与 `linux_raw_sys::net::ucred` 一致。
- `TCP_INFO` 直接按 `axnet` 的 `TcpInfo` 布局写用户（ABI 依赖 axnet 版本）。
- `Int<u8>` 的 `TryFrom<i32>` 会拒绝 >255（`IP_TTL` 合法范围 1..255）。

## 5. 与其他模块的关系

| 模块 | 关系 |
|---|---|
| `file/net.rs` | `Socket`/`nonblocking()` 用 `NonBlocking` 选项 |
| `syscall/net/socket.rs` | 创建 socket |
| `axnet::options` | `Get/SetSocketOption`、`Configurable` |
| `time.rs` | `TimeValueLike` |

## 6. 已知局限 / TODO

- 选项覆盖不全（无 `SO_BROADCAST/SO_LINGER/SO_REUSEPORT/SO_BINDTODEVICE/IP_MTU/IPV6_*`）。
- `getsockopt` 的"长度不足"返回 `InvalidInput`（Linux 返回 `EINVAL`，一致）。
- `TCP_INFO` 的 ABI 依赖 axnet 的内部结构，不稳定。
- 无 `SO_DOMAIN/SO_TYPE/SO_PROTOCOL`。
- `call_dispatch!` 宏的两层展开较难维护。

## 7. 变更影响面

改本文件 → `file/net.rs.md`、`syscall/net/mod.rs.md`、`time.rs.md`、`CONSTRAINTS.md` C6/C24。
