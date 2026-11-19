# `kernel/src/mm/aspace/backend/file.rs`

> 对应源码：`kernel/src/mm/aspace/backend/file.rs`
> 所属子系统：文件映射后端（MAP_SHARED 文件）
> 依赖：`axfs::{CachedFile, FileFlags}`、`axhal::paging::{PageTableCursor, PagingError}`、`Arc/Weak/AtomicUsize`
> 被依赖：`backend/mod.rs`、`syscall/mm/mmap.rs`（`MAP_SHARED` 文件）、`pseudofs/dev/loop.rs`（`DeviceMmap::Cache`）、`mm/loader.rs`（作为 Cow 的 file 来源）

## 1. 文件职责

实现 `MAP_SHARED` 文件映射：直接映射页缓存中的物理页；当页被缓存驱逐时，回调解除用户页表映射。

```rust
pub struct FileBackendInner {
    start: VirtAddr,
    cache: CachedFile,
    flags: FileFlags,          // 打开时的读/写权限
    offset_page: u32,          // 文件内起始页号
    handle: AtomicUsize,       // evict listener 句柄（0 = 未注册）
    futex_handle: Arc<()>,     // 作为共享 futex 表的身份
}
#[derive(Clone)] pub struct FileBackend(Arc<FileBackendInner>);
```

## 2. 关键流程

### 2.1 `new_file(start, cache, flags, offset, aspace)`

- `offset_page = offset / 4K`；
- 创建 inner 并 `register_listener(aspace)`：
  - `cache.add_evict_listener(|pn, page| ...)`，回调里 `Weak` 升级 `FileBackendInner` 与 `aspace`；
  - 用 `aspace.try_lock()`：拿不到锁（正在 populate）就返回，把解除映射委托给 populate 流程；
  - `on_evict(pn, &mut aspace)`。

### 2.2 `on_evict(pn, aspace)`

```text
pn_file = pn.checked_sub(offset_page) → 不相关页直接返回
vaddr = start + pn_file * 4K
若 find_area(vaddr) 的 backend 不是本 FileBackend → 返回（不是本映射控制的页）
pt.cursor().unmap(vaddr)（NotMapped 忽略）
```

### 2.3 `populate(range, flags, access_flags, pt)`

```text
start_page = (range.start - start)/4K + offset_page
for (i,addr) in pages_in(range,4K):
  pn = start_page + i
  pt.query(addr):
    Ok((paddr,page_flags,_)):
        需要写但页只读：
            in_memory = cache.in_memory()
            cache.with_page(pn, |page| {
                若 !in_memory → page.mark_dirty()      // 页不在内存时写回
                pt.remap(addr, paddr, flags)           // 升级权限
                pages += 1
            })
        已有权限 → pages += 1
    Err(NotMapped):
        map_flags = if cache.in_memory() { flags } else { flags - WRITE }
        cache.with_page_or_insert(pn, |page, evicted| {
            evicted → to_be_evicted.push(pn)
            pt.map(addr, page.paddr(), 4K, map_flags)
            pages += 1
        })
返回 (pages, Some(callback))  // callback 在 populate 之后解除被驱逐页的映射
```

### 2.4 `clone_map`

- 新建 `FileBackendInner`（同 cache/flags/offset_page，`futex_handle` 克隆共享）；
- `register_listener(new_aspace)`；
- 返回 `Backend::File`。

### 2.5 `check_flags`

`map/on_protect` 校验：`MappingFlags` 需要的 READ/WRITE 必须被 `FileFlags` 覆盖，否则 `PermissionDenied`。

## 3. 不变量与约束

- **C8/C32**：`on_evict` 通过 `try_lock` 避免死锁；回调执行时可能仍持有缓存锁，**不能**再取 `aspace` 锁（已通过参数传入）。
- `evict listener` 必须在 `Drop` 时 `remove_evict_listener(handle)`（handle != 0 时）；否则缓存持有悬垂回调。
- `futex_handle: Arc<()>` 稳定存在，供 `FutexKey::Shared` 作为 key（`file.futex_handle()` 返回 `Weak`）。
- `to_be_evicted` 的回调必须在 `populate` 返回后执行（携带 `FileBackendInner` 的 clone），避免在持页表游标时 unmap 同一游标。
- `in_memory()` 区分"文件完全在内存（如 tmpfs）"与"走页缓存"：前者不需要 dirty 标记、map 时可用原 flags；后者写时需要 `flags - WRITE`（写保护触发缺页 → 标记 dirty）。
- `offset_page` 是 `u32`：文件偏移不能超过 16TiB 页（4K * 2^32）。
- `check_flags` 是唯一权限闸门；不要绕过（否则用户可写只读打开的文件映射）。

## 4. 与其他模块的关系

| 模块 | 关系 |
|---|---|
| `syscall/mm/mmap.rs` | `MAP_SHARED` + 普通文件 → `Backend::new_file` |
| `mm/loader.rs` | `Backend::new_cow(..., FileBackend::Cached(cache), ...)`，本文件只提供类型 |
| `pseudofs/dev/loop.rs` | `DeviceMmap::Cache(cache)` → `new_file` |
| `mm/aspace/mod.rs` | `populate` 的 callback 执行 `on_evict` |
| `axfs::CachedFile` | `with_page/with_page_or_insert/add_evict_listener` |
| `task/futex.rs` | `futex_handle()` |

## 5. 已知局限 / TODO

- `// TODO(mivik): file mmap page size`：只支持 4K（未用大页缓存）。
- 驱逐回调只处理"本映射控制的页"；`find_area` 按 vaddr 判断 backend 指针相等（`Arc::ptr_eq`），clone 后的子映射有独立 inner，需各自的 listener。
- 写时 `flags - WRITE` 的映射依赖缺页把页标记 dirty；若页已在内存则直接 `remap` 不加 dirty（in_memory 分支）。
- 没有 `msync` 实现（`sys_msync` 是 no-op）。
- `Drop` 时 `remove_evict_listener` 用 `unsafe`，handle 类型是 `usize`。

## 6. 变更影响面

改本文件 → `backend/mod.rs.md`、`syscall/mm/mmap.rs.md`、`mm/aspace/mod.rs.md`、`mm/loader.rs.md`、`pseudofs/dev/loop.rs.md`、`task/futex.rs.md`、`CONSTRAINTS.md` C8/C32。
