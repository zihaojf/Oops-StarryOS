# `kernel/src/syscall/task/ctl.rs`

> 对应源码：`kernel/src/syscall/task/ctl.rs`
> 所属子系统：进程控制类 syscall
> 依赖：`linux_raw_sys::general::{__user_cap_header_struct, __user_cap_data_struct}`、`linux_raw_sys::prctl::*`、`starry_vm::{vm_write_slice, VmMutPtr, VmPtr}`、`mm::vm_load_string`
> 被依赖：`syscall/task/mod.rs`、`syscall/mod.rs`

## 1. 文件职责

实现 `capget/capset/umask/setreuid/setresuid/setresgid/get_mempolicy/prctl`。

## 2. 关键函数

| 函数 | 行为 |
|---|---|
| `validate_cap_header` | 读版本字段；非 `CAPABILITY_VERSION_3`（`0x20080522`）→ 写回期望版本并 `InvalidInput`；然后 `get_process_data(pid)` 校验 pid |
| `sys_capget` | 校验后写 `{effective: MAX, permitted: MAX, inheritable: MAX}` |
| `sys_capset` | 只校验 header，忽略 data（无权限模型） |
| `sys_umask` | `replace_umask(mask)` 返回旧值 |
| `sys_setreuid/setresuid/setresgid` | 返回 0（忽略） |
| `sys_get_mempolicy` | `warn!` + 0 |
| `sys_prctl` | `PR_SET_NAME`：`vm_load_string` → `set_name`；`PR_GET_NAME`：15 字节 + NUL 写回；`PR_SET_SECCOMP/PR_MCE_KILL` 空操作；`PR_SET_MM` → `InvalidInput`；其他 `warn!` + `InvalidInput` |

## 3. 不变量与约束

- **C6**：`cap_header`/`cap_data`/`arg2` 都经 `starry_vm`。
- `CAPABILITY_VERSION_3` 的 `0x20080522` 必须精确；版本不匹配时 Linux 的约定是"写回支持的版本 + EINVAL"（当前实现一致）。
- `capget` 的 `data` 指针必须指向**两个** `__user_cap_data_struct`（`_LINUX_CAPABILITY_U32S_3 = 2`），当前只写第一个（**不完整**）。
- `PR_GET_NAME` 的 16 字节缓冲必须包含结尾 NUL（`buf[..len]` + `buf` 其余为 0）。
- `PR_SET_NAME` 用 `vm_load_string`（NUL 结尾），长度不限但 `TaskInner::name` 会截断。
- `sys_umask` 的 `mask` 不掩码 `0o777`（Linux 允许任意值，实际只取低 9 位；当前原样存）。
- `prctl` 未知 option 返回 `InvalidInput`（严格）。

## 4. 与其他模块的关系

| 模块 | 关系 |
|---|---|
| `task/mod.rs` | `umask`、`AsThread` |
| `mm/access.rs` | `vm_load_string`/`vm_write_slice` |
| `axtask` | `set_name/name` |
| `syscall/task/mod.rs` | 门面 |
| `syscall/mod.rs` | 分派 |

## 5. 已知局限 / TODO

- `capget` 只写一个 `__user_cap_data_struct`（应为 2 个 / 或按 version 决定）。
- `capset` 不生效（全 capability）。
- `PR_SET_SECCOMP/PR_MCE_KILL` 空操作；`PR_SET_MM` 拒绝。
- 无 `PR_SET_DUMPABLE/PR_SET_PDEATHSIG/PR_GET_PDEATHSIG/PR_SET_NO_NEW_PRIVS`。
- `get_mempolicy` 无条件返回 0。

## 6. 变更影响面

改本文件 → `task/mod.rs.md`、`mm/access.rs.md`、`syscall/task/mod.rs.md`、`PRINCIPLES.md` P8。
