# `kernel/src/mm/aspace/mod.rs`

> 对应源码：`kernel/src/mm/aspace/mod.rs`
> 所属子系统：用户地址空间
> 依赖：`axhal::paging::{PageTable, MappingFlags, PageFaultFlags}`、`memory_set::{MemorySet, MemoryArea}`、`axsync::Mutex`
> 被依赖：`mm/mod.rs`、`syscall/mm/*`、`mm/loader.rs`、`task/mod.rs`、`pseudofs/proc.rs`（maps）

## 1. 文件职责

`AddrSpace` 是用户进程的虚拟地址空间：

```rust
pub struct AddrSpace {
    va_range: VirtAddrRange,       // 用户空间范围（base/size 来自 config）
    areas: MemorySet<Backend>,     // VMA 集合
    pt: PageTable,                 // 硬件页表
}
```

提供：范围校验、空闲区查找、映射/解除/保护/填充、缺页处理、跨页表克隆、只读数据读写、`Debug`、`Drop`（clear）。

## 2. 关键方法

| 方法 | 作用 |
|---|---|
| `new_empty(base, size)` | 空地址空间 + 新页表（`PageTable::try_new`） |
| `validate_region(start,size)` | 范围 + 4K 对齐检查（`ax_bail!`） |
| `find_free_area(hint, size, limit, align)` | 委托 `MemorySet` |
| `find_area(vaddr)` | VMA 查找（`FutexKey`/mincore/缺页用） |
| `map_linear(start_vaddr, start_paddr, size, flags)` | 线性映射（trampoline、fb0） |
| `map(start, size, flags, populate, backend)` | 通用映射；`populate=true` 时立即填充 |
| `populate_area(start, size, access_flags)` | 逐 VMA 调 `backend.populate`；未覆盖 → `NoMemory` |
| `unmap(start, size)` | `MemorySet::unmap` |
| `protect(start, size, flags)` | `MemorySet::protect` |
| `read/write(start, buf)` | 通过页表查询**内核物理地址**直接拷贝（不经用户映射权限） |
| `can_access_range(start,size,flags)` | 检查 VMA 覆盖与权限（不检查页是否存在） |
| `handle_page_fault(vaddr, access_flags)` | 见 §3 |
| `try_clone()` | 逐 VMA `clone_map` 到新地址空间（COW 语义） |
| `areas()` | 迭代 VMA（`/proc/<pid>/maps` 现状未用） |
| `Drop` | `clear()` 解除所有映射 |

## 3. 关键流程

### 3.1 `handle_page_fault`

```text
if !va_range.contains(vaddr) → false
if let Some(area) = areas.find(vaddr):
    if area.flags().contains(access_flags):
        page_size = area.backend().page_size()
        populate(页对齐的 [vaddr, vaddr+page_size), area.flags(), access_flags, pt.cursor())
        Ok((n, callback)):
            callback 存在 → cb(self)      // FileBackend 驱逐列表回调
            n == 0 → warn + false
            else true
        Err → warn + false
false
```

### 3.2 `populate_area`

```text
validate_region
start 起循环：areas.find(start) → range = [start, min(area.end, end))
             backend.populate(range, area.flags(), access_flags, &mut pt.cursor())
             start = area.end()（断言 4K 对齐）
若 start < end → NoMemory
```

### 3.3 `try_clone`

```text
new = new_empty(base, size)
for area in self.areas:
    new_backend = area.backend().clone_map(range, flags, &mut self_pt, &mut new_pt, &new)
    new.areas.map(MemoryArea::new(...), &mut new.pt, false)
```

COW 后端会在这里把**父页表**的写权限降级、把同一物理页映射进子页表（引用计数 +1）；`File` 后端会注册驱逐监听。

### 3.4 `process_area_data`（read/write）

- 逐页 `pt.query(vaddr)` 拿物理地址 → `phys_to_virt` → `copy_nonoverlapping`；
- 处理首尾非对齐（`align_offset_4k`）；
- **不检查用户态权限**：用于内核写用户栈（`load_user_app`）与 `mincore` 等；
- 缺页会 `BadAddress`（所以调用方通常先 `populate_area`）。

## 4. 不变量与约束

- **C8/C32**：修改本结构必须持 `Arc<Mutex<AddrSpace>>`；锁内不得做阻塞 I/O（缺页重入）。
- `try_clone` 的顺序：先克隆父页表权限/子页表映射，再 `areas.map`；顺序反了会出现"VMA 存在但页表未映射"的窗口（单核下靠锁避免）。
- `handle_page_fault` 的 `callback` 必须在**释放页表游标后**执行（`cb(self)` 可能操作页表）；当前实现是 `populate` 返回后调用，安全。
- `populate_area` 要求区间被 VMA 完全覆盖，否则 `NoMemory`；不能用于"部分已映射"的场景。
- `read/write` 是**内核特权写入**，绕过用户权限位；不能把用户可控的 `start` 直接传进来（必须已校验 VMA）。
- `Drop` 必须 `clear()`，否则页表与物理页泄漏。
- `addrspace` 的 `areas` 与 `pt` 必须保持一致；新增方法若只改一个会留下不一致。

## 5. 与其他模块的关系

| 模块 | 关系 |
|---|---|
| `mm/aspace/backend/*` | `Backend`/`BackendOps`/`MemoryArea` 的后端实现 |
| `mm/loader.rs` | `map/map_linear/populate_area/write/clear` |
| `mm/access.rs` | `can_access_range/populate_area/handle_page_fault` |
| `syscall/mm/mmap.rs` | `map/unmap/protect/find_free_area` |
| `syscall/mm/brk.rs` | `map/unmap` |
| `syscall/mm/mincore.rs` | `find_area/page_table().query` |
| `syscall/ipc/shm.rs` | `map/unmap` + `Backend::new_shared` |
| `syscall/task/clone.rs` | `try_clone`/`page_table_root` |
| `entry.rs` | `page_table_root` 设置到任务 |
| `pseudofs/proc.rs` | `maps` 尚为假数据（未用 `areas()`） |

## 6. 已知局限 / TODO

- `/proc/<pid>/maps` 未使用 `areas()`（`pseudofs/proc.rs` 返回硬编码 vdso 行）。
- 无 `mprotect` 的 grow-down/grow-up 支持（`syscall/mm/mmap.rs` 直接拒绝）。
- `try_clone` 不复制 `va_range` 以外的内核页（由 `copy_from_kernel` 单独做）。
- 无 `protect` 的 COW 边界处理（依赖 `MemorySet::protect` 的回调）。
- `read/write` 的 `pt.query` 对未映射页返回 `BadAddress`，不做惰性填充。

## 7. 变更影响面

改本文件 → `mm/aspace/backend/*.md`、`mm/loader.rs.md`、`mm/access.rs.md`、`syscall/mm/*.md`、`syscall/ipc/shm.rs.md`、`syscall/task/clone.rs.md`、`pseudofs/proc.rs.md`、`CONSTRAINTS.md` C8/C10/C32。
