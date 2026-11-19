# `kernel/src/pseudofs/mod.rs`

> 对应源码：`kernel/src/pseudofs/mod.rs`
> 所属子系统：伪文件系统（挂载与公共类型）
> 依赖：`axfs::{FS_CONTEXT, FsContext}`、`axfs_ng_vfs::{Filesystem, path}`、`tmp::MemoryFs`
> 被依赖：`entry.rs`（`mount_all`）、`file/mod.rs`（`Device`、`DeviceOps` 等）、`syscall/fs/mount.rs`

## 1. 文件职责

1. 声明并重导出子模块（`device/dir/file/fs/proc/tmp`）以及 `dev`；
2. 定义 `DirMaker`、`NodeOpsMux`（目录/文件节点的工厂抽象）；
3. `mount_all()`：在 rootfs 上挂载 `/dev`、`/dev/shm`、`/tmp`、`/proc`、`/sys`，并创建 Xorg 需要的 `/sys/class/graphics/fb0/device/subsystem` 软链接；
4. `mount_at()`：目录不存在则创建，然后 `mount`。

## 2. 关键类型

```rust
pub type DirMaker = Arc<dyn Fn(WeakDirEntry) -> Arc<dyn DirNodeOps> + Send + Sync>;
pub enum NodeOpsMux { Dir(DirMaker), File(Arc<dyn FileNodeOps>) }   // From impls 用于 .into()
const DIR_PERMISSION: NodePermission = 0o755;
```

`NodeOpsMux` 是"延迟创建"的核心：目录节点不能在 lookup 前实例化（需要 `WeakDirEntry`），因此用闭包工厂。

## 3. 挂载布局

| 路径 | 文件系统 | 说明 |
|---|---|---|
| `/dev` | `dev::new_devfs()`（`SimpleFs`） | 设备节点 |
| `/dev/shm` | `tmp::MemoryFs::new()` | POSIX shm |
| `/tmp` | `tmp::MemoryFs::new()` | 临时文件（memfd 落此） |
| `/proc` | `proc::new_procfs()` | 进程/系统信息 |
| `/sys` | `tmp::MemoryFs::new()` + 软链接 | Xorg 探测 |
| `/sys/class/graphics/fb0/device/subsystem` | symlink → `whatever` | 让 Xorg 认为有 fb 设备 |

## 4. 关键流程

```rust
pub fn mount_all() -> LinuxResult<()> {
    let fs = FS_CONTEXT.lock();
    mount_at(&fs, "/dev", dev::new_devfs())?;
    mount_at(&fs, "/dev/shm", MemoryFs::new())?;
    mount_at(&fs, "/tmp", MemoryFs::new())?;
    mount_at(&fs, "/proc", proc::new_procfs())?;
    mount_at(&fs, "/sys", MemoryFs::new())?;
    // 逐级创建 /sys/class/graphics/fb0/device，再 symlink("whatever", .../subsystem)
    drop(fs);
    #[cfg(feature = "dev-log")] dev::bind_dev_log()?;
}
```

`mount_at`：`fs.resolve(path).is_err()` 时 `create_dir(path, 0755)`，然后 `resolve(path)?.mount(&mount_fs)`。

## 5. 不变量与约束

- **挂载顺序**：`/dev` 必须在 `/dev/shm` 之前（后者需要在 `/dev` 下创建目录）；`/sys` 的软链接依赖 `/sys` 已挂载。
- `mount_all` 持有 `FS_CONTEXT` 锁完成所有挂载；`dev-log` 的 bind 在 `drop(fs)` 之后（它内部会解析 `/dev/log`，不能再持锁）。
- `mount_at` 幂等：已存在目录不重复创建；但重复挂载会覆盖（`mount` 语义）。
- `DirMaker` 必须 `Send + Sync`；闭包内不能捕获非 Send 的 `Rc`。
- `NodeOpsMux::File` 要求 `FileNodeOps + Send + Sync + 'static`。
- `/sys/class/graphics/fb0/device/subsystem` 的目标 `"whatever"` 必须是**存在的路径**吗？不需要——Xorg 只 `stat` 软链接（follow 后 ENOENT 也可接受），当前实现就是这么用的。

## 6. 与其他模块的关系

| 模块 | 关系 |
|---|---|
| `pseudofs/dev/mod.rs` | `new_devfs()` 与设备节点 |
| `pseudofs/proc.rs` | `new_procfs()` |
| `pseudofs/tmp.rs` | `MemoryFs` |
| `pseudofs/{fs,dir,file,device}.rs` | `SimpleFs` 基础设施 |
| `entry.rs` | `mount_all()` |
| `syscall/fs/mount.rs` | `mount("tmpfs")` 也创建 `MemoryFs` |
| `file/mod.rs` | `add_stdio` 打开 `/dev/console`（依赖本文件挂载 `/dev`） |

## 7. 已知局限 / TODO

- `/sys` 只构造了最小骨架（不是真 sysfs）。
- 没有 `/dev/pts` 之外的动态设备管理（udev 语义）。
- `mount_all` 失败即 panic（`entry.rs` 的 `expect`）。
- `/proc` 的 `mounts` 内容是硬编码的。

## 8. 变更影响面

改本文件 → `pseudofs/dev/mod.rs.md`、`pseudofs/proc.rs.md`、`pseudofs/tmp.rs.md`、`entry.rs.md`、`file/mod.rs.md`、`ARCHITECTURE.md#8`。
