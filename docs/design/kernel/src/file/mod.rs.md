# `kernel/src/file/mod.rs`

> 对应源码：`kernel/src/file/mod.rs`
> 所属子系统：文件描述符与文件抽象
> 依赖：`axfs::{FS_CONTEXT, OpenOptions}`、`axio`、`axpoll::Pollable`、`downcast-rs`、`flatten_objects`、`scope_local`
> 被依赖：`entry.rs`、`syscall/**`、`pseudofs/**`、`mm/aspace/backend/file.rs`

## 1. 文件职责

1. 声明子模块并重导出 `Directory/File/resolve_at/with_fs/Socket/PidFd/Pipe`；
2. 定义 `Kstat`（内核 stat 中间表示）及其到 `stat`/`statx` 的转换；
3. 定义 `FileLike` trait（所有 fd 的统一接口）与 `FileDescriptor`；
4. 定义 **scope-local 的 `FD_TABLE`** 与 fd 表操作（`get_file_like/add_file_like/close_file_like/add_stdio`）。

## 2. 关键类型

### 2.1 `Kstat`

`dev/ino/nlink/mode/uid/gid/size/blksize/blocks/rdev/atime/mtime/ctime`，`Default` 是"伪文件"值（ino=1、uid/gid=1、blksize=4096）。转换为 `stat` 与 `statx`（`stx_attributes` 用 mode 填充，`stx_dev_major/minor` 由 `dev` 拆分）。

### 2.2 `FileLike`

```rust
pub trait FileLike: Pollable + DowncastSync {
    fn read(&self, dst: &mut IoDst) -> AxResult<usize> { Err(InvalidInput) }
    fn write(&self, src: &mut IoSrc) -> AxResult<usize> { Err(InvalidInput) }
    fn stat(&self) -> AxResult<Kstat> { Ok(Kstat::default()) }
    fn path(&self) -> Cow<'_, str>;
    fn ioctl(&self, cmd: u32, arg: usize) -> AxResult<usize> { Err(NotATty) }
    fn nonblocking(&self) -> bool { false }
    fn set_nonblocking(&self, _: bool) -> AxResult { Ok(()) }
    fn from_fd(fd) -> AxResult<Arc<Self>>    // 向下转型
    fn add_to_fd_table(self, cloexec) -> AxResult<c_int>
}
```

- `IoDst = dyn Write + IoBufMut`，`IoSrc = dyn Read + IoBuf`；`VmBytesMut/VmBytes/IoVectorBuf` 实现它们，从而直接读写用户内存。
- `impl_downcast!(sync FileLike)` 生成 `downcast_arc/downcast_ref`。

### 2.3 `FileDescriptor` 与 `FD_TABLE`

```rust
pub struct FileDescriptor { pub inner: Arc<dyn FileLike>, pub cloexec: bool }
scope_local! { pub static FD_TABLE: Arc<RwLock<FlattenObjects<FileDescriptor, AX_FILE_LIMIT>>> = Arc::default(); }
```

- `FlattenObjects` 提供连续 id（即 fd）的分配与 `add_at`。
- `add_file_like` 先检查 `rlim[RLIMIT_NOFILE].current`，再 `table.add`。
- `close_file_like` 从表中移除；`Drop` 由 `Arc` 决定是否真正关闭。
- `add_stdio`：断言表为空，打开 `/dev/console` 三次（读、写、写），fd 0/1/2，`cloexec=false`。

## 3. 关键流程

```text
sys_openat → FileLike::add_to_fd_table → add_file_like
sys_read   → get_file_like(fd)?.read(&mut VmBytesMut::new(buf,len))
sys_close  → close_file_like(fd)
fork/clone → FD_TABLE.scope_mut(&mut new_scope).clone_from(&FD_TABLE)   // CLONE_FILES 时共享
```

## 4. 不变量与约束

- **C11**：`FD_TABLE` 是 scope-local；跨进程访问必须通过 `FD_TABLE.scope(&scope)`（如 `/proc/<pid>/fd` 与 `pidfd_getfd`），不能把 `Arc<FlattenObjects>` 存全局。
- **C12**：新增 fd 类型必须实现 `path()`（`/proc/<pid>/fd` 依赖）与 `Pollable`；`read/write` 默认错误不要改成 panic。
- **C13**：阻塞语义由实现方承担（`block_on(poll_io(...))` + `nonblocking()`）。
- `AX_FILE_LIMIT = 1024` 与 `FlattenObjects<_, 1024>` 必须一致。
- `Kstat::Default` 的 `uid/gid=1` 是历史值（相当于"非 root"），不要用于真实权限判断。
- `add_stdio` 的顺序（0 读、1 写、2 写）不可变；`cloexec=false` 保证 execve 后 stdio 保留。
- `from_fd` 的默认实现要求 `Self: 'static`；`File::from_fd` 对目录错误码有专门映射（`IsADirectory`/`BrokenPipe`）。

## 5. 与其他模块的关系

| 模块 | 关系 |
|---|---|
| `file/fs.rs` | `File`/`Directory`/`with_fs`/`resolve_at`/`Kstat` 转换 |
| `file/pipe.rs`、`event.rs`、`epoll.rs`、`net.rs`、`pidfd.rs`、`signalfd.rs` | 各 fd 实现 |
| `syscall/fs/fd_ops.rs` | open/close/dup/fcntl |
| `syscall/fs/io.rs` | read/write 与 `VmBytes` |
| `task/mod.rs` | `ProcessData.scope` 承载 fd 表 |
| `pseudofs/proc.rs` | `/proc/<pid>/fd` 遍历 `FD_TABLE.scope` |
| `mm/aspace/backend/file.rs` | 文件映射的页缓存 |

## 6. 已知局限 / TODO

- `Kstat::Default` 的 uid/gid=1 不是真实权限模型。
- fd 表用 `RwLock` 保护，`sys_close_range(UNSHARE)` 通过 `mem::take` + clone 实现，非原子复制。
- 没有 `fdinfo`/`O_PATH` 的完整语义（`File::from_fd` 对 path 文件仍返回文件对象）。
- `add_stdio` 每次打开 `/dev/console` 得到三个独立 `File`（读端/写端不共享偏移），符合 tty 语义但普通文件不适用。

## 7. 变更影响面

改本文件 → `file/*.md` 全部、`syscall/fs/*.md`、`pseudofs/proc.rs.md`、`task/mod.rs.md`、`ARCHITECTURE.md#7`、`CONSTRAINTS.md` C11/C12/C13。
