# `kernel/src/syscall/mm/brk.rs`

> 对应源码：`kernel/src/syscall/mm/brk.rs`
> 所属子系统：堆管理（brk）
> 依赖：`config::{USER_HEAP_BASE, USER_HEAP_SIZE, USER_HEAP_SIZE_MAX}`、`mm::Backend`、`memory_addr::{VirtAddr, align_up_4k}`
> 被依赖：`syscall/mm/mod.rs`、`syscall/mod.rs`

## 1. 文件职责

实现 `brk(addr)`：调整进程堆顶（`ProcessData.heap_top`），按需映射/解除 `Cow(alloc)` 区域。

## 2. 关键流程

```text
current_top = proc_data.get_heap_top()
heap_limit = USER_HEAP_BASE + USER_HEAP_SIZE_MAX
addr == 0            → 返回 current_top
addr < BASE || > limit → 返回 current_top（不改变，Linux 返回旧值）
new_top = align_up_4k(addr); cur_top = align_up_4k(current_top)
initial_heap_end = USER_HEAP_BASE + USER_HEAP_SIZE

if new_top > cur_top:
    expand_start = max(initial_heap_end, cur_top)
    expand_size = new_top - expand_start
    若 expand_size > 0 且 map(expand_start, expand_size, RW|USER, populate=false, Backend::new_alloc) 失败
        → 返回 current_top（失败不改）
elif new_top < cur_top:
    shrink_start = max(initial_heap_end, new_top)
    shrink_size = cur_top - shrink_start
    若 shrink_size > 0 且 unmap 失败 → 返回 current_top
proc_data.set_heap_top(addr)
返回 addr
```

## 3. 设计要点

- **初始堆已映射**：`load_user_app` 映射了 `[USER_HEAP_BASE, USER_HEAP_BASE+USER_HEAP_SIZE)`；`brk` 只处理超出初始区域的部分，避免重复映射。
- **失败返回旧值**：Linux `brk` 失败时返回当前 brk（不是错误），程序据此判断。
- **`populate=false`**：扩张只登记 VMA，页在首次访问（缺页）时分配。
- **收缩只 unmap 超出初始区域的页**：初始 64KiB 永远保留（程序可能仍在使用）。
- `addr == 0` 查询语义；`addr` 在合法范围内但未对齐时，`align_up_4k` 后映射，但 `heap_top` 存的是原始 `addr`（未对齐）。

## 4. 不变量与约束

- **C8**：`aspace.lock()` 在 `map/unmap` 时持有；不能持锁做阻塞操作。
- `USER_HEAP_BASE + USER_HEAP_SIZE_MAX` 必须小于栈底（`config` 不变量）。
- 失败必须返回旧值，不能返回 `-errno`（Linux 语义）。
- `heap_top` 用 `AtomicUsize`（`set_heap_top/get_heap_top`），与 `mmap` 可能并发（单核下顺序执行）。
- 收缩不能低于 `initial_heap_end`（初始堆不可回收）。
- 扩张的 `Backend::new_alloc` 是 COW 匿名后端（`populate` 时清零分配）。

## 5. 与其他模块的关系

| 模块 | 关系 |
|---|---|
| `mm/loader.rs` | 初始堆映射 |
| `mm/aspace/mod.rs` | `map/unmap` |
| `mm/aspace/backend/cow.rs` | `new_alloc` |
| `task/mod.rs` | `heap_top` |
| `syscall/task/execve.rs` | `set_heap_top(USER_HEAP_BASE)` 重置 |
| `syscall/task/clone.rs` | 子进程继承 `heap_top` |

## 6. 已知局限 / TODO

- `brk` 与 `mmap` 的区域可能冲突（`mmap` 的 `find_free_area` 从 hint 向上找，可能落在堆扩张区——但堆在 `[0x4000_0000, 0x6000_0000)`，`mmap` 从低地址找，通常不冲突）。
- 未对齐 `addr` 存原始值，`get_heap_top` 返回未对齐值；后续 `brk` 计算用对齐值。
- 收缩失败时堆顶不变（但页可能已被部分 unmap？——`unmap` 失败会回滚，`MemorySet` 保证）。
- 无 `RLIMIT_DATA` 检查。

## 7. 变更影响面

改本文件 → `mm/loader.rs.md`、`mm/aspace/mod.rs.md`、`task/mod.rs.md`、`syscall/task/{execve,clone}.rs.md`、`CONSTRAINTS.md` C8。
