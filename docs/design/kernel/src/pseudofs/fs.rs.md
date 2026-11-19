# `kernel/src/pseudofs/fs.rs`

> 对应源码：`kernel/src/pseudofs/fs.rs`
> 所属子系统：伪文件系统基础设施
> 依赖：`axfs_ng_vfs::{Filesystem, FilesystemOps, DirEntry, DirNode, NodeOps, Metadata, ...}`、`slab::Slab`
> 被依赖：`pseudofs/{mod,dir,file,device,proc,tmp}.rs`

## 1. 文件职责

提供伪文件系统的最小实现基类：

1. `dummy_stat_fs(fs_type)`：伪造 `StatFs`（`statfs/fstatfs` 返回的块数/文件数）；
2. `SimpleFs`：名字 + fs_type + inode slab + 根 `DirEntry`；
3. `SimpleFsNode`：通用节点（inode 号、metadata），实现 `NodeOps`；
4. 构造函数 `SimpleFs::new_with(name, fs_type, root_builder)`。

## 2. 关键类型

```rust
pub struct SimpleFs {
    name: String,
    fs_type: u32,
    inodes: Mutex<Slab<()>>,       // 只占位，用于分配 inode 号
    root: Mutex<Option<DirEntry>>,
}
pub struct SimpleFsNode {
    fs: Arc<SimpleFs>,
    ino: u64,
    pub(crate) metadata: Mutex<Metadata>,
}
```

`SimpleFs::new_with` 流程：

```text
fs = Arc::new(SimpleFs{...})
root = root_builder(fs.clone())              // DirMaker
fs.set_root(DirEntry::new_dir(|this| DirNode::new(root(this)), Reference::root()))
Filesystem::new(fs)
```

`inode` 分配：`Slab::insert(()) as u64 + 1`；`Drop for SimpleFsNode` 释放槽位。

## 3. `NodeOps for SimpleFsNode`

| 方法 | 行为 |
|---|---|
| `inode()` | 返回 slab 索引 + 1 |
| `metadata()` | 复制存储的 metadata，`size = self.len()?` |
| `len()` | 返回 0（子类如 `SimpleFile` 覆盖） |
| `update_metadata` | 更新 mode/owner/atime/mtime |
| `filesystem()` | `self.fs.as_ref()` |
| `sync()` | 空操作 |
| `into_any()` | `self`（downcast 用） |

## 4. 不变量与约束

- **C26**：inode 槽位生命周期由 `SimpleFsNode::new`/`Drop` 配对；不要手工 `alloc_inode`。`release_inode` 用 `ino as usize - 1`，依赖 slab 索引从 0 连续分配。
- `root` 在构造期间为 `None`，`root_dir()` 的 `unwrap()` 依赖"构造完成后才使用"；不要在 `new_with` 的闭包里访问 `root_dir()`。
- `dummy_stat_fs` 的 `blocks = 100, blocks_free = 100` 是假数据，仅用于让 `df`/`statfs` 不失败；不要用于真实容量判断。
- `metadata.size` 由 `len()` 动态计算（`SimpleFsNode::metadata`），子类覆盖 `len()` 即可让 `stat` 正确。
- `SimpleFs` 的 `fs_type` 值需与 Linux 常见值一致（`devfs`/`tmpfs` 用 `0x01021994`，`proc` 用 `0x9fa0`）。

## 5. 与其他模块的关系

| 模块 | 关系 |
|---|---|
| `pseudofs/dir.rs` | `SimpleDir<O>` 持有 `SimpleFsNode`，用 `inherit_methods` 转发 `NodeOps` |
| `pseudofs/file.rs` | `SimpleFile` 持有 `SimpleFsNode`，覆盖 `len/flags` |
| `pseudofs/device.rs` | `Device` 持有 `SimpleFsNode`，转发 `DeviceOps` |
| `pseudofs/proc.rs` | `new_procfs` 用 `SimpleFs::new_with("proc", 0x9fa0, builder)` |
| `pseudofs/tmp.rs` | 独立实现 `MemoryFs`（不用 `SimpleFs`） |
| `pseudofs/dev/mod.rs` | `new_devfs` 用 `SimpleFs::new_with("devfs", 0x01021994, builder)` |

## 6. 已知局限 / TODO

- `inodes: Slab<()>` 不存节点引用，只作 id 分配器；节点存活由 `Arc<dyn NodeOps>` 决定。
- `dummy_stat_fs` 的 `file_count/free_file_count` 为 0。
- 无 `stat` 的 `blocks/blksize` 真实统计。
- `SimpleFs` 不支持创建/删除挂载点、不支持 `remount`。

## 7. 变更影响面

改本文件 → `pseudofs/{dir,file,device,proc}.rs.md`、`pseudofs/mod.rs.md`、`syscall/fs/stat.rs.md`（statfs）。
