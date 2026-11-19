# `kernel/src/syscall/mm/mincore.rs`

> 对应源码：`kernel/src/syscall/mm/mincore.rs`
> 所属子系统：内存驻留查询（mincore）
> 依赖：`axhal::paging::MappingFlags`、`memory_addr::PAGE_SIZE_4K`、`starry_vm::vm_write_slice`、`task::AsThread`
> 被依赖：`syscall/mm/mod.rs`

## 1. 文件职责

实现 `mincore(addr, length, vec)`：判断一段用户虚拟内存的每页是否驻留在内存中，结果位图写回用户。

## 2. 关键流程

```text
addr 非 4K 对齐 → EINVAL
vec 为空 → EFAULT
length == 0 → Ok(0)                     // Linux：不校验映射
page_count = ceil(length/4K)
aspace = current().as_thread().proc_data.aspace.lock()
i = 0
while i < page_count:
    addr_i = start + i*4K
    area = aspace.find_area(addr_i).ok_or(ENOMEM)?
    若 !area.flags().contains(USER) → ENOMEM
    match page_table.query(addr_i):
        Ok((_,_,size)) → resident = true; n = size/4K
        Err(_)         → resident = false; n = 1
    if resident: result[i..min(i+n,page_count)].fill(1)
    i += n
vm_write_slice(vec, result)
```

## 3. 设计要点

- **批量查询**：`page_table.query` 返回页大小，对大页（2M/1G）一次覆盖多页，`result` 连续填 1。
- **未映射页**：`query` 返回 `NotMapped` 时只标记 1 页（`n=1`），继续。
- **`vec` 长度**：调用方必须提供 `ceil(length/4K)` 字节；本实现按该长度分配并写回（不检查用户缓冲实际大小，`vm_write_slice` 会校验）。
- **`length == 0`**：直接成功，不校验 `addr` 映射（符合 Linux）。
- 返回值只表示"是否驻留"，不表示可访问。

## 4. 不变量与约束

- **C6**：`vec` 经 `vm_write_slice`；结果位图写用户内存。
- **C24**：`addr` 必须 4K 对齐；`vec` 非空。
- `area.flags().contains(USER)`：内核映射（无 USER）视为 ENOMEM（防止用户探测内核地址）。
- `aspace` 锁在**整个循环**内持有：`page_table.query` 不阻塞，但 `vm_write_slice` 在锁外（循环结束后）。注意：`find_area`/`query` 期间不能触发缺页（不访问用户内存），安全。
- 位图只使用 bit0（驻留），bits1-7 保留为 0（Linux 语义）。
- 返回 0，不返回页数。

## 5. 与其他模块的关系

| 模块 | 关系 |
|---|---|
| `mm/aspace/mod.rs` | `find_area`/`page_table()` |
| `mm/access.rs` | `vm_write_slice` |
| `task/mod.rs` | `proc_data.aspace` |
| `syscall/mm/mmap.rs` | 映射建立后 mincore 才可能返回 1 |

## 6. 已知局限 / TODO

- 未校验 `length` 是否超出用户空间（`aspace.find_area` 会返回 None → ENOMEM，但 Linux 对 `addr+length > TASK_SIZE` 也是 ENOMEM，行为一致）。
- 不区分"页在内存但被换出"（无 swap）。
- 大页判定依赖 `query` 返回的 `size`；若后端用 4K 而页表是大页，可能不一致。
- `vec` 的剩余长度未检查（依赖 `vm_write_slice` 的 `check_access`）。

## 7. 变更影响面

改本文件 → `mm/aspace/mod.rs.md`、`mm/access.rs.md`、`syscall/mm/mod.rs.md`、`CONSTRAINTS.md` C6/C24。
