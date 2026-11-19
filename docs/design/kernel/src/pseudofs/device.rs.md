# `kernel/src/pseudofs/device.rs`

> 对应源码：`kernel/src/pseudofs/device.rs`
> 所属子系统：伪文件系统 / 设备节点
> 依赖：`axfs::CachedFile`、`axfs_ng_vfs::{FileNodeOps, NodeOps, DeviceId, NodeFlags}`、`axpoll::Pollable`、`memory_addr::PhysAddrRange`
> 被依赖：`pseudofs/mod.rs`（重导出）、`pseudofs/dev/*`、`file/mod.rs`（`File::from_fd` 的 downcast）、`syscall/mm/mmap.rs`（`DeviceMmap`）

## 1. 文件职责

1. `DeviceOps` trait：字符/块设备的操作（`read_at/write_at/ioctl/as_any/as_pollable/mmap/flags`）；
2. `DeviceMmap`：设备 mmap 行为枚举（`None/Physical/ReadOnly/Cache`）；
3. `Device`：把 `DeviceOps` 适配成 VFS 节点（`NodeOps + FileNodeOps + Pollable`）。

## 2. 关键类型

```rust
pub enum DeviceMmap {
    None,                       // 不可 mmap
    Physical(PhysAddrRange),    // 映射物理地址区间（fb0）
    ReadOnly,                   // 以 COW 只读方式映射
    Cache(CachedFile),          // 映射页缓存（loop 设备）
}
pub trait DeviceOps: Send + Sync {
    fn read_at(&self, buf, offset) -> VfsResult<usize>;
    fn write_at(&self, buf, offset) -> VfsResult<usize>;
    fn ioctl(&self, cmd, arg) -> VfsResult<usize> { Err(NotATty) }
    fn as_any(&self) -> &dyn Any;
    fn as_pollable(&self) -> Option<&dyn Pollable> { None }
    fn mmap(&self) -> DeviceMmap { None }
    fn flags(&self) -> NodeFlags { empty() }
}
pub struct Device { node: SimpleFsNode, ops: Arc<dyn DeviceOps> }
```

## 3. 关键行为

| 项 | 行为 |
|---|---|
| `Device::new` | 建 `SimpleFsNode`（默认权限），把 `device_id` 写入 `metadata.rdev` |
| `set_device_id` | 动态改 `rdev`（`/dev/pts/N` 用） |
| `NodeOps::len` | 0（设备没有固定长度） |
| `NodeOps::flags` | 转发 `DeviceOps::flags`（`NON_CACHEABLE/STREAM` 等） |
| `NodeOps::sync` | `InvalidInput` |
| `FileNodeOps::append` | `NotATty`（设备不支持追加） |
| `set_len` | 用 `write_at(b"", 0)` 是否成功判断是否可写 |
| `FileNodeOps::ioctl` | 转发 `DeviceOps::ioctl` |
| `Pollable` | 有 `as_pollable` 就转发，否则恒 `IN|OUT` |

## 4. 不变量与约束

- **C28**：`DeviceOps` 实现必须自行加锁保护内部状态；VFS 不做串行化。
- `as_any()` 是 `syscall/fs/fd_ops.rs` 判断 `/dev/ptmx`、`/dev/tty` 等特殊设备的基础；返回 `self` 的类型必须与实际一致。
- `DeviceMmap::Physical` 的 `PhysAddrRange` 必须连续且有效；`syscall/mm/mmap.rs` 会把 `offset` 加到 `range.start` 并用 `Linear` 映射（无效物理地址会导致不可恢复缺页）。
- `set_len` 的"用空写探测"会真实执行一次 `write_at`，设备实现要能接受空写。
- `flags()` 必须与设备语义一致：流设备要 `STREAM`（`read` 不被页缓存/预读影响），动态内容要 `NON_CACHEABLE`。
- `Device::new` 的 `NodePermission::default()` 由 `SimpleFsNode` 决定（当前默认权限位）。

## 5. 与其他模块的关系

| 模块 | 关系 |
|---|---|
| `pseudofs/dev/mod.rs` | 所有设备节点用 `Device::new` 注册 |
| `pseudofs/dev/{event,fb,loop,memtrack,rtc,log}.rs` | 实现 `DeviceOps` |
| `pseudofs/dev/tty/*` | `Tty<R,W>` 实现 `DeviceOps` |
| `syscall/fs/fd_ops.rs` | `add_to_fd` 里 `downcast::<Device>()` + `as_any().downcast_ref::<Ptmx/CurrentTty>` |
| `syscall/mm/mmap.rs` | `DeviceMmap` 分支 |
| `pseudofs/fs.rs` | `SimpleFsNode` |

## 6. 已知局限 / TODO

- 没有真正的块设备层（loop 设备自己实现 `read_at/write_at`）。
- `set_len` 的探测写有副作用（对某些设备）。
- `flags()` 默认空，新设备容易忘记设置 `NON_CACHEABLE`。
- `DeviceMmap::ReadOnly` 的语义在 mmap 中映射为 Cow + 文件后端（见 `syscall/mm/mmap.rs`）。

## 7. 变更影响面

改本文件 → `pseudofs/dev/*.md`、`syscall/fs/fd_ops.rs.md`、`syscall/mm/mmap.rs.md`、`CONSTRAINTS.md` C28。
