# `kernel/src/syscall/mm/mmap.rs`

> 对应源码：`kernel/src/syscall/mm/mmap.rs`
> 所属子系统：内存映射 syscall
> 依赖：`mm::{Backend, SharedPages}`、`pseudofs::{Device, DeviceMmap}`、`axfs::FileBackend`、`linux_raw_sys::general::{PROT_*, MAP_*}`、`memory_addr::{MemoryAddr, VirtAddr, VirtAddrRange}`
> 被依赖：`syscall/mm/mod.rs`、`syscall/mod.rs`

## 1. 文件职责

实现 `mmap/munmap/mprotect/mremap/madvise/msync/mlock/mlock2`。

## 2. `sys_mmap` 关键流程

```text
length == 0 → EINVAL
permission_flags = MmapProt::from_bits_truncate(prot)
map_flags = MmapFlags::from_bits(flags)（未知位：若 SHARED_VALIDATE → EOPNOTSUPP；否则 truncate + warn）
map_type = map_flags & TYPE；必须是 PRIVATE/SHARED/SHARED_VALIDATE
ANONYMOUS 与 (fd <= 0) 必须一致；匿名且 offset != 0 → EINVAL
offset 转 usize 且 4K 对齐
page_size = HUGE_1GB ? 1G : HUGE ? 2M : 4K
start = align_down(addr, page_size); end = align_up(addr+length); length = end-start
start = FIXED/FIXED_NOREPLACE ? (FIXED 先 unmap) : find_free_area(hint, ..., align) 或从 base 再找
file = fd > 0 ? Some(File::from_fd(fd)) : None
backend = match map_type {
  SHARED/SHARED_VALIDATE:
    有 file → FileBackend::Cached → new_file；Direct → Device::mmap():
                None → ENODEV；ReadOnly → new_cow；Physical(range) → 调整后 new_linear；Cache → new_file
    无 file → new_shared(start, SharedPages::new(length, 4K))
  PRIVATE:
    有 file → new_cow(file backend)
    无 file → new_alloc(start, page_size)
}
populate = POPULATE
aspace.map(start, length, permission_flags.into(), populate, backend)
返回 start
```

## 3. 其他函数

| syscall | 行为 |
|---|---|
| `munmap(addr,len)` | `align_up_4k(len)` → `aspace.unmap` |
| `mprotect(addr,len,prot)` | `from_bits` 严格（含 GROW* 拒绝）→ `align_up_4k` → `aspace.protect` |
| `mremap(addr,old,new,flags)` | 简化：校验对齐 → `mmap(new_size, PRIVATE, fd=-1)` → 复制 `min(new,old)` → `munmap(old)`（**不是原地扩展**，flags 忽略） |
| `madvise` | 记日志，返回 0 |
| `msync` | 记日志，返回 0 |
| `mlock`/`mlock2` | 返回 0（不锁定） |

## 4. 不变量与约束

- **C8**：整个 `mmap` 持 `aspace` 锁（`find_free_area`/`map`/`unmap`）；不能持锁做文件 I/O（文件映射的读取延迟到 `populate`/缺页）。
- **C9**：`new_alloc`/`new_cow` 走 COW 帧表；`new_shared` 分配 `SharedPages`。
- **C10**：`new_shared` 的映射不能部分 unmap。
- `MAP_FIXED` 会先 `unmap` 目标区间；`MAP_FIXED_NOREPLACE` 不 unmap（由 `map` 失败返回 EEXIST？——当前 `aspace.map` 对重叠会返回错误，但错误码不一定是 EEXIST）。
- `ANONYMOUS` 与 `fd` 的一致性检查：`(flags & ANONYMOUS) != (fd <= 0)` → 匿名必须无 fd、非匿名必须有 fd。
- `offset` 必须是 4K 对齐（`PageSize::Size4K.is_aligned`）；大页映射的 offset 仍需 4K 对齐（Linux 允许更大对齐）。
- `DeviceMmap::Physical` 的 `range.start += offset` 后若为空 → `EINVAL`；`length` 被截断到 `range.size().align_down(page_size)`。
- `mremap` 的实现是"复制+重映射"，会改变地址（Linux 的 `mremap` 尽量原地扩展）；依赖 `MREMAP_FIXED/MAYMOVE` 的程序可能行为不同。
- `MAP_HUGETLB` 只影响 `page_size`，后端仍是 4K 对齐分配（大页支持不完整）。

## 5. 与其他模块的关系

| 模块 | 关系 |
|---|---|
| `mm/aspace/mod.rs` | `map/unmap/protect/find_free_area` |
| `mm/aspace/backend/{cow,shared,file,linear}.rs` | 后端选择 |
| `file/fs.rs` | `File::from_fd` |
| `pseudofs/device.rs` | `DeviceMmap` |
| `pseudofs/dev/fb.rs` | `Physical` |
| `syscall/ipc/shm.rs` | 用 `SharedPages`（不经过本文件） |
| `mm/loader.rs` | ELF 用 `map` 内部 API（不经过 sys_mmap） |

## 6. 已知局限 / TODO

- `mremap` 非原地/不支持 `MREMAP_FIXED/MAYMOVE/DONTUNMAP`。
- `madvise/msync/mlock` 是 no-op。
- `MAP_HUGETLB` 的大页分配不完整（`alloc_frame` 可能失败）。
- `MAP_FIXED_NOREPLACE` 的冲突错误码不精确。
- 无 `mmap` 的 `MAP_GROWSDOWN/STACK` 特殊处理（`STACK` 只是 flag）。
- `PROT_GROWSDOWN/GROWSUP` 在 `mprotect` 被拒绝。

## 7. 变更影响面

改本文件 → `mm/aspace/mod.rs.md`、`mm/aspace/backend/*.md`、`pseudofs/device.rs.md`、`pseudofs/dev/fb.rs.md`、`syscall/mm/{brk,mincore}.rs.md`、`CONSTRAINTS.md` C8/C9/C10。
