# `kernel/src/syscall/net/mod.rs`

> 对应源码：`kernel/src/syscall/net/mod.rs`
> 所属子系统：网络 syscall（模块门面）
> 依赖：`addr`/`cmsg`/`io`/`name`/`opt`/`socket`

## 1. 文件职责

纯门面：

```rust
mod addr; mod cmsg; mod io; mod name; mod opt; mod socket;
pub use self::{cmsg::*, io::*, name::*, opt::*, socket::*};
```

注意：`addr` 只定义 `SocketAddrExt` trait（`pub use` 未包含 `addr::*`？——实际文件里 `pub use self::{cmsg::*, io::*, name::*, opt::*, socket::*};`，`addr` 的 `SocketAddrExt` 通过 trait 在 `super::addr::SocketAddrExt` 引用）。

| 文件 | 内容 |
|---|---|
| `socket.rs` | `socket/socketpair/bind/connect/listen/accept(4)/shutdown` |
| `io.rs` | `sendto/recvfrom/sendmsg/recvmsg` |
| `addr.rs` | `sockaddr ↔ SocketAddrEx` 转换 |
| `cmsg.rs` | `SCM_RIGHTS` 控制消息解析/构造 |
| `opt.rs` | `getsockopt/setsockopt` |
| `name.rs` | `getsockname/getpeername` |

## 2. 不变量与约束

- 所有实现必须经 `file::Socket`（`axnet` 包装）与 `mm::UserPtr`（C6）。
- `socklen_t` 长度参数必须校验（`addr`/`opt` 内）。
- 新增 socket 类型/选项时同步改 `addr.rs`/`opt.rs` 与本文档。

## 3. 变更影响面

改本文件 → `syscall/net/*.md`、`file/net.rs.md`、`syscall/mod.rs.md`。
