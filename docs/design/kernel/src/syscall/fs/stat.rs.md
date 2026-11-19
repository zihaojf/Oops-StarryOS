# `kernel/src/syscall/fs/stat.rs`

> 对应源码：`kernel/src/syscall/fs/stat.rs`
> 所属子系统：文件元数据 syscall
> 依赖：`axfs::FS_CONTEXT`、`axfs_ng_vfs::{Location, NodePermission}`、`file::{File, FileLike, resolve_at}`、`mm::vm_load_string`、`linux_raw_sys::general::{stat, statx, statfs, R_OK/W_OK/X_OK}`
> 被依赖：`syscall/fs/mod.rs`

## 1. 文件职责

实现 `stat/fstat/lstat/fstatat(2)/statx/access/faccessat/faccessat2/statfs/fstatfs`。

## 2. 关键实现

### 2.1 `sys_fstatat(dirfd, path, statbuf, flags)`

```text
path = nullable → vm_load_string
loc = resolve_at(dirfd, path, flags)?
statbuf.vm_write(loc.stat()?.into())?     // Kstat → stat
```

`resolve_at` 处理 `AT_FDCWD`/`AT_EMPTY_PATH`/`AT_SYMLINK_NOFOLLOW`（见 `file/fs.rs`）。

### 2.2 `sys_statx`

同样用 `resolve_at`，但写 `statx`（`Kstat → statx` 在 `file/mod.rs`）。`_mask` 忽略。

### 2.3 `sys_faccessat2(dirfd, path, mode, flags)`

- `mode == 0` → `Ok(0)`（只检查存在）。
- 把 `R_OK/W_OK/X_OK` 映射成 `NodePermission::OWNER_READ/WRITE/EXEC`，检查 `stat().mode & required == required`，否则 `PermissionDenied`。

### 2.4 `statfs`

```text
stat = loc.filesystem().stat()?            // StatFs
result.f_type = fs_type; f_bsize/f_blocks/f_bfree/f_bavail/f_files/f_ffree
result.f_fsid = __kernel_fsid_t { val: [0, loc.mountpoint().device()] }
f_namelen/f_frsize/f_flags
```

`sys_statfs` 解析路径 → `loc.mountpoint().root_location()`；`sys_fstatfs` 用 `File::from_fd(fd).inner().location()`。

## 3. 不变量与约束

- **C6**：路径经 `vm_load_string`；输出结构经 `vm_write`。
- **C21**：错误映射为 errno；`access` 的权限不足返回 `PermissionDenied`（`EACCES`）。
- `fstat`/`fchmod`/`fchown` 等用 `AT_EMPTY_PATH` + `fd` 作为 `resolve_at` 的 dirfd 特例。
- `sys_faccessat2` 的权限检查是 **OWNER 位**（不区分 uid/gid），因为系统无权限模型；这是简化实现。
- `statx` 的 `_mask` 被忽略：总是返回全部字段（`stx_mask` 未设置，危险——程序可能认为返回了全部信息）。
- `statfs` 的 `f_fsid` 用挂载点 device（伪造）。
- `x86_64` 专属的 `stat/lstat/access` 用 `#[cfg]`；`fstatat` 在 riscv64 是 `newfstatat`。

## 4. 与其他模块的关系

| 模块 | 关系 |
|---|---|
| `file/fs.rs` | `resolve_at`/`metadata_to_kstat` |
| `file/mod.rs` | `Kstat`/`FileLike::stat` |
| `pseudofs/*` | 各文件系统的 `NodeOps::metadata` |
| `syscall/fs/ctl.rs` | `chmod/chown/utimes` 修改元数据 |
| `syscall/fs/fd_ops.rs` | `F_GETFL` 用 `stat().mode` |

## 5. 已知局限 / TODO

- `statx` 未设置 `stx_mask`（调用方无法判断哪些字段有效）。
- `access` 不检查真实 uid/gid（全 root）。
- `statfs` 的容量是伪数据（`dummy_stat_fs`）。
- 无 `fstatat64`/`stat64` 的 32 位 ABI 变体。
- `statx` 的 `AT_STATX_SYNC_*` 忽略。

## 6. 变更影响面

改本文件 → `file/fs.rs.md`、`file/mod.rs.md`、`pseudofs/fs.rs.md`、`syscall/fs/{ctl,fd_ops}.rs.md`、`CONSTRAINTS.md` C6/C21。
