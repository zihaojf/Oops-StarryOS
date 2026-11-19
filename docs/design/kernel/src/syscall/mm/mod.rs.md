# `kernel/src/syscall/mm/mod.rs`

> 对应源码：`kernel/src/syscall/mm/mod.rs`
> 所属子系统：内存 syscall（模块门面）
> 依赖：`brk`/`mincore`/`mmap`

## 1. 文件职责

纯门面：

```rust
mod brk; mod mincore; mod mmap;
pub use self::{brk::*, mincore::*, mmap::*};
```

| 文件 | syscall |
|---|---|
| `brk.rs` | `brk` |
| `mmap.rs` | `mmap/munmap/mprotect/mremap/madvise/msync/mlock/mlock2` |
| `mincore.rs` | `mincore` |

## 2. 不变量与约束

- 三个实现都必须以 `current().as_thread().proc_data.aspace` 为操作对象（C8）。
- 用户地址参数（`VirtAddr`）必须经 4K 对齐/范围校验（各实现内做）。
- 新增 mm syscall 时在此声明并同步 `syscall/mod.rs`。

## 3. 变更影响面

改本文件 → `syscall/mm/*.md`、`syscall/mod.rs.md`。
