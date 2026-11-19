# `kernel/src/config/aarch64.rs`

> 对应源码：`kernel/src/config/aarch64.rs`
> 所属：配置 / 架构常量（AArch64）
> 状态：编译支持，不参与竞赛构建

## 1. 文件职责

AArch64 的用户态地址布局。与 x86_64 相同（47 位用户空间），因为二者都是高位内核 + 低位用户。

## 2. 常量表

| 常量 | 值 |
|---|---|
| `KERNEL_STACK_SIZE` | `0x4_0000` |
| `USER_SPACE_BASE` | `0x1000` |
| `USER_SPACE_SIZE` | `0x7fff_ffff_f000` |
| `USER_STACK_TOP` | `0x7fff_0000_0000` |
| `USER_STACK_SIZE` | `0x8_0000` |
| `USER_HEAP_BASE` | `0x4000_0000` |
| `USER_HEAP_SIZE` | `0x1_0000` |
| `USER_HEAP_SIZE_MAX` | `0x2000_0000` |
| `USER_INTERP_BASE` | `0x400_0000` |
| `SIGNAL_TRAMPOLINE` | `0x6000_1000` |

## 3. 不变量与约束

- **aarch64 使用独立用户页表（TTBR0_EL1）**：`mm/loader.rs::copy_from_kernel` 对 aarch64/loongarch64 是空操作。本文件不体现这一点，但改布局时不要假设内核页被复制进用户表。
- `USER_STACK_TOP` 必须在用户空间内（`< 0x8000_0000_0000`），且 4K 对齐。
- 与 x86_64 的数值完全一致；改了要三处（x86_64/aarch64）同步检查。
- `USER_INTERP_BASE` 必须与 aarch64 动态链接器链接地址匹配（若有）。

## 4. 已知局限 / TODO

- 未在竞赛流程验证（README 未勾选）。
- 未考虑 52 位 VA（ARMv8.2-LVA）。
- `KERNEL_STACK_SIZE = 256KiB` 对 aarch64 的 16KiB 页/栈对齐未做特殊处理。
