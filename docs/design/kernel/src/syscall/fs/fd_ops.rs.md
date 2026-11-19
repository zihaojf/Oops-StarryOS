# `kernel/src/syscall/fs/fd_ops.rs`

> 对应源码：`kernel/src/syscall/fs/fd_ops.rs`
> 所属子系统：文件描述符操作
> 依赖：`axfs::{FS_CONTEXT, OpenOptions, OpenResult}`、`axfs_ng_vfs::{DirEntry, FileNode, Location, NodeType, Reference}`、`file::{FD_TABLE, File, Directory, FileLike, Pipe, add_file_like, close_file_like, get_file_like, with_fs}`、`pseudofs::{Device, dev::tty}`
> 被依赖：`syscall/fs/mod.rs`

## 1. 文件职责

实现 `open/openat`、`close/close_range`、`dup/dup2/dup3`、`fcntl`、`flock`，以及打开 `/dev/ptmx`、`/dev/tty` 的特殊替换逻辑。

## 2. 关键流程

### 2.1 `sys_openat(dirfd, path, flags, mode)`

```text
mode &= !umask
options = flags_to_options(flags, mode, (euid, egid))
with_fs(dirfd, |fs| options.open(fs, path)) → OpenResult::{File, Dir}
add_to_fd(result, flags)
```

`flags_to_options`：`O_RDONLY/O_WRONLY/O_RDWR`、`O_APPEND/TRUNC/CREAT/PATH/EXCL/DIRECTORY/NOFOLLOW/DIRECT`。

### 2.2 `add_to_fd`（特殊设备替换）

```text
OpenResult::File(file)：
  若 entry 是 Device：
    as_any().downcast_ref::<Ptmx>() → ptmx.create_pty() → 构造 /dev/pts/N 的 DirEntry/Location → File
    as_any().is::<CurrentTty>() → 取 session.terminal()：NTtyDriver → /dev/console；PtyDriver → /dev/pts/N
  包成 file::File
OpenResult::Dir(dir) → Directory::new
O_NONBLOCK → set_nonblocking(true)
add_file_like(f, O_CLOEXEC)
```

### 2.3 `close_range`

- `first < 0 || last < first` → `InvalidInput`；
- `UNSHARE`：`FD_TABLE.scope_mut(&mut scope)` 取旧表 → `clone_from`（复制一份，达到 unshare 效果）；
- `CLOEXEC`：把范围内 fd 的 `cloexec = true`；否则逐个 `remove`。

### 2.4 `dup3`

- 校验 flags 只有 `O_CLOEXEC`；`old == new` → `InvalidInput`；
- 复制 `FileDescriptor`（`clone`，共享 `Arc<dyn FileLike>`），设 cloexec；
- `remove(new)` 后 `add_at(new, f)`。

### 2.5 `fcntl`

| cmd | 行为 |
|---|---|
| `F_DUPFD/F_DUPFD_CLOEXEC` | `dup_fd` |
| `F_SETLK/F_SETLKW/F_OFD_*` | 返回 0（不实现锁） |
| `F_GETLK/F_OFD_GETLK` | 写 `l_type = F_UNLCK` |
| `F_SETFL` | `set_nonblocking(arg & O_NONBLOCK)` |
| `F_GETFL` | 由 `stat().mode` 的 OWNER_READ/WRITE 推断 `O_RDONLY/O_WRONLY/O_RDWR` + `O_NONBLOCK` |
| `F_GETFD/F_SETFD` | 读写 `cloexec` |
| `F_GETPIPE_SZ/F_SETPIPE_SZ` | `Pipe::from_fd` 的容量/调整 |
| 其他 | `warn!` + 0 |

### 2.6 `flock`

返回 0（TODO：不实现文件锁）。

## 3. 不变量与约束

- **C12/C13**：新 fd 必须经 `add_file_like`/`FileLike::add_to_fd_table`；阻塞语义由 `FileLike` 承担。
- `add_to_fd` 的 ptmx 替换会**新建** `Location`（`Reference::new(Some(pts.entry().clone()), pty_number)`）；`/proc/<pid>/fd` 的 path 依赖它。
- `sys_openat` 的 `mode` 必须 `& !umask`（否则 umask 失效）。
- `dup3` 的 `add_at` 失败要返回 `BadFileDescriptor`（fd 越界）。
- `close_range(UNSHARE)` 用 `mem::take` 后 `clone_from`，原表被 drop（关闭所有 fd）——顺序不能反。
- `F_GETFL` 的权限推断基于 `stat().mode`，对 socket/pipe 会得到默认值（可能不准确）。
- `openat` 必须拒绝 `O_TMPFILE`（未实现，flags 位被忽略 → 会被当成普通 open）。

## 4. 与其他模块的关系

| 模块 | 关系 |
|---|---|
| `file/fs.rs` | `File::new`/`Directory` |
| `file/mod.rs` | `FD_TABLE`/`add_file_like`/`close_file_like` |
| `pseudofs/dev/tty/*` | ptmx/current-tty |
| `pseudofs/device.rs` | `Device` downcast |
| `pseudofs/mod.rs` | `/dev/pts` 路径 |
| `task/mod.rs` | `umask`、`scope` |
| `syscall/fs/io.rs` | dup 后的 read/write |

## 5. 已知局限 / TODO

- 无文件锁（`flock`/`fcntl` 锁都是空实现）。
- `O_TMPFILE`/`O_PATH` 的语义不完整（`O_PATH` 只设 open 选项）。
- `add_to_fd` 的 ptmx 替换用 `// TODO: this is cursed` 注释（构造 `Location` 的手工路径）。
- `close_range` 的 `UNSHARE` 语义是"复制表"，未真正 unshare 页表/其他资源。
- `F_GETFL` 对非普通文件不准。
- `O_CLOEXEC` 与 `dup3` 的 `cloexec` 覆盖逻辑（新 fd 用 flags，旧 fd 不变）。

## 6. 变更影响面

改本文件 → `file/mod.rs.md`、`file/fs.rs.md`、`pseudofs/dev/tty/{ptm,mod}.rs.md`、`syscall/fs/{io,pipe}.rs.md`、`CONSTRAINTS.md` C12/C13。
