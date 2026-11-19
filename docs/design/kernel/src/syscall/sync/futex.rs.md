# `kernel/src/syscall/sync/futex.rs`

> 对应源码：`kernel/src/syscall/sync/futex.rs`
> 所属子系统：futex syscall
> 依赖：`task::{FutexKey, futex_table_for, get_task, AsThread}`、`time::TimeValueLike`、`linux_raw_sys::general::{FUTEX_*, robust_list_head}`
> 被依赖：`syscall/sync/mod.rs`、`syscall/mod.rs`

## 1. 文件职责

实现 `futex(uaddr, op, val, timeout, uaddr2, val3)` 的 `WAIT/WAKE/REQUEUE/CMP_REQUEUE/WAIT_BITSET/WAKE_BITSET`，以及 `get_robust_list/set_robust_list`。

## 2. 关键流程

```text
key = FutexKey::new_current(uaddr.addr())
table = futex_table_for(&key)
cmd = futex_op & FUTEX_CMD_MASK
match cmd:
  WAIT | WAIT_BITSET:
    若 *uaddr != val → WouldBlock            // 快路径
    timeout = nullable → try_into_time_value
    futex = table.get_or_insert(&key)
    bitset = (cmd==WAIT_BITSET) ? val3 : u32::MAX
    if !futex.wq.wait_if(bitset, timeout, || *uaddr == val) → WouldBlock
    if futex.owner_dead.swap(false) → EOWNERDEAD
    else Ok(0)
  WAKE | WAKE_BITSET:
    futex = table.get(&key)（可能 None）
    bitset = ...; count = futex.wq.wake(val, bitset)
    yield_now(); Ok(count)
  REQUEUE | CMP_REQUEUE:
    value 必须非负
    CMP_REQUEUE 时 *uaddr != val3 → WouldBlock
    value2 = timeout 指针的低 32 位（当作 u32）
    futex = table.get(&key); key2 = new_current(uaddr2); table2 = ...; futex2 = table2.get_or_insert(&key2)
    count = futex.wq.wake(val, MAX)
    若 count == val → count += futex.wq.requeue(value2, &futex2.wq)
    Ok(count)
  _ → Unsupported
```

`get_robust_list(tid, head, size)`：`get_task(tid)` → 写 `robust_list_head` 指针与 `size_of::<robust_list_head>()`。
`set_robust_list(head, size)`：`size` 必须精确 → `current().as_thread().set_robust_list_head(head)`。

## 3. 约束与细节

- **C6**：`uaddr` 用 `vm_read` 读值（`starry_vm::VmPtr`）；`timeout` 是 `*const timespec`（nullable）。
- **C24**：`value` 用于 `WAKE` 时必须是合法数量？当前直接 `as usize`（负数会变成巨大值；`assert_unsigned` 只用于 REQUEUE）。
- `FUTEX_CMP_REQUEUE` 的 `value3` 是期望值；`timeout` 指针被复用为 `value2`（Linux 的丑陋 ABI：`val2` 通过指针传递）——`timeout.addr() as u32` 取低 32 位。
- `wait_if` 返回 `Ok(false)` → `WouldBlock`（值已变，无需等待）。
- `owner_dead` 只在 `WAIT` 返回后检查一次；robust list 置位由 `task/ops.rs` 完成。
- `WAKE` 后 `yield_now()`：让被唤醒任务有机会运行（单核）。
- `bitset`：`WAIT/WAKE` 用 `u32::MAX`（匹配所有），`*_BITSET` 用 `val3`。
- `FUTEX_WAKE` 的 `val` 是"最多唤醒数"，`as usize` 后传给 `wake(count, mask)`。
- 不支持 `FUTEX_FD`/`FUTEX_WAIT_REQUEUE_PI`/`FUTEX_LOCK_PI` 等（`Unsupported`）。

## 4. 与其他模块的关系

| 模块 | 关系 |
|---|---|
| `task/futex.rs` | `FutexKey/FutexTable/WaitQueue` |
| `task/ops.rs` | robust list 的 `owner_dead` 置位与唤醒 |
| `syscall/sync/mod.rs` | 门面 |
| `syscall/mod.rs` | 分派 |
| `mm/aspace/backend/{shared,file}.rs` | `FutexKey::Shared` 的身份 |
| `time.rs` | 超时转换 |

## 5. 已知局限 / TODO

- 无 PI futex（优先级继承）。
- `value` 的负数检查不完整（`WAKE` 直接 `as usize`）。
- `FUTEX_REQUEUE` 的 `value2` 用指针地址的低 32 位（依赖 ABI 约定，非类型安全）。
- 无 `FUTEX_WAKE_OP`。
- `get_robust_list` 未校验 `size` 指针可写（依赖 `vm_write`）。

## 6. 变更影响面

改本文件 → `task/futex.rs.md`、`task/ops.rs.md`、`syscall/sync/mod.rs.md`、`time.rs.md`、`ARCHITECTURE.md#11`、`CONSTRAINTS.md` C24。
