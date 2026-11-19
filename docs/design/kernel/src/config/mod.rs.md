# `kernel/src/config/mod.rs`

> 对应源码：`kernel/src/config/mod.rs`
> 所属：配置 / 架构常量
> 依赖：四个架构文件
> 被依赖：`task/mod.rs`、`mm/loader.rs`、`syscall/mm/brk.rs`、`syscall/task/*`、`mm/access.rs` 等

## 1. 文件职责

按目标架构选择并**重导出**一套常量。使用 `cfg_if!` 在编译期互斥选择：

```rust
cfg_if::cfg_if! {
    if #[cfg(target_arch = "riscv64")]     { mod riscv64;     pub use riscv64::*; }
    else if #[cfg(target_arch = "loongarch64")] { mod loongarch64; pub use loongarch64::*; }
    else if #[cfg(target_arch = "x86_64")]      { mod x86_64;      pub use x86_64::*; }
    else if #[cfg(target_arch = "aarch64")]     { mod aarch64;     pub use aarch64::*; }
    else { compile_error!("Unsupported architecture"); }
}
```

`compile_error!` 保证不支持的架构在编译期失败，而不是产生难以理解的错误。

## 2. 导出的常量（4 个架构共同）

| 常量 | 含义 | 使用者 |
|---|---|---|
| `KERNEL_STACK_SIZE` | 每个用户任务的内核栈大小（`0x4_0000` = 256KiB） | `task/user.rs::new_user_task` |
| `USER_SPACE_BASE` | 用户空间起始 | `mm/loader.rs`、`mm/access.rs::check_access` |
| `USER_SPACE_SIZE` | 用户空间大小 | 同上 |
| `USER_STACK_TOP` | 用户栈顶（栈向下增长） | `mm/loader.rs` |
| `USER_STACK_SIZE` | 用户栈大小（`0x8_0000` = 512KiB） | `mm/loader.rs`、`task/resources.rs`（RLIMIT_STACK） |
| `USER_HEAP_BASE` | 用户堆起始 | `mm/loader.rs`、`syscall/mm/brk.rs`、`task/mod.rs` |
| `USER_HEAP_SIZE` | 初始堆大小（64KiB） | `mm/loader.rs`、`syscall/mm/brk.rs` |
| `USER_HEAP_SIZE_MAX` | brk 最大扩张 | `syscall/mm/brk.rs` |
| `USER_INTERP_BASE` | 动态链接器装载基址 | `mm/loader.rs` |
| `SIGNAL_TRAMPOLINE` | 信号返回蹦床虚拟地址（`0x6000_1000`） | `task/mod.rs`（ProcessSignalManager）、`mm/loader.rs` |

## 3. 架构差异

| 常量 | rv64 | la64 | x86_64 | aarch64 |
|---|---|---|---|---|
| `USER_SPACE_SIZE` | `0x3f_ffff_f000` | `0x3f_ffff_f000` | `0x7fff_ffff_f000` | `0x7fff_ffff_f000` |
| `USER_STACK_TOP` | `0x4_0000_0000` | `0x4_0000_0000` | `0x7fff_0000_0000` | `0x7fff_0000_0000` |
| `USER_STACK_SIZE` | `0x8_0000` | `0x8_0000` | `0x8_0000` | `0x8_0000` |
| 其余 | 相同 | 相同 | 相同 | 相同 |

设计意图：RISC-V/LoongArch 用 38 位用户地址空间（`0x0`–`0x40_0000_0000`），x86_64/aarch64 用 47 位（`0x0`–`0x8000_0000_0000`）。栈顶低于空间上限，堆与 trampoline 在低 2GB 内。

## 4. 不变量与约束

- **`USER_HEAP_BASE + USER_HEAP_SIZE_MAX` 必须 < `USER_STACK_TOP - USER_STACK_SIZE`**（否则堆会撞栈）。当前：`0x4000_0000 + 0x2000_0000 = 0x6000_0000 < 0x3_fff8_0000`。
- **`SIGNAL_TRAMPOLINE` 必须落在用户空间内且不与堆/栈/解释器重叠**。当前 `0x6000_1000` 恰在堆上限之上、栈之下。
- 四个架构文件必须导出**同名同类型**的常量集合；新增常量要四份都加（或提供 `cfg` 默认值）。
- `USER_INTERP_BASE` 必须与 ELF 解释器（ld.so）的链接基址匹配；改动会导致动态链接程序崩溃。
- `KERNEL_STACK_SIZE` 不能小于最深的 syscall 调用栈（当前 256KiB 足够，但 `execve`/`mmap` 递归深）。

## 5. 与其他模块的关系

- 上游：`axconfig`（平台配置）不直接参与这些常量——它们是 StarryOS 自己的用户态布局常量，与平台无关。
- 下游：`mm/loader.rs`（映射栈/堆/trampoline）、`mm/access.rs`（`check_access` 边界）、`syscall/mm/brk.rs`（堆扩张）、`task/resources.rs`（RLIMIT_STACK 初值）、`task/mod.rs`（信号 trampoline 地址）。

## 6. 已知局限 / TODO

- x86_64/aarch64 的用户布局未在竞赛流程验证（仅编译）。
- `USER_HEAP_SIZE` 与 `USER_HEAP_SIZE_MAX` 是硬编码，未与 `RLIMIT_AS`/内存大小联动。
- `SIGNAL_TRAMPOLINE` 注释说 "placed at top of user heap"，但实际高于堆上限，注释略不准确。

## 7. 变更影响面

改本文件或任一架构文件 → 四份架构文档、`mm/loader.rs.md`、`syscall/mm/mmap.rs.md`、`syscall/mm/brk.rs.md`、`CONSTRAINTS.md`（若改变不变量）。
