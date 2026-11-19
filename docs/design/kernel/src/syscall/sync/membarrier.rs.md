# `kernel/src/syscall/sync/membarrier.rs`

> 对应源码：`kernel/src/syscall/sync/membarrier.rs`
> 所属子系统：内存屏障 syscall
> 依赖：`core::sync::atomic::{Ordering, compiler_fence}`

## 1. 文件职责

实现 `membarrier(cmd, flags, cpu_id)` 的**最小**语义：查询支持的命令 + 编译器屏障。

```rust
const MEMBARRIER_CMD_QUERY: i32 = 0;
const MEMBARRIER_CMD_GLOBAL: i32 = 1;
const MEMBARRIER_CMD_GLOBAL_EXPEDITED: i32 = 2;
const MEMBARRIER_CMD_REGISTER_GLOBAL_EXPEDITED: i32 = 3;
const MEMBARRIER_CMD_PRIVATE_EXPEDITED: i32 = 4;
const MEMBARRIER_CMD_REGISTER_PRIVATE_EXPEDITED: i32 = 5;
const SUPPORTED_COMMANDS: i32 = (1<<GLOBAL)|(1<<GLOBAL_EXPEDITED)|(1<<REGISTER_GLOBAL)|(1<<PRIVATE)|(1<<REGISTER_PRIVATE);

pub fn sys_membarrier(cmd, flags, _cpu_id) -> AxResult<isize> {
    if flags != 0 { return Err(InvalidInput) }
    match cmd {
        QUERY => Ok(SUPPORTED_COMMANDS),
        _ => { compiler_fence(SeqCst); Ok(0) }
    }
}
```

## 2. 设计要点

- **"支持"是声明的**：`QUERY` 返回位图告诉用户态支持哪些命令；当前声明支持 GLOBAL/GLOBAL_EXPEDITED/REGISTER_*/PRIVATE_EXPEDITED，但实际只做 `compiler_fence`（不发送 IPI）。
- 单核下 `compiler_fence(SeqCst)` 足够（没有其他核需要同步）；SMP 下不完整。
- `flags != 0` → `InvalidInput`（当前 Linux 要求 0）。
- 未识别的 cmd 也返回 0（宽松），不报 `EINVAL`——程序探测时不会失败。

## 3. 不变量与约束

- 单核假设：真正的跨核屏障不需要；若启用 `smp` feature，此实现**不满足语义**（需要 IPI 或其他核的屏障）。
- `SUPPORTED_COMMANDS` 的位定义必须与 Linux 头文件一致（`cmd` 是位序号，不是位掩码）。
- `QUERY` 的返回值是位掩码（bit N 表示 cmd N 支持）。
- 不能删掉 `compiler_fence`（否则编译器可能重排）。

## 4. 与其他模块的关系

| 模块 | 关系 |
|---|---|
| `syscall/mod.rs` | 分派 |
| `syscall/sync/mod.rs` | 门面 |
| `axtask`/`axhal` | 未来 SMP 需要 IPI 支持 |

## 5. 已知局限 / TODO

- 不真正跨核同步（单核可接受，SMP 不安全）。
- 未识别的 cmd 静默成功。
- 无 `MEMBARRIER_CMD_PRIVATE_EXPEDITED_SYNC_CORE`/`RSEQ` 等新命令。

## 6. 变更影响面

改本文件 → `syscall/sync/mod.rs.md`、`syscall/mod.rs.md`、`CONSTRAINTS.md`（若启用 SMP 需重新审视 P7）。
