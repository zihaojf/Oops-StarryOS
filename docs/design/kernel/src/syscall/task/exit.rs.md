# `kernel/src/syscall/task/exit.rs`

> 对应源码：`kernel/src/syscall/task/exit.rs`
> 所属子系统：进程/线程退出 syscall
> 依赖：`task::do_exit`
> 被依赖：`syscall/task/mod.rs`、`syscall/mod.rs`

## 1. 文件职责

`exit` 与 `exit_group` 的薄封装：

```rust
pub fn sys_exit(exit_code: i32) -> AxResult<isize> {
    do_exit(exit_code << 8, false);
    Ok(0)
}
pub fn sys_exit_group(exit_code: i32) -> AxResult<isize> {
    do_exit(exit_code << 8, true);
    Ok(0)
}
```

## 2. 关键语义

- **`exit_code << 8`**：Linux 的 `wait` 状态是 `(exit_code & 0xff) << 8`；这里把用户传入的 code 左移 8 位后存为"原始状态"，`wait4` 返回给父进程时直接是 wait status（低 8 位是信号编号 0）。
- `sys_exit` 只退当前线程（`group_exit=false`）；`sys_exit_group` 退整个线程组（`group_exit=true`）并杀其他线程。
- `do_exit` 之后不会返回用户态（`thr.set_exit()` 让主循环退出）。

## 3. 不变量与约束

- **C16**：必须走 `do_exit`；不要在这里自行清理。
- `exit_code` 是 `i32`，左移 8 位可能溢出（`i32` 高位丢失；Linux 只取低 8 位，行为一致）。
- `Ok(0)` 不可达（`do_exit` 后任务被标记退出，`handle_syscall` 设置返回值后 `user.rs` 检查 `pending_exit` 退出循环），但保留以匹配 `AxResult` 签名。
- `exit_group` 的"杀其他线程"由 `do_exit` 实现（发送 `SIGKILL`）。

## 4. 与其他模块的关系

| 模块 | 关系 |
|---|---|
| `task/ops.rs` | `do_exit` |
| `task/user.rs` | 主循环 `pending_exit` |
| `syscall/task/wait.rs` | `exit_code` 的消费方 |
| `task/stat.rs` | `/proc/<pid>/stat` 的 `exit_code` |
| `syscall/task/mod.rs` | 门面 |

## 5. 已知局限 / TODO

- `do_exit` 不返回，`Ok(0)` 只是形式。
- `exit` 的 `exit_code` 低 8 位之外的位被保留在 `exit_code` 字段（`wait` 返回时未掩码，可能带高位）。

## 6. 变更影响面

改本文件 → `task/ops.rs.md`、`syscall/task/wait.rs.md`、`task/stat.rs.md`、`CONSTRAINTS.md` C16。
