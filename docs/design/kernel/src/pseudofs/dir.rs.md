# `kernel/src/pseudofs/dir.rs`

> 对应源码：`kernel/src/pseudofs/dir.rs`
> 所属子系统：伪文件系统 / 目录
> 依赖：`axfs_ng_vfs::{DirNodeOps, NodeOps, DirEntry, Reference, path::{DOT,DOTDOT}}`、`inherit_methods_macro`
> 被依赖：`pseudofs/{mod,proc,dev,tmp}.rs`

## 1. 文件职责

1. `SimpleDirOps` trait：伪目录的"内容提供者"（`child_names`/`lookup_child`/`is_cacheable`/`chain`）；
2. `DirMapping`：`BTreeMap<String, NodeOpsMux>` 的简易实现（最常用）；
3. `ChainedDirOps<A,B>`：把两个目录合并（`/proc` 根把动态 pid 目录与静态文件链在一起）；
4. `SimpleDir<O>`：`DirNodeOps` 实现，负责 `read_dir/lookup` 与创建/链接/重命名的"不支持"错误。

## 2. 关键类型

```rust
pub trait SimpleDirOps: Send + Sync + 'static {
    fn child_names<'a>(&'a self) -> Box<dyn Iterator<Item = Cow<'a, str>> + 'a>;
    fn lookup_child(&self, name: &str) -> VfsResult<NodeOpsMux>;
    fn is_cacheable(&self) -> bool { true }
    fn chain<N: SimpleDirOps>(self, other: N) -> ChainedDirOps<Self, N>;
}
pub struct DirMapping(BTreeMap<String, NodeOpsMux>);
pub struct ChainedDirOps<A, B>(A, B);
pub struct SimpleDir<O> { node: SimpleFsNode, this: WeakDirEntry, ops: Arc<O> }
```

## 3. 关键流程

### 3.1 `read_dir(offset, sink)`

```text
children = [".", ".."].chain(ops.child_names())，用 enumerate 得到序号 i
从 offset 开始：
    "." → this_entry.metadata()
    ".." → parent 的 metadata（无 parent 则自身）
    其他 → this_dir.lookup(name)?.metadata()
    sink.accept(name, ino, node_type, i+1) 失败 → break
返回 count
```

`DirEntrySink`（`getdents64` 用它填充 `linux_dirent64`）的 `accept` 返回 false 表示缓冲区满。

### 3.2 `lookup(name)`

```text
ops = ops.lookup_child(name)?
reference = Reference::new(self.this.upgrade(), name)
match ops:
  Dir(maker)  → DirEntry::new_dir(|this| DirNode::new(maker(this)), reference)
  File(ops)   → DirEntry::new_file(FileNode::new(ops.clone()), ops.metadata()?.node_type, reference)
```

### 3.3 `ChainedDirOps`

- `child_names`：两段迭代器串接（可能有重复名，`lookup` 时先命中 A）。
- `lookup_child`：先 A，`NotFound` 再 B，其他错误透传。
- `is_cacheable`：`A && B`（注释：一边不可缓存一边可缓存时行为未定义）。

## 4. 不变量与约束

- **C15**：`create/link/unlink/rename` 一律返回 `OperationNotPermitted`（伪目录只读）；不要改成 `InvalidInput`，`mkdir`/`rm` 等依赖具体 errno。
- `read_dir` 的序号 `i+1` 作为 `d_off`，必须单调递增（`getdents64` 用它定位下一次读取）。
- `lookup` 的 `Reference::new` 依赖 `self.this` 已升级；`this` 是 `WeakDirEntry`，在 `DirMaker` 创建时由 VFS 提供。
- `is_cacheable()` 返回 false 的目录（`/proc/<pid>` 等）不会被 VFS 缓存 dentry，每次 lookup 重新构造——这是 `/proc` 动态性的关键。
- `DirMapping` 用 `BTreeMap`，`child_names` 顺序稳定（`ls` 输出稳定）。
- `ChainedDirOps` 的两段不能有同名文件（后者不可达）；`/proc` 用 `ProcFsHandler.chain(root)`，`root` 里的 `mounts/meminfo/...` 与动态 pid 名不冲突。

## 5. 与其他模块的关系

| 模块 | 关系 |
|---|---|
| `pseudofs/fs.rs` | `SimpleDir::new_maker` 用 `SimpleFsNode` |
| `pseudofs/mod.rs` | `DirMaker`/`NodeOpsMux` 定义 |
| `pseudofs/proc.rs` | `ThreadDir/ProcessTaskDir/ProcFsHandler` 都实现 `SimpleDirOps` |
| `pseudofs/dev/mod.rs` | `builder` 用 `DirMapping` |
| `pseudofs/tmp.rs` | 自己的 `MemoryNode`（不实现 `SimpleDirOps`） |
| `syscall/fs/ctl.rs` | `getdents64` 通过 `Location::read_dir` 最终调用 `read_dir` |

## 6. 已知局限 / TODO

- `read_dir` 每次从头构造子项迭代器（`skip(offset)`），大目录低效。
- `is_cacheable` 的链式语义未定义（TODO 注释）。
- 不支持 `opendir/readdir` 的 `d_type` 之外的 `telldir/seekdir`（靠 offset 序号）。
- 没有 `.`/`..` 之外的特殊项。

## 7. 变更影响面

改本文件 → `pseudofs/fs.rs.md`、`pseudofs/proc.rs.md`、`pseudofs/dev/mod.rs.md`、`syscall/fs/ctl.rs.md`、`CONSTRAINTS.md` C15。
