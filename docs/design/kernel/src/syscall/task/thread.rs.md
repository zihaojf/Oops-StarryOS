# `kernel/src/syscall/task/thread.rs`

> 对应源码：`kernel/src/syscall/task/thread.rs`
> 所属子系统：线程信息/线程本地存储
> 依赖：`axtask::current`、`task::AsThread`、`starry_vm::VmMutPtr`
> 被依赖：`syscall/task/mod.rs`、`syscall/mod.rs`

## 1. 文件职责

实现 `getpid/getppid/gettid/set_tid_address` 与 `arch_prctl`（仅 x86_64）。

## 2. 各函数

| syscall | 行为 |
|---|---|
| `getpid` | `proc.pid()`（线程组 id） |
| `getppid` | `proc.parent().ok_or(NoSuchProcess).pid()` |
| `gettid` | `current().id()`（内核任务 id） |
| `set_tid_address(ptr)` | `set_clear_child_tid(ptr)`；返回当前 tid（总是成功） |
| `arch_prctl(code, addr)` | 见下 |

### `arch_prctl`（x86_64）

`ArchPrctlCode`：`SetGs(0x1001)/SetFs(0x1002)/GetFs(0x1003)/GetGs(0x1004)/GetCpuid(0x1011)/SetCpuid(0x1012)`。

| code | 行为 |
|---|---|
| `GetFs` | 写 `uctx.tls()` |
| `SetFs` | `uctx.set_tls(addr)` |
| `GetGs` | 写 `uctx.gs_base` |
| `SetGs` | `uctx.gs_base = addr` |
| `GetCpuid` | 返回 0（不写） |
| `SetCpuid` | `NoSuchDevice`（表示不支持） |

## 3. 不变量与约束

- **C6**：`set_tid_address` 的指针在任务退出时由 `do_exit` 使用（`vm_write(0)`）；`arch_prctl` 的 `addr` 是输出指针时用 `vm_write`。
- `set_tid_address` 总是成功（Linux 语义），返回 tid。
- `getppid` 对 init（无父）返回 `NoSuchProcess`——Linux 返回 0；**已知差异**（init 通常不会调）。
- `arch_prctl` 只在 x86_64 编译；`SetFs` 同时更新 `UserContext.tls`（后续 `clone(CLONE_SETTLS)` 也用它）。
- `ArchPrctlCode::try_from` 失败 → `InvalidInput`。
- `GetCpuid` 的返回 0 表示"未禁用 cpuid"（Linux 语义：返回 0 表示允许）。

## 4. 与其他模块的关系

| 模块 | 关系 |
|---|---|
| `task/mod.rs` | `proc_data.proc`、`clear_child_tid` |
| `axhal::uspace::UserContext` | `tls()/set_tls/gs_base` |
| `syscall/task/clone.rs` | `CLONE_SETTLS` 设置 `uctx.tls` |
| `task/ops.rs` | `do_exit` 清 `clear_child_tid` |
| `syscall/task/mod.rs` | 门面 |

## 5. 已知局限 / TODO

- `getppid` 对 init 返回错误（Linux 返回 0）。
- `arch_prctl` 的 `GetCpuid/SetCpuid` 不真正控制 CPUID。
- 无 `PR_SET_TSC`/`arch_prctl` 其他 code。
- `getpid` 与 `gettid` 的区分依赖 `Process`/`TaskInner` 一致性。

## 6. 变更影响面

改本文件 → `task/mod.rs.md`、`syscall/task/clone.rs.md`、`task/ops.rs.md`、`syscall/task/mod.rs.md`、`CONSTRAINTS.md` C6。
