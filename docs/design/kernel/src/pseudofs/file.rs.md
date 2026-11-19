# `kernel/src/pseudofs/file.rs`

> 对应源码：`kernel/src/pseudofs/file.rs`
> 所属子系统：伪文件系统 / 普通文件
> 依赖：`axfs_ng_vfs::{FileNodeOps, NodeOps, NodeFlags}`、`inherit_methods_macro`
> 被依赖：`pseudofs/proc.rs`、`pseudofs/dev/mod.rs`（`log` 文件）

## 1. 文件职责

1. `SimpleFileOps` trait：伪文件的内容提供者（`read_all` / `write_all`）；
2. `SimpleFileOperation`：`Read` / `Write(&[u8])` 请求枚举；
3. `RwFile<F>`：把 `Fn(SimpleFileOperation) -> VfsResult<Option<R>>` 适配成 `SimpleFileOps`；
4. `impl<F,R> SimpleFileOps for F where F: Fn() -> VfsResult<R>`：只读闭包自动可作文件；
5. `SimpleFile`：`NodeOps + FileNodeOps` 实现（读时取全部内容再切片，写时整文件替换）。

## 2. 关键类型

```rust
pub struct SimpleFile {
    node: SimpleFsNode,
    ops: Arc<dyn SimpleFileOps>,
}
impl SimpleFile {
    pub fn new(fs, ty: NodeType, ops: impl SimpleFileOps) -> Arc<Self>;
    pub fn new_regular(fs, ops) -> Arc<Self>;      // NodeType::RegularFile
}
```

## 3. 读写语义

| 方法 | 行为 |
|---|---|
| `read_at(buf, offset)` | `read_all()` → 从 `offset` 切片，`min(buf.len())` 拷贝 |
| `write_at(buf, offset)` | `read_all()`；`offset==0 && buf.len() >= len` → 直接 `write_all(buf)`；否则拷贝到旧内容（必要时 `resize`）再 `write_all` |
| `append(buf)` | 读旧内容 + 追加 → `write_all`，返回 `(len, new_len)` |
| `set_len(len)` | 读出后截断/零填充 → `write_all` |
| `set_symlink(target)` | `write_all(target.as_bytes())`（用于 `/proc/<pid>/exe` 等） |
| `flags()` | `NON_CACHEABLE`（动态内容，不能走页缓存） |

## 4. 不变量与约束

- **C27**：`SimpleFile` 的内容必须"每次读取时生成"（闭包），不能缓存到静态变量；`flags = NON_CACHEABLE` 保证 VFS 不缓存内容。
- `write_at` 的"整文件替换"优化只在 `offset==0 && buf.len() >= len` 时成立；其他情况读-改-写，非原子。
- `read_all` 返回 `Cow`：`RwFile` 用 `Cow::Owned`（`fn` 返回 `Vec`/`Bytes` 转换），闭包实现也 `Owned`。
- `SimpleFile` 覆盖 `SimpleFsNode::len()` 为 `read_all()?.len()`；`metadata.size` 因此正确。
- `SimpleDirOps` 的 `lookup_child` 构造 `SimpleFile` 时必须传 `NodeType`（`RegularFile` 或 `Symlink`）。
- `RwFile` 的 `read_all` 里 `unwrap()` 依赖闭包 `Read` 分支返回 `Some`；若闭包返回 `None` 会 panic（当前所有实现都返回 `Some`）。

## 5. 与其他模块的关系

| 模块 | 关系 |
|---|---|
| `pseudofs/fs.rs` | `SimpleFile` 内嵌 `SimpleFsNode` |
| `pseudofs/proc.rs` | `/proc/*` 全部用 `SimpleFile::new_regular` 或 `RwFile` 构造 |
| `pseudofs/dev/mod.rs` | `/dev/log` 用 `SimpleFile::new(fs, NodeType::Socket, \|\| Ok(b""))` |
| `syscall/fs/*` | 通过 `Location`/`FileNode` 最终调用 `read_at/write_at` |
| `pseudofs/dir.rs` | `lookup` 时把 `NodeOpsMux::File(ops)` 包成 `DirEntry` |

## 6. 已知局限 / TODO

- `write_at` 非原子（read-modify-write）。
- `RwFile::read_all` 的 `unwrap` 假设 `Read` 返回 `Some`。
- `flags` 固定 `NON_CACHEABLE`；需要缓存的文件不能用 `SimpleFile`。
- 无 `ioctl` 支持（`Device` 才有）；`/dev/log` 作为 socket 类型但没有真实 socket 语义（`dev-log` 实际用 axnet 服务器）。

## 7. 变更影响面

改本文件 → `pseudofs/fs.rs.md`、`pseudofs/proc.rs.md`、`pseudofs/dev/mod.rs.md`、`syscall/fs/io.rs.md`、`CONSTRAINTS.md` C27。
