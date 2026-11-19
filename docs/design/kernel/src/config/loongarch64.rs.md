# `kernel/src/config/loongarch64.rs`

> 对应源码：`kernel/src/config/loongarch64.rs`
> 所属：配置 / 架构常量（LoongArch64）
> 由 `config/mod.rs` 在 `target_arch = "loongarch64"` 时重导出

## 1. 文件职责

定义 LoongArch64 目标的用户态地址布局常量。当前竞赛主目标之一（`kernel-la`）。

## 2. 常量表

| 常量 | 值 | 含义 |
|---|---|---|
| `KERNEL_STACK_SIZE` | `0x4_0000` | 用户任务内核栈 |
| `USER_SPACE_BASE` | `0x1000` | 用户空间起始 |
| `USER_SPACE_SIZE` | `0x3f_ffff_f000` | 用户空间大小 |
| `USER_STACK_TOP` | `0x4_0000_0000` | 用户栈顶 |
| `USER_STACK_SIZE` | `0x8_0000` | 用户栈大小 |
| `USER_HEAP_BASE` | `0x4000_0000` | 堆起始 |
| `USER_HEAP_SIZE` | `0x1_0000`（注释：64KB） | 初始堆 |
| `USER_HEAP_SIZE_MAX` | `0x2000_0000`（注释：512MB） | brk 上限 |
| `USER_INTERP_BASE` | `0x400_0000` | 动态链接器基址 |
| `SIGNAL_TRAMPOLINE` | `0x6000_1000` | 信号蹦床 |

## 3. 与 riscv64 的差异

**数值完全相同**，只有两行带行尾注释（`// 64KB`、`// 512MB`）。设计上刻意保持两架构用户态布局一致，便于同一批用户程序/测试镜像复用。

## 4. 不变量与约束

- 与 `riscv64.rs` 保持同步（同一布局）；修改必须成对。
- `mm/loader.rs` 的 `copy_from_kernel` 对 LoongArch 是**空操作**（使用独立页表 PGDL），但本文件不感知这一点——不要在此加入页表相关常量。
- LoongArch 的 `USER_STACK_TOP = 0x4_0000_0000` 与其 `phys-virt-offset = 0xffff_8000_0000_0000` 无关；用户空间与内核空间完全分离。
- 其余不变量同 `riscv64.rs`（栈不撞堆、trampoline 在堆之上、解释器基址与 ELF 一致）。

## 5. 与其他模块的关系

同 `riscv64.rs`：被 `config/mod.rs` 重导出，由 `task/`、`mm/`、`syscall/mm/brk.rs` 使用。

## 6. 已知局限 / TODO

- 注释中的 "64KB/512MB" 是历史遗留说明；实际值 0x1_0000=64KiB、0x2000_0000=512MiB，与 RISC-V 一致。
- 未针对 LoongArch 的 `-softfloat` ABI 做任何特殊布局（不需要）。
