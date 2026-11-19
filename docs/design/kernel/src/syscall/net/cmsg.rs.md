# `kernel/src/syscall/net/cmsg.rs`

> 对应源码：`kernel/src/syscall/net/cmsg.rs`
> 所属子系统：控制消息（SCM_RIGHTS 等）
> 依赖：`linux_raw_sys::net::{cmsghdr, SCM_RIGHTS, SOL_SOCKET}`、`file::{FileLike, get_file_like}`、`mm::{UserConstPtr, UserPtr}`
> 被依赖：`syscall/net/io.rs`

## 1. 文件职责

解析与构造 `sendmsg/recvmsg` 的辅助数据（control messages）：

```rust
pub enum CMsg { Rights { fds: Vec<Arc<dyn FileLike>> } }
pub struct CMsgBuilder<'a> { hdr: UserPtr<cmsghdr>, len: &'a mut usize, capacity: usize }
```

## 2. 关键流程

### 2.1 `CMsg::parse(hdr)`

```text
hdr.cmsg_len < size_of::<cmsghdr>() → InvalidInput
data = 用户内存 [hdr+sizeof(cmsghdr), cmsg_len - sizeof(cmsghdr))
match (level, type):
    (SOL_SOCKET, SCM_RIGHTS):
        data.len() % 4 != 0 → InvalidInput
        逐个 i32 fd：负 → BadFileDescriptor；get_file_like(fd)? → push
        Rights { fds }
    其他 → InvalidInput
```

### 2.2 `CMsgBuilder::push(level, ty, body)`

```text
body_capacity = (capacity - *len).checked_sub(sizeof(cmsghdr))? → None 返回 Ok(false)
hdr = self.hdr.get_as_mut()?            // 写 cmsg_level/cmsg_type
data = [hdr+sizeof(cmsghdr), body_capacity)
body_len = body(data)?
cmsg_len = sizeof(cmsghdr) + body_len
hdr.cmsg_len = cmsg_len
self.hdr = self.hdr + cmsg_len          // 前进
*len += cmsg_len
Ok(true)
```

## 3. 不变量与约束

- **C6**：`cmsghdr` 与数据都经 `UserPtr` 读写。
- **C24**：`cmsg_len` 必须 >= header 且不越界（`io.rs` 在解析时检查 `ptr_end - ptr < hdr.cmsg_len`）。
- `CMsgBuilder::new` 会把 `*len` 置 0（表示已写入 0 字节），`capacity` 保存原始容量；调用方在 `recvmsg` 结束后 `*msg_controllen` 即为写入长度。
- `SCM_RIGHTS` 的数据长度必须是 4 的倍数，fd 必须有效（`get_file_like`）。
- `push` 返回 `Ok(false)` 表示"空间不足"，调用方应停止（`recv_impl` 里 `if !pushed { break }`）。
- `CMsg` 是 `axnet::CMsgData`（`Box<dyn Any>`）的具体实现；`recv_impl` 用 `downcast::<CMsg>()` 取回。
- 不处理 `SCM_CREDENTIALS`/`SCM_SECURITY`（返回 `InvalidInput`）。

## 4. 与其他模块的关系

| 模块 | 关系 |
|---|---|
| `syscall/net/io.rs` | `sendmsg` 解析 control、`recvmsg` 构造 control |
| `file/mod.rs` | `get_file_like`/`add_file_like` |
| `axnet::CMsgData` | `Box<dyn Any + Send + Sync>` |
| `syscall/fs/fd_ops.rs` | 收到的 fd 由 `add_file_like` 分配 |

## 5. 已知局限 / TODO

- 只支持 `SCM_RIGHTS`。
- 没有 `MSG_CMSG_CLOEXEC` 的处理（recvmsg 的该 flag 被忽略，收到的 fd 不带 cloexec）。
- `CMsgBuilder` 不写 `cmsg_level/type` 之外的对齐填充（Linux 的 cmsg 有对齐要求，当前按 `sizeof(cmsghdr)+body` 紧凑排列）。
- 无发送 `SCM_RIGHTS` 的辅助（发送侧由用户构造 control，本文件只解析）。

## 6. 变更影响面

改本文件 → `syscall/net/io.rs.md`、`file/mod.rs.md`、`CONSTRAINTS.md` C6/C24。
