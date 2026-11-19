# `kernel/src/syscall/task/clone3.rs`

> 对应源码：`kernel/src/syscall/task/clone3.rs`
> 所属子系统：clone3 syscall
> 依赖：`clone::{CloneArgs, CloneFlags}`、`bytemuck`、`starry_vm::vm_read_slice`
> 被依赖：`syscall/task/mod.rs`、`syscall/mod.rs`

## 1. 文件职责

实现 `clone3(args_ptr, size)`：从用户读取 `Clone3Args`，转成 `CloneArgs` 并调用统一的 `do_clone`。

## 2. 关键类型

```rust
#[repr(C)] #[derive(AnyBitPattern)] pub struct Clone3Args {
    flags, pidfd, child_tid, parent_tid, exit_signal, stack, stack_size, tls, set_tid, set_tid_size, cgroup: u64,
}
const MIN_CLONE_ARGS_SIZE: usize = 8 * 8;      // 前 8 个 u64 必须存在
```

## 3. `TryFrom<Clone3Args> for CloneArgs`

```text
set_tid/set_tid_size != 0 → warn（不支持，忽略）
cgroup != 0 → warn（忽略）
flags = from_bits_truncate
exit_signal > 0 且 (THREAD|PARENT) → InvalidInput
DETACHED → InvalidInput
stack = stack > 0 ? (stack_size > 0 ? stack+stack_size : stack) : 0
Ok(CloneArgs{..., pidfd})
```

## 4. `sys_clone3`

```text
size < MIN_CLONE_ARGS_SIZE → InvalidInput（warn）
size > size_of::<Clone3Args>() → debug（只用已知字段）
buffer = [0u8; size_of::<Clone3Args>()]
vm_read_slice(args, &mut buffer[..size])       // 只读 size 字节
clone3_args = try_pod_read_unaligned(&buffer)
CloneArgs::try_from(clone3_args)?.do_clone(uctx)
```

## 5. 不变量与约束

- **C6**：用户结构必须经 `vm_read_slice` + `bytemuck`（不能裸读，不能依赖对齐）。
- **C24**：`size` 必须 >= 64（前 8 字段）；大于结构大小时只用已知字段（前向兼容）。
- `stack + stack_size` 可能溢出 `u64`（未检查；调用方正常不会传）。
- `set_tid` 不支持（Linux 的 PID namespace 分配）→ 忽略（warn）。
- `clone3` 的 `exit_signal` 是独立字段（不像 `sys_clone` 放在 flags 低位）。
- `CloneArgs::do_clone` 的约束与 `clone` 共用（C19）。

## 6. 与其他模块的关系

| 模块 | 关系 |
|---|---|
| `syscall/task/clone.rs` | `CloneArgs/CloneFlags/do_clone` |
| `mm/access.rs` | `vm_read_slice` |
| `syscall/task/mod.rs` | 门面 |
| `syscall/mod.rs` | 分派 |

## 7. 已知局限 / TODO

- `set_tid`/`cgroup` 忽略。
- `stack` 溢出未检查。
- 不支持 `CLONE_INTO_CGROUP`（忽略）。
- `size` 小于结构时剩余字段为 0（正确，但 `exit_signal` 可能意外为 0 → 无退出信号）。

## 8. 变更影响面

改本文件 → `syscall/task/clone.rs.md`、`syscall/task/mod.rs.md`、`CONSTRAINTS.md` C6/C24。
