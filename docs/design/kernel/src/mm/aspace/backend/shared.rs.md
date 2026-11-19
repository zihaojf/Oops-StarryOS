# `kernel/src/mm/aspace/backend/shared.rs`

> 对应源码：`kernel/src/mm/aspace/backend/shared.rs`
> 所属子系统：共享映射后端（MAP_SHARED / SysV shm）
> 依赖：`axalloc`（经 `alloc_frame`）、`axhal::paging::{PageSize, PageTableCursor}`
> 被依赖：`backend/mod.rs`、`syscall/mm/mmap.rs`（`MAP_SHARED|MAP_ANONYMOUS`）、`syscall/ipc/shm.rs`（`shmat`）

## 1. 文件职责

实现多进程共享同一组物理页的映射：

```rust
pub struct SharedPages { pub phys_pages: Vec<PhysAddr>, pub size: PageSize }  // 持有一组页
#[derive(Clone)]
pub struct SharedBackend { start: VirtAddr, pages: Arc<SharedPages> }
```

- `SharedPages::new(size, page_size)`：按页分配并清零（`alloc_frame(true, size)`）；
- `Drop`：逐个 `dealloc_frame`；
- `map`：对 `range` 内每页 `pt.map(vaddr, paddr, self.pages.size, flags)`，物理页由 `pages_starting_from(range.start)` 决定（偏移 = `start - self.start` 对应的页索引）；
- `unmap`：逐页 `pt.unmap`（**不释放物理页**，由 `SharedPages::Drop` 负责）；
- `clone_map`：克隆自身（共享同一 `Arc<SharedPages>`）。

## 2. 关键流程

```text
shmget(key,size,flg) → 创建 ShmInner（此时无物理页）
shmat(shmid,addr,flg)
  ├─ 找空闲 VA 区间
  ├─ 若 ShmInner.phys_pages 为空（首次 attach）
  │     pages = Arc::new(SharedPages::new(length, 4K))
  │     aspace.map(start, length, flags, false, Backend::new_shared(start, pages.clone()))
  │     ShmInner.map_to_phys(pages)
  └─ 否则
        aspace.map(start, length, flags, false, Backend::new_shared(start, 已有 pages))
```

## 3. 不变量与约束

- **C10（重要）**：`// FIXME: This implementation does not allow map or unmap partial ranges.` —— `SharedBackend` 假设 VMA 起点与 `pages` 起点对齐；`unmap` 只解除 page 表中对应虚拟页，不处理"部分区间"与 `pages` 索引的对应关系（`pages_in(range)` 从 `range.start` 开始，但 `pages_starting_from` 依赖 `self.start`）。因此：
  - 不要对 shared 映射做部分 `munmap`/`mprotect` 后假设物理页索引正确；
  - 新增调用点必须整段操作。
- `start` 必须是 `SharedBackend` 的虚拟基址（创建时传入），`pages` 的索引基于 `start`。
- `SharedPages` 的物理页必须按 `size` 连续分配（`alloc_frame` 一次分配 `num_pages` 个 4K 页）。
- `unmap` 不 `dealloc`：物理页生命周期由 `Arc<SharedPages>` 决定；所有 attach 的地址空间都 drop 后，`SharedPages` 释放并归还页。
- `clone_map` 共享 `Arc`：fork 后子进程的 `MAP_SHARED` 区域与父进程共享物理页（正确语义）。
- 与 `FutexKey::Shared` 的关联：`SharedPages` 的 `Arc` 指针是共享 futex 表的 key（`mm/aspace/backend` 的 `FutexKey::new` 用 `Arc::downgrade(backend.pages())`）。

## 4. 与其他模块的关系

| 模块 | 关系 |
|---|---|
| `syscall/mm/mmap.rs` | `MAP_SHARED|MAP_ANONYMOUS` → `Backend::new_shared(start, Arc::new(SharedPages::new(...)))` |
| `syscall/ipc/shm.rs` | `sys_shmat`：首次分配 / 后续共享 |
| `mm/aspace/mod.rs` | `map/unmap/clone_map` |
| `task/futex.rs` | `FutexKey::Shared{region: Ok(Weak<SharedPages>)}` |
| `backend/mod.rs` | `alloc_frame/dealloc_frame/divide_page/pages_in` |

## 5. 已知局限 / TODO

- **部分区间 map/unmap 不支持（C10）**。
- 不支持 `MAP_SHARED` 文件映射（走 `FileBackend`）。
- 没有 `SharedPages` 的 `len/is_empty` 之外的统计（`len()` 返回页数）。
- 页大小只支持单一 `size`（整段统一）。

## 6. 变更影响面

改本文件 → `backend/mod.rs.md`、`syscall/mm/mmap.rs.md`、`syscall/ipc/shm.rs.md`、`task/futex.rs.md`、`CONSTRAINTS.md` C10。
