# `kernel/src/syscall/sys.rs`

> 对应源码：`kernel/src/syscall/sys.rs`
> 所属子系统：系统信息类 syscall
> 依赖：`axconfig::ARCH`、`axfs::FS_CONTEXT`、`linux_raw_sys::system::{new_utsname, sysinfo}`、`starry_vm`
> 被依赖：`syscall/mod.rs`

## 1. 文件职责

实现"系统级"syscall：UID/GID、`uname`、`sysinfo`、`syslog`、`getrandom`、`seccomp`、`reboot`、`riscv_flush_icache`、以及 `get_groups/set_groups`。

## 2. 各函数行为

| 函数 | 行为 |
|---|---|
| `sys_getuid/geteuid/getgid/getegid` | 恒 0（单用户 root 语义） |
| `sys_setuid/setgid` | 忽略参数，返回 0（debug 日志） |
| `sys_getgroups(size, list)` | `size < 1` → `InvalidInput`；否则写 `[0]` 返回 1 |
| `sys_setgroups` | 返回 0（忽略） |
| `sys_uname(name)` | 写固定 `UTSNAME`：sysname="Linux"、nodename="starry"、release/version="10.0.0"、machine=`ARCH`、domainname=仓库地址 |
| `sys_sysinfo(info)` | 填 `procs = processes().len()`、`mem_unit = 1`，其余 0（`unsafe zeroed`） |
| `sys_syslog` | 返回 0（不写） |
| `sys_getrandom(buf,len,flags)` | `len==0` → 0；按 `GRND_RANDOM` 选 `/dev/random` 或 `/dev/urandom`；`read_at` 后 `vm_write_slice` |
| `sys_seccomp` | `warn!("dummy sys_seccomp")` + 0 |
| `sys_reboot(magic1,magic2,cmd,arg)` | 所有命令都 `axhal::power::system_off()` |
| `sys_riscv_flush_icache` | `riscv::asm::fence_i()`（仅 riscv64） |

## 3. 关键设计点

- **`UTSNAME` 是 `const`**：`pad_str` 用 `const fn` + `ptr::copy_nonoverlapping` 生成 65 字节定长字段（`new_utsname` 要求 `[c_char; 65]`）。
- **`getrandom` 走伪设备**：不实现 CSPRNG，直接读 `/dev/random`/`/dev/urandom`（`SmallRng`，固定种子）→ **非密码学安全**。
- **`reboot` 直接关机**：评测环境里 `poweroff/reboot/halt` 都等价于 `system_off`（QEMU 下结束）。
- **`sysinfo` 的 `unsafe zeroed()`**：`sysinfo` 结构未实现 `Zeroable`（FIXME 注释）。

## 4. 不变量与约束

- **C6**：`buf`/`name`/`info` 必须经 `vm_write_slice`/`vm_write`，不能裸写。
- `sys_uname` 的 `machine` 必须是**内核编译架构**（`axconfig::ARCH`），不是用户程序架构；错误会导致 `uname -m` 与测试脚本的架构分支不一致（`src/init.sh` 依赖 `uname -m` 判断 loongarch64）。
- `sys_reboot` 不校验 `magic`：任何参数都会关机（安全上可接受，因为只有 root 能调，而本系统全 root）。
- `sys_getrandom` 的 `flags` 未知位被忽略（`from_bits_retain`）。
- `sys_seccomp` 必须返回 0（程序探测 seccomp 支持）；返回错误会让某些程序禁用自身功能。
- `sys_syslog` 返回 0 但不写缓冲：调用方可能读到未初始化内容？——当前直接返回 0，不写，符合"长度 0"的语义（安全）。

## 5. 与其他模块的关系

| 模块 | 关系 |
|---|---|
| `syscall/mod.rs` | 分派 |
| `pseudofs/dev/mod.rs` | `/dev/random`、`/dev/urandom` |
| `task/ops.rs` | `processes()` |
| `axhal::power` | `system_off` |
| `src/init.sh` | `uname -m` 的架构判断 |
| `linux_raw_sys::system` | `new_utsname`/`sysinfo` 布局 |

## 6. 已知局限 / TODO

- UID/GID 全 0，无权限模型。
- `getrandom` 非密码学安全（固定种子）。
- `sysinfo` 只填 `procs/mem_unit`，内存信息全 0。
- `sys_syslog` 不实现 `SYSLOG_ACTION_READ` 等。
- `sys_reboot` 不区分 poweroff/reboot/halt（都关机）。
- `seccomp` 是空实现。

## 7. 变更影响面

改本文件 → `syscall/mod.rs.md`、`pseudofs/dev/mod.rs.md`、`task/ops.rs.md`、`src/init.sh.md`、`PRINCIPLES.md` P8。
