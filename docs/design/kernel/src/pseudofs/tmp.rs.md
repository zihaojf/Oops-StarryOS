# `kernel/src/pseudofs/tmp.rs`

> 对应源码：`kernel/src/pseudofs/tmp.rs`
> 所属子系统：伪文件系统 / 内存文件系统（tmpfs）
> 依赖：`axfs_ng_vfs::{Filesystem, FilesystemOps, DirEntry, DirNode, FileNode, NodeOps, ...}`、`slab::Slab`、`hashbrown::HashMap`
> 被依赖：`pseudofs/mod.rs`（`/tmp`、`/sys`、`/dev/shm`）、`syscall/fs/mount.rs`（`mount("tmpfs")`）

## 1. 文件职责

实现**可读写**的内存文件系统 `MemoryFs`：支持目录/文件/符号链接/硬链接/重命名，文件内容委托页缓存（`NodeFlags::ALWAYS_CACHE`）。

## 2. 关键类型

```rust
struct FileName(String);                 // Ord: "." < ".." < 其他（BTree 语义用于 readdir 顺序）
pub struct MemoryFs { inodes: Mutex<Slab<Arc<Inode>>>, root: Mutex<Option<DirEntry>> }
struct Inode { ino: u64, metadata: Mutex<Metadata>, content: NodeContent }
enum NodeContent { File(FileContent), Dir(DirContent) }
struct FileContent { length: Mutex<u64>, symlink: Mutex<Option<String>> }
struct DirContent  { entries: Mutex<HashMap<FileName, InodeRef>> }
struct InodeRef { fs: Arc<MemoryFs>, ino: u64 }     // 引用计数：nlink
struct MemoryNode { fs: Arc<MemoryFs>, inode: Arc<Inode>, this: Option<WeakDirEntry> }
```

## 3. 关键流程

### 3.1 创建

- `MemoryFs::new()`：建 `inodes` slab，创建根目录 inode（`Inode::new`，parent=None → ".." 指向自己），`DirEntry::new_dir` + `Reference::root()`。
- `Inode::new`：slab 分配 ino；目录插入 `"."` 与 `".."`；`InodeRef::new` 使 `nlink += 1`。

### 3.2 查找/创建/删除/重命名（`DirNodeOps for MemoryNode`）

| 操作 | 行为 |
|---|---|
| `read_dir` | 遍历 `entries`，`skip(offset)`，`sink.accept(name, ino, node_type, i+1)` |
| `lookup` | `entries.get(name)` → `new_entry`（目录包 `DirEntry::new_dir`，其他 `FileNode`） |
| `create` | `entries.contains_key` → `AlreadyExists`；否则 `Inode::new` + 插入 |
| `link` | 目标必须 `downcast::<Self>()`；插入 `InodeRef`（硬链接，nlink+1） |
| `unlink` | 目录非空 → `DirectoryNotEmpty`；`entries.remove` → `InodeRef::drop` → nlink-1 |
| `rename` | 目标已存在且是同 inode → `Ok(())`；否则从源删除并插入目标；`// TODO: atomicity` |

### 3.3 文件内容

- `FileContent` 只存 `length` 与 `symlink`，实际数据在页缓存（`NodeFlags::ALWAYS_CACHE`）。
- `read_at`：仅符号链接有实现（从 `symlink` 拷贝）；普通文件 `unreachable!("page cache should handle reading")`。
- `write_at/append`：`unreachable!`（页缓存写）。
- `set_len`：改 `length`；`set_symlink`：设置 `symlink` 与 `length`。
- `metadata.size`：文件用 `length`，目录用 `entries.len()`。

## 4. 不变量与约束

- **`NodeFlags::ALWAYS_CACHE`**：普通文件读写必须走页缓存；如果直接调用 `MemoryNode::read_at` 会 `unreachable!` panic。因此 VFS 层必须正确路由（`axfs` 负责）。
- **nlink 与引用计数**：`InodeRef` 的创建/销毁维护 `metadata.nlink`；`release_inode` 在 `nlink == 0 && Arc::strong_count(inode) == 2` 时从 slab 移除（2 = slab + 临时 `get()` 的克隆）。
- `InodeRef::drop` 会 `self.get()` 再 `release_inode`，依赖 slab 中仍存在该 inode（若已移除会 panic）。
- `unlink` 只在目录 `entries.len() > 2` 时返回 `DirectoryNotEmpty`；`entries` 不含 "."/".."（它们在创建时插入，算在 len 里）。
- `rename` 的"同 inode 直接成功"是 POSIX 要求；其他情况非原子（TODO）。
- `MemoryNode::drop`：目录清空 `entries`，然后 `release_inode(..., 0)`（目录自身不占 nlink）。
- `FileName` 的 `Ord` 只影响 `child_names`（`HashMap` 不排序！）——`read_dir` 顺序实际是 HashMap 迭代顺序，**不稳定**。注释里的 Ord 仅用于 `BTree` 语义的设想，当前 `DirContent.entries` 是 `HashMap`。

## 5. 与其他模块的关系

| 模块 | 关系 |
|---|---|
| `pseudofs/mod.rs` | `/tmp`、`/sys`、`/dev/shm` 挂载 |
| `syscall/fs/mount.rs` | `mount("tmpfs")` |
| `syscall/fs/memfd.rs` | 在 `/tmp` 下创建 `memfd-XXXX` 文件 |
| `syscall/ipc/shm.rs` | 不直接使用（用 `SharedPages`） |
| `axfs` | 页缓存负责文件数据 |

## 6. 已知局限 / TODO

- `read_dir` 顺序依赖 `HashMap` 迭代（不稳定）；`FileName` 的 `Ord` 未生效。
- `rename` 非原子（TODO）。
- 不限制文件大小/总容量（无 tmpfs 大小上限）。
- 无 `O_TMPFILE`、无 `fallocate` 优化（`set_len` 只改长度，页缓存按需分配）。
- 符号链接内容与普通文件共用 `FileContent`，`read_at` 的 `assert_eq!(offset, 0)` 假设一次读完（读链接时 offset 总为 0）。

## 7. 变更影响面

改本文件 → `pseudofs/mod.rs.md`、`syscall/fs/mount.rs.md`、`syscall/fs/memfd.rs.md`、`syscall/fs/ctl.rs.md`（readdir/rename）、`ARCHITECTURE.md#8`。
