# `kernel/src/syscall/fs/mount.rs`

> 对应源码：`kernel/src/syscall/fs/mount.rs`
> 所属子系统：mount/umount syscall
> 依赖：`axfs::FS_CONTEXT`、`pseudofs::MemoryFs`、`mm::vm_load_string`
> 被依赖：`syscall/fs/mod.rs`

## 1. 文件职责

实现 `mount` 与 `umount2`：`tmpfs`/`ramfs` 挂真实的内存文件系统；FAT 家族（`vfat`/`fat`/`msdos`/`exfat`）按伪挂载处理；其余 `fs_type` 返回 `ENODEV`：

```rust
pub fn sys_mount(source, target, fs_type, _flags, _data) -> AxResult<isize> {
    let fs = match fs_type.as_str() {
        "tmpfs" | "ramfs" => MemoryFs::new(),
        "vfat" | "fat" | "msdos" | "exfat" => MemoryFs::new(),
        _ => return Err(AxError::NoSuchDevice),
    };
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

- `fs_type` 精确匹配：`tmpfs`/`ramfs` 真实挂载内存文件系统；`vfat`/`fat`/`msdos`/`exfat` 伪挂载（见 §3）；`ext4`/`proc`/`devfs`/`sysfs` 等返回 `NoSuchDevice`（`ENODEV`）。
- 伪挂载只保证 `mount`/`umount2` 往返闭合：挂载点上出现一个空的内存文件系统，不解析、也不写回任何块设备内容。
- `source` 被忽略（没有任何 `fs_type` 会真正打开块设备）。
- `flags`（`MS_RDONLY/NOSUID/NODEV/...`）与 `data`（如 `size=`）被忽略。
- 目标路径必须已存在（`resolve`），不会自动创建挂载点；目标已被挂载时 `Location::mount` 返回 `EBUSY`（`axfs_ng_vfs` 的 `ResourceBusy`）。
- `umount2` 忽略 flags（`MNT_FORCE/MNT_DETACH` 无效）；挂载点仍有子挂载时返回 `EBUSY`。

## 3. 不变量与约束

- **C6**：`source/target/fs_type` 都经 `vm_load_string`。
- `mount` 必须在 `FS_CONTEXT` 下执行（scope-local）；不能跨进程/命名空间。相对路径（如 `./mnt`）由 `FS_CONTEXT` 的当前工作目录解析，`chdir`/`fchdir` 已在 `syscall/fs/ctl.rs` 同步。
- `fs_type` 比较是精确字符串；不要放宽到“包含”，也不要放行 `ext4`——rootfs 由 `axfs-ng` 的 ext4 后端真实挂载，伪挂载会掩盖真实的挂载失败。
- 伪挂载成立的唯一前提是评测镜像没有可解析的 FAT 卷：`sdcard-*.img` 是整盘 ext4、无分区表，`/dev/vda2` 既不是有效设备节点也不是 FAT 卷。镜像一旦带 FAT 分区，就必须改成真实挂载。
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

- FAT 家族是伪挂载：不读块设备、不解析 FAT，挂载点内容为空且不落盘；`basic/mount`、`basic/umount` 只校验返回值，故可通过。
- 真 FAT 挂载需要三件事：`axfeat` 打开 `fs-ng-fat`；留住 `axfs::init_filesystems` 里被 `take_one()` 消费掉的块设备句柄；把 `/dev/vdaN` 映射成带偏移的分区设备。
- 忽略 flags/data，不自动创建挂载点。
- 无 `MS_BIND`/`MS_MOVE`/`MS_REC`。
- `umount2` 的强制卸载未实现。

## 6. 变更影响面

改本文件 → `pseudofs/tmp.rs.md`、`syscall/fs/mod.rs.md`、`ARCHITECTURE.md#8`。
