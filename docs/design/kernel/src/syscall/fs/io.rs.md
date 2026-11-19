# `kernel/src/syscall/fs/io.rs`

> 对应源码：`kernel/src/syscall/fs/io.rs`
> 所属子系统：文件 I/O syscall
> 依赖：`axfs::{FS_CONTEXT, FileFlags, OpenOptions}`、`axio::Seek`、`mm::{IoVec, IoVectorBuf, UserConstPtr, VmBytes, VmBytesMut}`、`file::{File, FileLike, Pipe, get_file_like}`
> 被依赖：`syscall/fs/mod.rs`

## 1. 文件职责

实现 `read/readv/write/writev/lseek/truncate/ftruncate/fallocate/fsync/fdatasync/fadvise64/pread64/pwrite64/preadv/pwritev/preadv2/pwritev2/sendfile/copy_file_range/splice`，以及 `sys_dummy_fd`。

## 2. 关键实现

### 2.1 基本读写

| syscall | 实现 |
|---|---|
| `read` | `get_file_like(fd)?.read(&mut VmBytesMut::new(buf, len))` |
| `write` | `get_file_like(fd)?.write(&mut VmBytes::new(buf, len))` |
| `readv/writev` | `IoVectorBuf::new(iov, iovcnt)?.into_io()` |
| `pread64/pwrite64` | `File::from_fd(fd)?.inner().read_at/write_at(VmBytes*, offset)` |
| `preadv2/pwritev2` | `read_at(IoVectorBuf...)` + `offset`（flags 忽略） |

**注意**：`sys_pwritev2` 的实现调用了 `read_at`（复制粘贴错误，应为 `write_at`）——已知 bug。

### 2.2 其他

- `lseek`：`whence 0/1/2` → `SeekFrom::{Start,Current,End}`；`File::from_fd` 后 `seek`。
- `truncate`：`path.get_as_str()`（`UserConstPtr<c_char>`）→ 以写方式 `OpenOptions::open` → `set_len`。
- `ftruncate`：`File::from_fd(fd)?.inner().access(WRITE)?.set_len`。
- `fallocate`：`mode != 0` → `InvalidInput`；`set_len(max(current, offset+len))`。
- `fsync/fdatasync`：`inner().sync(false/true)`。
- `fadvise64`：pipe 上 → `BrokenPipe`；`advice > 5` → `InvalidInput`；否则 0。
- `sendfile(out,in,offset,len)`：用 `SendFile::{Direct,Offset}` 包装读写；`do_send` 循环 4K 缓冲；`offset` 非空时用 `read_at` 并回写偏移。
- `copy_file_range`：两端都可为 `Offset`/`Direct`；复用 `do_send`（TODO：检查同文件重叠）。
- `splice`：要求至少一端是 pipe（`has_pipe`）；拒绝 `DummyFd`；`O_APPEND` 目标拒绝；`offset` 负值拒绝。
- `sys_dummy_fd`：`qemu-` 前缀任务返回 `Unsupported`；否则创建 `DummyFd`（空 `FileLike`，`poll` 空事件）。

## 3. 不变量与约束

- **C6**：所有用户缓冲经 `VmBytes/VmBytesMut/IoVectorBuf`；禁止裸指针。
- **C13**：`read/write` 的阻塞语义由 `FileLike` 实现（`block_on(poll_io)`），本文件不等待。
- `sys_pwritev2` 必须用 `write_at`（当前是 bug，修复时注意 `IoVectorBuf` 的 `Write` 实现已支持）。
- `sendfile` 的 `offset` 类型是 `*mut u64`：`vm_read`/`vm_write` 成对；`offset > u32::MAX` 拒绝（历史限制）。
- `do_send` 的"已写出一部分后 `WouldBlock` 就停止"是正确语义（返回短写）。
- `splice` 的 `has_pipe` 检查必须至少一端 pipe（Linux 要求），否则 `InvalidInput`。
- `fallocate` 的 `mode` 只支持 0（`FALLOC_FL_KEEP_SIZE` 等未实现）。
- `sys_truncate` 的路径来自 `UserConstPtr::get_as_str()`（不是 `vm_load_string`），两者都要校验。

## 4. 与其他模块的关系

| 模块 | 关系 |
|---|---|
| `file/mod.rs` | `FileLike`/`IoDst/IoSrc` |
| `file/fs.rs` | `File::inner()`/`access` |
| `file/pipe.rs` | `splice` 的方向检查 |
| `mm/io.rs` | `IoVectorBuf` |
| `mm/access.rs` | `VmBytes/VmBytesMut` |
| `syscall/fs/fd_ops.rs` | fd 有效性 |
| `task/user.rs` | 阻塞时的调度 |

## 5. 已知局限 / TODO

- `sys_pwritev2` 用了 `read_at`（**bug**）。
- `sys_dummy_fd` 创建的空 fd 对 `read/write` 返回 `InvalidInput`（可接受）。
- `copy_file_range` 未检查同文件/重叠/类型。
- `splice` 未实现真正的零拷贝（用 `do_send` 复制）。
- `fadvise64` 只校验 advice 范围，不生效。
- `fallocate` 不支持 `KEEP_SIZE/PUNCH_HOLE`。
- `sendfile` 的 `offset` 上限 `u32::MAX`。

## 6. 变更影响面

改本文件 → `file/{mod,fs,pipe}.rs.md`、`mm/io.rs.md`、`syscall/fs/fd_ops.rs.md`、`CONSTRAINTS.md` C6/C13。
