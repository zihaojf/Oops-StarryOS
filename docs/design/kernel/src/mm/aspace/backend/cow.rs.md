# `kernel/src/mm/aspace/backend/cow.rs`

> 对应源码：`kernel/src/mm/aspace/backend/cow.rs`
> 所属子系统：写时复制后端（MAP_PRIVATE / ELF / 栈 / 堆）
> 依赖：`axhal::paging::{PageTableCursor, PagingError}`、`kspin::SpinNoIrq`、`alloc::collections::BTreeMap`、`axfs::FileBackend`
> 被依赖：`backend/mod.rs`、`mm/loader.rs`（ELF 段、栈/堆）、`syscall/mm/mmap.rs`（`MAP_PRIVATE`）、`syscall/mm/brk.rs`

## 1. 文件职责

实现 `MAP_PRIVATE` 语义：初始可只读共享（文件内容），写时复制新页。

```rust
struct FrameRefCnt(u8);                          // 物理帧引用计数
struct FrameTableRefCount { table: BTreeMap<PhysAddr, Arc<SpinNoIrq<FrameRefCnt>>> }
static FRAME_TABLE: SpinNoIrq<FrameTableRefCount>;

#[derive(Clone)]
pub struct CowBackend {
    start: VirtAddr,
    size: PageSize,
    file: Option<(FileBackend, u64, Option<u64>)>,  // 初始内容来源（ELF 段）
}
```

## 2. 关键流程

### 2.1 `map()` —— 空实现

`populate` 之前不建立任何映射；VMA 只是登记。

### 2.2 `populate(range, flags, access_flags, pt)`

```text
for addr in pages_in(range, size):
  pt.query(addr):
    Ok((paddr, page_flags, page_size)):
        access 需要写 && 页不可写 → handle_cow_fault(addr, paddr, flags, pt); pages += 1
        page_flags 已含 access → pages += 1
    Err(NotMapped) → alloc_new_at(addr, flags, pt); pages += 1     // 首次访问/匿名
    Err(其他) → BadAddress
```

### 2.3 `alloc_new_at`

- `alloc_new_frame(true)`（清零）→ `FRAME_TABLE.init_frame`（计数=1）；
- 若有 `file`：按 `vaddr - self.start` 计算文件偏移，`file.read_at` 填充（处理非对齐映射的 gap 与 `file_end` 截断）；
- `pt.map(vaddr, frame, size, flags)`。

### 2.4 `handle_cow_fault`

```text
frame = FRAME_TABLE.get_frame_ref(paddr)（无 → BadAddress）
lock frame:
  0 → assert
  1 → 只有一个引用：直接 pt.protect(vaddr, flags)（升级为可写），返回
  _ → 分配新页、copy_nonoverlapping 整页、pt.remap(vaddr, new, flags)、frame.drop_frame(old)
```

### 2.5 `unmap`

逐页 `pt.unmap` → 若映射存在，取 `FRAME_TABLE` 引用并 `drop_frame`（计数为 0 时摘表 + `dealloc_frame`）；未映射页直接跳过。

### 2.6 `clone_map`（fork）

```text
cow_flags = flags - WRITE
for vaddr in range:
  old_pt.query(vaddr):
    Ok((paddr,...)) → frame.0 += 1；old_pt.protect(vaddr, cow_flags)；new_pt.map(vaddr, paddr, size, cow_flags)
    Err(NotMapped) → 跳过
```

即：父子共享物理页、双方都降为只读，后续写触发 COW。

## 3. 不变量与约束

- **C9（核心）**：帧引用计数必须与 `FRAME_TABLE` 一致：
  - `alloc_frame` 后必须 `init_frame`（`alloc_new_frame` 已做）；
  - 减少引用必须走 `FrameRefCnt::drop_frame`（它先摘表再 `dealloc_frame`；注释说明顺序原因：先 dealloc 再摘表会让别的线程分配到同一物理页）；
  - `drop_frame` 要求调用者**不持有 `FRAME_TABLE` 锁**（它内部会再取锁）。
- `u8` 引用计数上限：`clone_map` 中 `frame.0 += 1` 后若为 `u8::MAX` 返回 `BadAddress`（避免溢出）。
- `handle_cow_fault` 的 `frame` 锁顺序：`FRAME_TABLE` → `FrameRefCnt`；释放 `FRAME_TABLE` 后再 `frame.lock()`（`drop(frame_table)`）。
- `alloc_new_at` 中 `assert!(start < self.size as _)`：非对齐映射最多错开一页。
- 文件内容只在 `alloc_new_at` 时读取一次；后续写不回写（`MAP_PRIVATE` 语义）。
- `populate` 的 `flags` 是 VMA 权限，`access_flags` 是本次访问需要；COW 只处理"需要写但页只读"的情况。
- `CowBackend::clone()`（`derive(Clone)`）只是复制 `(start,size,file)` 描述，**不**增加任何帧引用；`clone_map` 才增加引用。

## 4. 与其他模块的关系

| 模块 | 关系 |
|---|---|
| `mm/loader.rs` | ELF 段（带 file）、栈/堆（无 file，`new_alloc`） |
| `syscall/mm/mmap.rs` | `MAP_PRIVATE` 匿名/文件 |
| `syscall/mm/brk.rs` | 堆扩张用 `new_alloc` |
| `mm/aspace/mod.rs` | `populate`/`clone_map` 调用方 |
| `mm/aspace/backend/file.rs` | 提供 `FileBackend`（作为 `file` 字段） |
| `backend/mod.rs` | `FRAME_TABLE` 相关辅助 |

## 5. 已知局限 / TODO

- 无 COW 的批量优化（逐页处理）。
- 帧引用计数 `u8`（最多 255 个地址空间共享同一页）。
- `clone_map` 只处理已映射页；未映射页在子进程首次访问时由 `populate` 分配（正确）。
- 文件内容读取在 `populate` 时同步进行（可能阻塞在 I/O，但不在 aspace 锁外——`populate` 被 `handle_page_fault` 在持锁下调用的，**这是潜在阻塞点**，见 C8 的例外讨论）。
- `unmap` 对 `pt.unmap` 返回的错误除 NotMapped 外未处理（`Ok((frame,..))` 分支才 drop）。

## 6. 变更影响面

改本文件 → `backend/mod.rs.md`、`mm/aspace/mod.rs.md`、`mm/loader.rs.md`、`syscall/mm/mmap.rs.md`、`syscall/mm/brk.rs.md`、`CONSTRAINTS.md` C9/C8。
