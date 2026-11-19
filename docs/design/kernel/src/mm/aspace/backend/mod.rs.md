# `kernel/src/mm/aspace/backend/mod.rs`

> 对应源码：`kernel/src/mm/aspace/backend/mod.rs`
> 所属子系统：内存映射后端（抽象层）
> 依赖：`axalloc::global_allocator`、`axhal::paging::{PageTableCursor, PageSize}`、`memory_set::MappingBackend`、`enum_dispatch`
> 被依赖：`mm/aspace/mod.rs`、`mm/loader.rs`、`syscall/mm/mmap.rs`、`syscall/ipc/shm.rs`、`mm/access.rs`

## 1. 文件职责

1. 定义 `BackendOps` trait（后端统一接口）；
2. 定义 `Backend` 枚举：`Linear / Cow / Shared / File`，用 `enum_dispatch` 静态分派；
3. 提供页级辅助：`alloc_frame`/`dealloc_frame`/`divide_page`/`pages_in`；
4. 实现 `memory_set::MappingBackend`，使 `Backend` 能放进 `MemorySet<Backend>`。

## 2. 关键类型

```rust
pub trait BackendOps {
    fn page_size(&self) -> PageSize;
    fn map(&self, range, flags, pt: &mut PageTableCursor) -> AxResult;
    fn unmap(&self, range, pt) -> AxResult;
    fn on_protect(&self, range, new_flags, pt) -> AxResult { Ok(()) }     // 默认无操作
    fn populate(&self, range, flags, access_flags, pt) -> AxResult<(usize, Option<PopulateCallback>)> { Ok((0,None)) }
    fn clone_map(&self, range, flags, old_pt, new_pt, new_aspace) -> AxResult<Backend>;
}

type PopulateCallback = Box<dyn FnOnce(&mut AddrSpace)>;   // 释放页表锁后执行

#[derive(Clone)] #[enum_dispatch(BackendOps)]
pub enum Backend { Linear(LinearBackend), Cow(CowBackend), Shared(SharedBackend), File(FileBackend) }
```

## 3. 辅助函数

| 函数 | 语义 |
|---|---|
| `divide_page(size, page_size)` | 断言对齐后右移得页数 |
| `alloc_frame(zeroed, size)` | 从 `axalloc` 分配 `num_pages` 个 4K 页（`UsageKind::VirtMem`），可选清零，返回物理地址 |
| `dealloc_frame(frame, align)` | `phys_to_virt` 后按页数归还 |
| `pages_in(range, align)` | `DynPageIter`，区间/对齐非法 → `InvalidInput` |

## 4. `MappingBackend for Backend`

| 回调 | 行为 |
|---|---|
| `map(start,size,flags,pt)` | `BackendOps::map`；失败只 `warn!` 并返回 false（**注意：错误被吞掉**） |
| `unmap(start,size,pt)` | 同上 |
| `protect(start,size,new_flags,pt)` | 先 `on_protect`（File 的权限校验），再 `cursor.protect_region` |

`MemorySet` 用布尔返回值决定是否回滚，因此 `warn!` + false 会触发 `MemorySet` 的清理逻辑。

## 5. 不变量与约束

- **`populate` 的返回值 `(n, callback)`**：`n` 是"本次满足 `access_flags` 的页数"；`callback` 必须在调用方释放 `aspace` 锁后执行（当前 `AddrSpace::handle_page_fault` 是 `populate` 返回后立即调用，仍持有锁——**FileBackend 的回调会再次取 aspace 锁吗？** 不会：`on_evict` 接收 `&mut AddrSpace`，不取锁。因此当前安全）。
- `alloc_frame` 使用 `UsageKind::VirtMem` 统计；改用途需同步 memtrack 分类。
- `MappingBackend::map/unmap` 的布尔接口丢错误信息：调用方只能从 `warn!` 日志看到原因；新增后端应尽量在 `BackendOps` 返回具体错误，再由上层记录。
- `Backend` 必须 `Clone`（`MemorySet` 分裂 VMA 时按需克隆）。
- `pages_in` 要求 `range` 按 `align` 对齐（`DynPageIter::new` 返回 None 时转 `InvalidInput`）。

## 6. 与其他模块的关系

| 模块 | 关系 |
|---|---|
| `backend/linear.rs` | `Backend::Linear`，`new_linear` |
| `backend/cow.rs` | `Backend::Cow`，`new_cow`/`new_alloc`，COW 帧表 |
| `backend/shared.rs` | `Backend::Shared`，`SharedPages`，`new_shared` |
| `backend/file.rs` | `Backend::File`，页缓存映射，`new_file` |
| `mm/aspace/mod.rs` | `MemorySet<Backend>` 的宿主 |
| `syscall/mm/mmap.rs` | 选择后端 |
| `mm/loader.rs` | ELF 段用 Cow，trampoline 用 Linear，栈/堆用 Cow(alloc) |
| `syscall/ipc/shm.rs` | 用 Shared |

## 7. 已知局限 / TODO

- `MappingBackend` 的布尔接口吞错误（日志可查，但无法区分失败原因）。
- `alloc_frame` 支持 2M/1G 页（`num_pages` 个 4K 页连续分配），依赖 `axalloc` 的连续性；大页分配可能失败。
- 无 `swap`/`page reclaim` 后端。
- `enum_dispatch` 新增变体需要改所有 match（当前无 match，全靠 dispatch）。

## 8. 变更影响面

改本文件 → `backend/*.md` 全部、`mm/aspace/mod.rs.md`、`mm/access.rs.md`、`syscall/mm/mmap.rs.md`、`CONSTRAINTS.md` C9/C10。
