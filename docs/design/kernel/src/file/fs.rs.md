# `kernel/src/file/fs.rs`

> 对应源码：`kernel/src/file/fs.rs`
> 所属子系统：文件/VFS 包装
> 依赖：`axfs::{FS_CONTEXT, FsContext, File}`、`axfs_ng_vfs::{Location, Metadata, NodeFlags}`、`axpoll`、`axtask::future::{block_on, poll_io}`
> 被依赖：`file/mod.rs`（重导出）、`syscall/fs/*`、`pseudofs/*`、`syscall/task/execve.rs`

## 1. 文件职责

把 ArceOS 的 `axfs::File` / `axfs_ng_vfs::Location` 适配成 StarryOS 的 `FileLike`：

1. `with_fs(dirfd, f)`：在指定目录（或当前 cwd）的 `FsContext` 下执行闭包；
2. `resolve_at(dirfd, path, flags)`：统一路径解析（`AT_FDCWD`/`AT_EMPTY_PATH`/`AT_SYMLINK_NOFOLLOW`）；
3. `metadata_to_kstat`：`Metadata → Kstat`；
4. `File`：普通文件/设备节点的 `FileLike` 实现（含非阻塞等待）；
5. `Directory`：目录的 `FileLike` 实现（readdir 偏移）。

## 2. 关键流程

### 2.1 `with_fs`

```rust
let mut fs = FS_CONTEXT.lock();
if dirfd == AT_FDCWD { f(&mut fs) }
else {
    let dir = Directory::from_fd(dirfd)?.inner.clone();
    f(&mut fs.with_current_dir(dir)?)       // 临时切换 cwd 执行，退出恢复
}
```

`FsContext::with_current_dir` 是 ArceOS 提供的"临时 cwd"机制，避免全局改 cwd。

### 2.2 `resolve_at`

| path | flags | 行为 |
|---|---|---|
| `None`/`""` | 无 `AT_EMPTY_PATH` | `NotFound` |
| `None`/`""` | 有 `AT_EMPTY_PATH` | 用 `dirfd` 本身：`File`→`backend.location()`；`Directory`→`inner`；其他→`Other(file_like)` |
| 非空 | 有 `AT_SYMLINK_NOFOLLOW` | `fs.resolve_no_follow` |
| 非空 | 默认 | `fs.resolve`（跟随符号链接） |

返回值 `ResolveAtResult::{File(Location), Other(Arc<dyn FileLike>)}`，`into_file()` 把 `Other` 转成 `None`（调用方据此报 `BadFileDescriptor`）。

### 2.3 `File` 的非阻塞读写

```rust
fn read(&self, dst) {
    if self.is_blocking() { self.inner.read(dst) }        // NodeFlags::BLOCKING（普通文件）
    else { block_on(poll_io(self, IoEvents::IN, self.nonblocking(), || self.inner.read(&mut *dst))) }
}
```

- 阻塞节点（普通文件、块设备）直接读，避免 poll 开销。
- 非阻塞节点（tty、pipe、socket 等经 VFS 的）用 `poll_io` 等待；`nonblocking()` 为 true 时返回 `WouldBlock`。
- `ioctl` 转发给 `backend().location().ioctl`。

### 2.4 `Directory`

- `read`/`write` 返回 `BadFileDescriptor`（目录不能读写）。
- `offset: Mutex<u64>` 保存 `getdents64` 的目录流位置；由 `sys_getdents64` 读写。
- `poll` 永远 `IN|OUT`；`register` 空实现。

## 3. 不变量与约束

- **C13**：非阻塞语义必须由本文件与非阻塞节点一致实现；`is_blocking()` 判断依据是 `NodeFlags::BLOCKING`，不要改成按 fd 类型判断。
- `with_fs` 内部会再次 `FS_CONTEXT.lock()`（通过 `Directory::from_fd` → `get_file_like` 不取 FS 锁，安全）；**禁止**在已持有 `FS_CONTEXT` 锁时再调用 `with_fs`（`axsync::Mutex` 非重入，会死锁）。
- `resolve_at` 是 AT_* 语义的唯一入口；新增 syscall 必须复用它，不要自己 `fs.resolve`。
- `File::from_fd` 的 downcast 失败映射：目录 → `IsADirectory`，其他 → `BrokenPipe`（模仿 Linux 对非普通文件的错误）。
- `Directory::offset` 必须在 `getdents64` 的整个循环期间持锁，防止并发 readdir 串位。

## 4. 与其他模块的关系

| 模块 | 关系 |
|---|---|
| `file/mod.rs` | 重导出；`FileLike`/`IoSrc/IoDst` 定义 |
| `syscall/fs/fd_ops.rs` | `openat` 用 `with_fs` + `OpenOptions::open` |
| `syscall/fs/ctl.rs` | `chdir/fchdir/getdents64/stat` 用 `with_fs`/`resolve_at` |
| `syscall/fs/stat.rs` | `fstatat/statx/faccessat/statfs` 用 `resolve_at` 与 `stat()` |
| `syscall/task/execve.rs` | `FS_CONTEXT.resolve(path)` |
| `pseudofs/*` | `SimpleFs` 的 `NodeOps` 最终被 `Location` 包装 |
| `mm/aspace/backend/file.rs` | `Location` 的页缓存映射 |

## 5. 已知局限 / TODO

- `is_blocking()` 只区分了 `NodeFlags::BLOCKING`；`poll_io` 对普通文件的 `IN` 事件恒真，因此路径正确但多一次 poll。
- `Directory::poll` 恒 `IN|OUT`，epoll 对目录会立即就绪（Linux 也类似，但语义差异未验证）。
- `path_for` 失败返回 `"<error>"`，与 Linux `/proc/<pid>/fd` 的 `(deleted)` 等不同。
- `resolve_at` 未处理 `AT_NO_AUTOMOUNT`/`AT_HANDLE_FID` 等新 flag。

## 6. 变更影响面

改本文件 → `file/mod.rs.md`、`syscall/fs/{ctl,io,stat,fd_ops}.rs.md`、`pseudofs/*.md`、`ARCHITECTURE.md#7`。
