# `kernel/src/lib.rs`

> 对应源码：`kernel/src/lib.rs`（crate `starry-kernel`）
> 所属：内核 crate 根
> 依赖：`extern crate axruntime`、`axlog`（宏）、`alloc`

## 1. 文件职责

定义内核 crate 的编译属性与模块树。只有 24 行，但是**所有模块的可见性边界**。

## 2. 内容

```rust
#![no_std]                 // C1：无 std
#![feature(likely_unlikely)] // 热路径分支提示（std::hint::{likely,unlikely}）
#![feature(bstr)]          // core::bstr::ByteStr（dev/log 用）
#![allow(missing_docs)]
#![allow(clippy::not_unsafe_ptr_arg_deref)]

extern crate alloc;        // 必须最早，供所有模块用 alloc
extern crate axruntime;    // 链接运行时（启动/关机/调度器初始化）

#[macro_use]
extern crate axlog;        // 导出 info!/warn!/debug!/trace!/error!

pub mod entry;             // 公开：供顶层 crate 调用 init()

mod config;                // 私有：架构常量
mod file;                  // 私有：fd 与 VFS 包装
mod mm;                    // 私有：地址空间/用户内存访问
mod pseudofs;              // 私有：/dev /proc /tmp /sys
mod syscall;               // 私有：系统调用分派
mod task;                  // 私有：任务/线程/进程数据
mod time;                  // 私有：Linux 时间结构转换
```

## 3. 设计意图

- **最小公开面**：只有 `entry` 是 `pub`，顶层 crate 无法直接触摸内核内部；测试/调试通过 syscall 或 `/proc` 完成。
- **`axruntime` extern crate**：即使没有直接符号引用，也必须链接，否则启动代码不会被包含。删掉它会导致镜像没有入口。
- **`#[macro_use] extern crate axlog`**：使日志宏在子模块中无需 `use`。
- **feature 门控在子模块**：`input/dev-log/memtrack/vsock` 通过各自 `#[cfg(feature=...)]`，不在本文件展开。
- `#![allow(missing_docs)]` 是现实妥协（大量未加 doc 的内部项）；新代码仍鼓励写 doc。

## 4. 不变量与约束

- **模块声明顺序**：`alloc` 必须在任何使用 `alloc::` 的模块之前；`axlog` 的 `#[macro_use]` 必须在模块声明之前（宏在全 crate 生效）。
- 不要在这里加 `pub use` 重导出内部模块（会破坏最小公开面）；需要跨模块使用请在同一 crate 内 `use crate::...`。
- 新增顶层模块时必须：创建 `docs/design/kernel/src/<模块>.md` 与 `docs/design/kernel/src/<模块>/mod.rs.md`，并更新 `ARCHITECTURE.md` 分层图。
- `#![feature(...)]` 只能用于当前 nightly 已稳定的（见 C3）。

## 5. 与其他模块的关系

- `src/main.rs` → `starry_kernel::entry::init`。
- `entry.rs` 是唯一对外入口；它依赖 `file::FD_TABLE`、`mm::{copy_from_kernel, load_user_app, new_user_aspace_empty}`、`pseudofs::{mount_all, dev::tty::N_TTY}`、`task::{...}`、`time`。

## 6. 已知局限 / TODO

- `missing_docs` 被全局允许；若将来启用，需要补大量文档。
- `#![feature(bstr)]` 只服务 `dev/log`，关闭 `dev-log` 时仍是编译期依赖。

## 7. 变更影响面

新增/删除模块 → 本文件 + 逐文件文档 + `ARCHITECTURE.md` + `TRACEABILITY.md`。
