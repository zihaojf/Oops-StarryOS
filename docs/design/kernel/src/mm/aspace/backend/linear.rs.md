# `kernel/src/mm/aspace/backend/linear.rs`

> 对应源码：`kernel/src/mm/aspace/backend/linear.rs`
> 所属子系统：线性映射后端
> 依赖：`axhal::paging::{MappingFlags, PageSize, PageTableCursor}`、`memory_addr::{PhysAddr, PhysAddrRange}`
> 被依赖：`backend/mod.rs`、`mm/aspace/mod.rs::map_linear`、`mm/loader.rs::map_trampoline`、`syscall/mm/mmap.rs`（物理设备）

## 1. 文件职责

实现"虚拟地址 ↔ 物理地址固定偏移"的映射：

```rust
#[derive(Clone)]
pub struct LinearBackend { offset: isize }   // pa = va - offset
```

- `pa(va)`：`PhysAddr((va as isize - offset) as usize)`；
- `map`：`pt.map_region(range.start, |va| self.pa(va), size, flags, false)`（一次建整段，`false` 表示不预先分配）；
- `unmap`：`pt.unmap_region`；
- `clone_map`：直接克隆自身（新地址空间用相同偏移）。

## 2. 构造

```rust
impl Backend { pub fn new_linear(offset: isize) -> Self }
```

`offset = start_vaddr - start_paddr`（由 `AddrSpace::map_linear` 计算）。

## 3. 使用场景

| 场景 | 调用点 |
|---|---|
| 信号 trampoline（内核代码页映射到用户空间） | `mm/loader.rs::map_trampoline`（`virt_to_phys(signal_trampoline_address)`） |
| `/dev/fb0` mmap（物理帧缓冲） | `syscall/mm/mmap.rs` 的 `DeviceMmap::Physical` |
| 可能的其他物理设备 | 同上 |

## 4. 不变量与约束

- **偏移恒定**：整个 VMA 内 VA 与 PA 差值固定，不能跨越物理不连续区域；物理设备映射必须保证目标物理区间连续。
- 依赖物理页已存在（不是惰性分配）：`map_region` 直接写页表，没有 `populate`；若物理地址无效，用户访问会触发**不可恢复的**缺页（`populate` 默认返回 `(0,None)` → `handle_page_fault` 返回 false → 内核 panic）。因此 `map_linear` 只用于内核确知有效的物理地址。
- `unmap` 不会释放物理页（这些页不属于本后端，归设备/内核所有）。
- `clone_map` 忽略 `range/flags/old_pt/new_pt`：fork 后子进程同样能访问同一物理页（对 trampoline 是期望行为；对 fb0 也可接受）。
- `page_size()` 固定 `Size4K`；大页设备映射不支持。

## 5. 与其他模块的关系

- `mm/aspace/mod.rs::map_linear`：唯一的创建入口（校验对齐/范围）。
- `mm/loader.rs::map_trampoline`：`READ|EXECUTE|USER`。
- `syscall/mm/mmap.rs`：`DeviceMmap::Physical` 分支把 `offset` 加到设备物理区间起点，并用 `start - range.start` 构造偏移。
- `mm/aspace/backend/mod.rs`：`Backend::Linear` 变体。

## 6. 已知局限 / TODO

- 不支持非连续物理区间。
- 没有 `on_protect` 特殊处理（`protect_region` 只改页表权限）。
- 没有 `populate`：无法与惰性分配配合。
- 大页（2M/1G）不可用。

## 7. 变更影响面

改本文件 → `backend/mod.rs.md`、`mm/aspace/mod.rs.md`、`mm/loader.rs.md`、`syscall/mm/mmap.rs.md`。
