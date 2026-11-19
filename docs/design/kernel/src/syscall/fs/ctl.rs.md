# `kernel/src/syscall/fs/ctl.rs`

> 对应源码：`kernel/src/syscall/fs/ctl.rs`
> 所属子系统：文件系统控制类 syscall
> 依赖：`axfs::{FS_CONTEXT, FsContext}`、`axfs_ng_vfs::{MetadataUpdate, NodePermission, NodeType, path::Path}`、`file::{Directory, FileLike, get_file_like, resolve_at, with_fs}`、`mm::vm_load_string`
> 被依赖：`syscall/fs/mod.rs`、`syscall/mod.rs`

## 1. 文件职责

实现路径/目录/元数据操作：`ioctl`、`chdir/fchdir/chroot`、`mkdir(at)`、`getdents64`、`link(at)`、`unlink(at)/rmdir`、`getcwd`、`symlink(at)`、`readlink(at)`、`chown/lchown/fchown(at)`、`chmod/fchmod/fchmodat`、`utime/utimes/utimensat`、`rename(at/at2)`、`sync/syncfs`。

## 2. 关键实现

### 2.1 `sys_ioctl`

- 特判 `FIONBIO`：读 `u8`，只接受 0/1，调 `set_nonblocking`；
- 其余转发 `FileLike::ioctl`；
- `NotATty` 且 `cmd == TIOCGWINSZ` 时**静默忽略**（glibc 习惯对非终端文件调它）；其他 `NotATty` 只 `warn!`。

### 2.2 `chdir/fchdir/chroot`

- `sys_chdir`：`FS_CONTEXT.resolve(path)` → `set_current_dir`。
- `sys_fchdir`：用 `with_fs(dirfd, |fs| fs.current_dir().clone())` 取目录，再设为 cwd。
- `sys_chroot`：必须是目录；`*fs = FsContext::new(loc)` 整体替换（root 变化）。

### 2.3 `mkdirat`

```text
mode &= !umask
resolve(path).is_ok() → AlreadyExists        // 注释：mkdir -p 依赖
fs.create_dir(path, mode)
```

### 2.4 `getdents64`

- `Directory::from_fd(fd)`；`dir.offset`（`Mutex<u64>`）记录位置；
- 内部 `DirBuffer` 按 `linux_dirent64` 填充（`d_ino/d_off/d_reclen/d_type/d_name`，名字后补 `\0`，按 `align_of::<linux_dirent64>()` 对齐）；
- `Location::read_dir` 回调：写成功则更新 `*dir_offset = offset`；失败返回 false 停止；
- 缓冲区一个都放不下但仍有条目 → `InvalidInput`（避免无限循环）；
- 最后 `vm_write_slice(buf, &buffer.buf)` 写整个缓冲区（含未使用部分）。

### 2.5 `linkat`/`unlinkat`

- `linkat`：`resolve_at(old)` 取源（必须是普通文件，目录 `OperationNotPermitted`），`resolve_nonexistent` 取目标目录+名字，`new_dir.link(name, &old)`；`flags != 0` 仅 `warn!`。
- `unlinkat`：`AT_REMOVEDIR` → `remove_dir`，否则 `remove_file`。

### 2.6 `getcwd`

- `size < 0` → `BadAddress`；`buf` 为空 → `Ok(0)`（异常约定）；
- `cwd.absolute_path()` + NUL，长度不够 → `OutOfRange`；成功返回 **buf 指针**（注释 FIXME，但保留以兼容）。

### 2.7 `chown/chmod/utimes/utimensat/renameat2`

- `chown`：清除 `SET_UID`，若 group-exec 再清 `SET_GID`；`-1` 表示不改。
- `utimensat`：`UTIME_NOW/UTIME_OMIT` 特殊值；`path` 为 NULL 时补 `AT_EMPTY_PATH`；两者都 OMIT 直接 `Ok(0)`。
- `renameat2`：`resolve_parent(old)` + `resolve_nonexistent(new)` → `old_dir.rename(...)`；`flags` 未使用（`RENAME_NOREPLACE` 等未实现）。

## 3. 不变量与约束

- **C15**：目录操作错误码要精确（`AlreadyExists/NotFound/OperationNotPermitted`）；`mkdir` 的 EEXIST 行为不能改。
- **C6**：所有路径经 `vm_load_string`；`getdents64` 的 `buf` 经 `vm_write_slice`。
- `with_fs` 不能在已持 `FS_CONTEXT` 锁时调用（`sys_chdir` 是直接持锁，`sys_fchdir` 用 `with_fs` 后再取锁——两段式，避免嵌套）。
- `getdents64` 的 `dir.offset` 与 `Location::read_dir` 的 `offset` 参数必须一致（序号从 1 开始，"." 为 1）。
- `utimensat` 的 NULL path 会转成 `AT_EMPTY_PATH`（Linux 6.11+ 语义）。
- `sys_sync`/`sys_syncfs` 是 no-op + `warn!`（未实现刷盘）。
- `sys_chmod`/`sys_chown` 等 x86_64 专属函数用 `#[cfg]` 隔离。

## 4. 与其他模块的关系

| 模块 | 关系 |
|---|---|
| `file/fs.rs` | `with_fs`/`resolve_at`/`Directory` |
| `file/mod.rs` | `get_file_like`/`FileLike::stat` |
| `mm/access.rs` | `vm_load_string`/`UserPtr` |
| `axfs_ng_vfs` | `Path/NodePermission/MetadataUpdate` |
| `pseudofs/tmp.rs` | rename/unlink 的实际实现 |
| `pseudofs/dir.rs` | read_dir 的 `DirEntrySink` |

## 5. 已知局限 / TODO

- `sync/syncfs` 未刷盘。
- `renameat2` 忽略 flags（无 `RENAME_NOREPLACE/EXCHANGE/WHITEOUT`）。
- `linkat` 忽略 flags（无 `AT_SYMLINK_FOLLOW`）。
- `getdents64` 每次都把整个 `len` 字节写回用户（包括未填充部分），可能覆盖调用方缓冲区尾部（Linux 只写实际长度；当前实现写 `buffer.buf` 全长）。
- `getcwd` 返回 buf 指针（FIXME）。
- `fchmodat` 的 `AT_SYMLINK_NOFOLLOW` 未实现（会跟随链接）。
- `ioctl` 未知命令只 warn，不返回错误（有 `NotATty` 返回）。

## 6. 变更影响面

改本文件 → `file/fs.rs.md`、`file/mod.rs.md`、`pseudofs/{tmp,dir}.rs.md`、`syscall/fs/{stat,io,fd_ops}.rs.md`、`CONSTRAINTS.md` C15。
