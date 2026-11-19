# `kernel/src/mm/mod.rs`

> 对应源码：`kernel/src/mm/mod.rs`
> 所属子系统：内存管理（模块门面）
> 依赖：子模块 `access`/`aspace`/`io`/`loader`

## 1. 文件职责

纯门面文件：声明并重导出四个子模块：

```rust
mod access;    // 用户内存访问、UserPtr/VmBytes、PAGE_FAULT handler
mod aspace;    // AddrSpace + Backend
mod io;        // IoVec/IoVectorBuf（分散聚集 I/O）
mod loader;    // ELF 装载、栈/堆/trampoline 映射

pub use self::{access::*, aspace::*, io::*, loader::*};
```

- **不变量/流程/类型**全部在子模块文档中说明；本文件只决定"mm 命名空间对外暴露什么"。
- 重导出使业务代码统一写 `crate::mm::{AddrSpace, Backend, UserPtr, VmBytes, load_user_app, ...}`。

## 2. 对外符号清单（按子模块）

| 子模块 | 主要导出 |
|---|---|
| `access` | `access_user_memory`、`UserPtr<T>`、`UserConstPtr<T>`、`VmBytes`、`VmBytesMut`、`check_access`、`vm_load_string`、`nullable!`、`PAGE_FAULT` handler |
| `aspace` | `AddrSpace`、`Backend`、`BackendOps`、`SharedPages`、`FileBackend`、`CowBackend`、`LinearBackend`、`SharedBackend` |
| `io` | `IoVec`、`IoVectorBuf`、`IoVectorBufIo` |
| `loader` | `new_user_aspace_empty`、`copy_from_kernel`、`load_user_app`、`map_trampoline`、`clear_elf_cache` |

## 3. 与其他模块的关系

- `file/mod.rs` 的 `IoDst/IoSrc` 由 `mm::access` 的 `VmBytes/VmBytesMut` 与 `mm::io` 的 `IoVectorBuf` 实现。
- `syscall/mm/*` 使用 `AddrSpace`/`Backend`。
- `entry.rs` 使用 `new_user_aspace_empty`/`copy_from_kernel`/`load_user_app`。
- `syscall/task/{clone,execve}.rs` 使用 `load_user_app`/`copy_from_kernel`。

## 4. 已知局限 / TODO

- 无独立"物理页管理"模块：物理页由 `axalloc`（`alloc_frame` 包装）与 `SharedPages` 管理。
- 没有 swap、没有 page reclaim（仅文件页缓存有驱逐）。

## 5. 变更影响面

新增 mm 子模块 → 本文件 + 新子模块文档 + `ARCHITECTURE.md#5`。
