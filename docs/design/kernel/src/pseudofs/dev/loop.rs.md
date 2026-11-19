# `kernel/src/pseudofs/dev/loop.rs`

> 对应源码：`kernel/src/pseudofs/dev/loop.rs`
> 所属子系统：loop 块设备（`/dev/loopX`）
> 依赖：`axfs::FileBackend`、`linux_raw_sys::{ioctl::*, loop_device::*}`、`starry_vm`
> 被依赖：`pseudofs/dev/mod.rs`（注册 `loop0..15`）、`syscall/fs/mount.rs`（挂载镜像）

## 1. 文件职责

把普通文件包装成块设备，支持 loop 设备的 ioctl 协议与读写：

```rust
pub struct LoopDevice {
    number: u32,
    dev_id: DeviceId,
    pub file: Mutex<Option<FileBackend>>,   // 绑定的后端文件
    pub ro: AtomicBool,                     // 只读
    pub ra: AtomicU32,                      // 预读大小（字节）
}
```

## 2. 关键流程

### 2.1 ioctl

| cmd | 行为 |
|---|---|
| `LOOP_SET_FD` | `arg` 是 fd；downcast 到 `crate::file::File`；取 `inner().backend()?.clone()` 存入 `file`；已绑定 → `ResourceBusy` |
| `LOOP_CLR_FD` | 清空 `file`；未绑定 → `ENXIO` |
| `LOOP_GET_STATUS` | 写 `loop_info`（`lo_number`、`lo_rdevice`） |
| `LOOP_SET_STATUS` | 读 `loop_info`，`set_info`（当前忽略） |
| `BLKGETSIZE` | 扇区数 = `file.location().len()? / 512`，写 `u32` |
| `BLKGETSIZE64` | 字节数，写 `u64` |
| `BLKROGET` / `BLKROSET` | 读/写 `ro`（`BLKROSET` 只接受 0/1） |
| `BLKRAGET` / `BLKRASET` | 读/写 `ra` |
| 其他 | `warn!` + `NotATty` |

### 2.2 读写

- `read_at`：`file.read_at(buf, offset)`；未绑定 → `OperationNotPermitted`。
- `write_at`：`ro` → `ReadOnlyFilesystem`；否则转发。
- `mmap`：若 `file` 是 `FileBackend::Cached(cache)` → `DeviceMmap::Cache(cache.clone())`；否则 `None`。

## 3. 不变量与约束

- **绑定生命周期**：`file` 是 `Mutex<Option<FileBackend>>`；`clone_file()` 返回 clone 供 `BLKGETSIZE` 与 mmap 使用；不要在持锁时调用可能阻塞的操作（`read_at` 在 `read_at` 里先 clone 再调用，已避免）。
- `LOOP_SET_FD` 的 fd 必须是 `File`（不能是目录/pipe）；错误 `InvalidInput`。
- `ro` 默认 false；`BLKROSET` 只接受 0/1（其他 `InvalidInput`）。
- `ra` 默认 512 字节；只存不用（预读未实现）。
- `mmap` 只支持 `Cached` 后端；`Direct` 返回 `None`（不可 mmap）。
- `dev_id` 在创建时固定为 `7:0`（所有 loop 设备相同），`number` 区分实例。

## 4. 与其他模块的关系

| 模块 | 关系 |
|---|---|
| `pseudofs/dev/mod.rs` | 注册 16 个 loop 设备 |
| `pseudofs/device.rs` | `DeviceOps`/`DeviceMmap` |
| `syscall/fs/fd_ops.rs` | `open("/dev/loop0")` 得到 `File`，其 fd 传给 `LOOP_SET_FD` |
| `syscall/fs/mount.rs` | 挂载 loop 设备（通过 `mount`/`umount2`，但 `mount` 目前只支持 tmpfs） |
| `axfs::FileBackend` | 文件后端（`Cached`/`Direct`） |

## 5. 已知局限 / TODO

- `mount` 不支持挂载 loop 设备（`sys_mount` 只接受 `tmpfs`），因此 loop 设备目前无法真正用于镜像挂载。
- 预读大小（`ra`）未生效。
- `LOOP_SET_STATUS` 忽略大部分字段（`lo_offset`/`lo_encrypt` 等）。
- 无 `LOOP_CONFIGURE`（新接口）。
- 所有 loop 设备共享同一 `DeviceId`（`7:0`），用户态 `lsblk` 可能混淆。

## 6. 变更影响面

改本文件 → `pseudofs/dev/mod.rs.md`、`pseudofs/device.rs.md`、`syscall/fs/mount.rs.md`、`syscall/mm/mmap.rs.md`。
