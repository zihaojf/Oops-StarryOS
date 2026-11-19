# `kernel/src/syscall/fs/mount.rs`

> 对应源码：`kernel/src/syscall/fs/mount.rs`
> 所属子系统：mount/umount syscall
> 依赖：`axfs::FS_CONTEXT`、`pseudofs::MemoryFs`、`mm::vm_load_string`
> 被依赖：`syscall/fs/mod.rs`

## 1. 文件职责

实现 `mount` 与 `umount2`——目前只支持 `tmpfs`：

```rust
pub fn sys_mount(source, target, fs_type, _flags, _data) -> AxResult<isize> {
    if fs_type != "tmpfs" { return Err(NoSuchDevice) }
    let fs = MemoryFs::new();
    let target = FS_CONTEXT.lock().resolve(target)?;
    target.mount(&fs)?;
    Ok(0)
}
pub fn sys_umount2(target, _flags) -> AxResult<isize> {
    FS_CONTEXT.lock().resolve(target)?.unmount()?;
    Ok(0)
}
```

## 2. 语义与限制

- 只接受 `fs_type == "tmpfs"`；`ext4`/`proc`/`devfs`/`sysfs` 等返回 `NoSuchDevice`（`ENODEV`）。
- `source` 被忽略（tmpfs 无源设备）。
- `flags`（`MS_RDONLY/NOSUID/NODEV/...`）与 `data`（如 `size=`）被忽略。
- 目标路径必须已存在（`resolve`），不会自动创建挂载点。
- `umount2` 忽略 flags（`MNT_FORCE/MNT_DETACH` 无效）；`target.unmount()` 由 `axfs_ng_vfs` 实现。

## 3. 不变量与约束

- **C6**：`source/target/fs_type` 都经 `vm_load_string`。
- `mount` 必须在 `FS_CONTEXT` 下执行（scope-local）；不能跨进程/命名空间。
- 挂载点已挂载时 `target.mount` 的行为由 VFS 决定（可能覆盖或报错）。
- `fs_type` 比较是精确字符串；不要放宽到"包含"。
- 返回 `isize`（0 成功）；`sys_mount` 在 `syscall/mod.rs` 里 `as _` 转换。

## 4. 与其他模块的关系

| 模块 | 关系 |
|---|---|
| `pseudofs/tmp.rs` | `MemoryFs` |
| `axfs_ng_vfs` | `Location::mount/unmount` |
| `pseudofs/mod.rs` | 启动时挂载 `/dev`/`/proc`/`/tmp` 等（不走本 syscall） |
| `syscall/fs/stat.rs` | `statfs` 反映挂载的文件系统 |
| `src/init.sh` | 可能调用 `mount`（Alpine 风格） |

## 5. 已知局限 / TODO

- 只支持 tmpfs。
- 忽略 flags/data。
- 不自动创建挂载点。
- 无 `MS_BIND`/`MS_MOVE`/`MS_REC`。
- `umount2` 的强制卸载未实现。

## 6. 变更影响面

改本文件 → `pseudofs/tmp.rs.md`、`syscall/fs/mod.rs.md`、`ARCHITECTURE.md#8`。
