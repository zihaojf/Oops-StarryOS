# `kernel/src/syscall/sync/mod.rs`

> 对应源码：`kernel/src/syscall/sync/mod.rs`
> 所属子系统：同步类 syscall（门面）
> 依赖：`futex`/`membarrier`

## 1. 文件职责

```rust
mod futex; mod membarrier;
pub use self::{futex::*, membarrier::*};
```

| 文件 | syscall |
|---|---|
| `futex.rs` | `futex`、`get_robust_list`、`set_robust_list` |
| `membarrier.rs` | `membarrier` |

## 2. 不变量与约束

- `futex` 的 key 必须由 `task::FutexKey` 生成（不能自己拼地址）。
- `membarrier` 是内存屏障语义，不能省略。
- 新增同步 syscall 时同步 `syscall/mod.rs`。

## 3. 变更影响面

改本文件 → `syscall/sync/*.md`、`syscall/mod.rs.md`。
