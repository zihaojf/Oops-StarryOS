# `kernel/src/mm/io.rs`

> 对应源码：`kernel/src/mm/io.rs`
> 所属子系统：分散聚集 I/O（iovec）
> 依赖：`starry_vm::{VmPtr, vm_read_slice, vm_write_slice}`、`axio`
> 被依赖：`mm/mod.rs`（重导出）、`syscall/fs/io.rs`（readv/writev/preadv/pwritev）、`syscall/net/io.rs`（sendmsg/recvmsg）

## 1. 文件职责

实现 `struct iovec` 数组的封装：

```rust
#[repr(C)] pub struct IoVec { pub iov_base: *mut u8, pub iov_len: isize }

pub struct IoVectorBuf { iovs: *const IoVec, iovcnt: usize, len: usize }
pub struct IoVectorBufIo { inner: IoVectorBuf, start: usize, offset: usize }
```

- `IoVectorBuf::new`：校验 `iovcnt <= 1024`、逐项读取 `iov_len`（`vm_read`）、拒绝负长度、累计总长。
- `read_with/fill_with`：对每个 iovec 调用闭包（用于简单的一次性聚合）。
- `into_io()`：转成实现 `Read/Write/IoBuf/IoBufMut` 的游标，供 `FileLike::read/write` 使用。

## 2. 关键流程

### 2.1 `IoVectorBufIo::read/write`

```text
loop {
    skip_empty()                      // 跳过 offset 已到 iov 末尾的项
    start >= iovcnt → break
    取当前 iov；len = min(iov_len - offset, buf.len() - count)
    len == 0 → break
    vm_read_slice / vm_write_slice(iov_base + offset, &mut buf[count..count+len])
    offset += len; inner.len -= len; count += len
}
```

每一步都通过 `vm_read_slice/vm_write_slice`，因此用户缓冲区在缺页时能被正确填充（依赖 `access_user_memory`）。

## 3. 不变量与约束

- **C6**：不能直接 `from_raw_parts(iov_base, len)`；所有访问经 `vm_*` 辅助函数。
- `iovcnt > 1024` 直接 `InvalidInput`（防 DoS）。
- `iov_len` 为负 → `InvalidInput`；`len` 累加可能溢出 usize（当前未检查，理论上 1024 * isize::MAX 会溢出——实际内核地址空间有限，但仍应在新增路径加检查）。
- `IoVectorBuf::new` 读一次 iov 数组、`read_with/fill_with` 每次迭代再读一次（用户可能并发改 iov，单核假设下安全）。
- `IoVectorBufIo` 的 `IoBuf::remaining`/`IoBufMut::remaining_mut` 都返回 `inner.len`（剩余总量），随读写递减。
- 空 iovec（`iov_len == 0`）必须跳过，不能返回 0（否则调用方误判 EOF）。

## 4. 与其他模块的关系

| 模块 | 关系 |
|---|---|
| `syscall/fs/io.rs` | `sys_readv/writev/preadv2/pwritev2` 构造 `IoVectorBuf` |
| `syscall/net/io.rs` | `msghdr.msg_iov` → `IoVectorBuf` |
| `file/mod.rs` | `IoDst/IoSrc` 兼容本类型 |
| `mm/access.rs` | `vm_read_slice/vm_write_slice` 的 `VmIo` 后端 |
| `file/pipe.rs`、`pseudofs/dev/tty/*` | 通过 `Read/Write` 消费 |

## 5. 已知局限 / TODO

- 无 `UIO_MAXIOV` 常量引用（硬编码 1024）。
- 总长度累加未做溢出检查。
- `read_with/fill_with` 是另一套接口，当前只有测试/少用路径使用（`sendfile` 没用它）。
- `iov_base` 为空但 `iov_len > 0` 时，`vm_read_slice` 会返回 `AccessDenied`（正确）。

## 6. 变更影响面

改本文件 → `syscall/fs/io.rs.md`、`syscall/net/io.rs.md`、`mm/access.rs.md`、`CONSTRAINTS.md` C6。
