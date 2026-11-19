# `kernel/src/config/riscv64.rs`

> 对应源码：`kernel/src/config/riscv64.rs`
> 所属：配置 / 架构常量（RISC-V 64）
> 由 `config/mod.rs` 在 `target_arch = "riscv64"` 时重导出

## 1. 文件职责

定义 RISC-V 64 位目标的用户态地址布局常量。当前竞赛主目标之一（`kernel-rv`）。

## 2. 常量表

| 常量 | 值 | 含义 |
|---|---|---|
| `KERNEL_STACK_SIZE` | `0x4_0000`（256KiB） | 用户任务内核栈 |
| `USER_SPACE_BASE` | `0x1000` | 用户空间起始（跳过零页） |
| `USER_SPACE_SIZE` | `0x3f_ffff_f000` | 用户空间大小（≈256GiB，SV39/SV48 由 axhal 决定） |
| `USER_STACK_TOP` | `0x4_0000_0000` | 用户栈顶 |
| `USER_STACK_SIZE` | `0x8_0000`（512KiB） | 用户栈大小 |
| `USER_HEAP_BASE` | `0x4000_0000`（1GiB） | 堆起始 |
| `USER_HEAP_SIZE` | `0x1_0000`（64KiB） | 初始堆 |
| `USER_HEAP_SIZE_MAX` | `0x2000_0000`（512MiB） | brk 上限 |
| `USER_INTERP_BASE` | `0x400_0000`（64MiB） | 动态链接器基址 |
| `SIGNAL_TRAMPOLINE` | `0x6000_1000` | 信号蹦床（一页，RX） |

## 3. 布局图

```text
0x0000_1000  USER_SPACE_BASE
0x0400_0000  USER_INTERP_BASE   (ld-linux / ld-musl)
0x4000_0000  USER_HEAP_BASE     ┐
0x4000_0000 + 0x1_0000          │ 初始堆（ELF 装载时映射）
0x6000_1000  SIGNAL_TRAMPOLINE  │
0x6000_0000  堆上限 = HEAP_BASE+HEAP_SIZE_MAX（512MiB）
0x3_FFF8_0000 栈底 = STACK_TOP - STACK_SIZE
0x4_0000_0000 USER_STACK_TOP
0x40_0000_0000 用户空间结束
```

## 4. 不变量与约束

- 与 `loongarch64.rs` **必须完全一致**（除注释）；两处改动要同步。
- `USER_STACK_TOP` 必须 4K 对齐，且 `USER_STACK_TOP - USER_STACK_SIZE` 不能低于 `USER_HEAP_BASE + USER_HEAP_SIZE_MAX`。
- `SIGNAL_TRAMPOLINE` 在 `USER_HEAP_BASE` 之上 2GiB 附近，必须在用户空间内且不落在堆可扩张区间（`0x6000_1000 > 0x6000_0000` 堆上限，安全）。
- 修改 `USER_INTERP_BASE` 会破坏动态链接（解释器按此地址装载，ELF 里记录的入口依赖它）。

## 5. 与其他模块的关系

- 被 `config/mod.rs` 重导出。
- 使用点：`task/user.rs`（内核栈）、`task/mod.rs`（trampoline）、`mm/loader.rs`（栈/堆/trampoline）、`mm/access.rs`（边界）、`syscall/mm/brk.rs`（堆扩张上限）、`task/resources.rs`（RLIMIT_STACK）。

## 6. 已知局限 / TODO

- 未根据实际物理内存大小动态调整 `USER_HEAP_SIZE_MAX`。
- RISC-V 的 `USER_SPACE_SIZE` 与 `USER_STACK_TOP` 组合使栈落在 2GiB 以内，未使用完整的 256GiB 空间（`mmap` 从低地址找空闲区）。
