# `kernel/src/mm/loader.rs`

> 对应源码：`kernel/src/mm/loader.rs`
> 所属子系统：ELF 装载 / 用户地址空间初始化
> 依赖：`axfs::{CachedFile, FS_CONTEXT, FileBackend}`、`kernel_elf_parser`、`xmas_elf`、`ouroboros`、`uluru::LRUCache`
> 被依赖：`mm/mod.rs`、`entry.rs`、`syscall/task/{clone,execve}.rs`

## 1. 文件职责

1. `new_user_aspace_empty()`：按 `config::USER_SPACE_BASE/SIZE` 创建空地址空间；
2. `copy_from_kernel(&mut aspace)`：把内核页表项复制进用户页表（仅非 aarch64/la64）；
3. `map_trampoline(aspace)`：把信号蹦床（一页 RX）映射到 `SIGNAL_TRAMPOLINE`；
4. `load_user_app(aspace, path, args, envs)`：解析/装载 ELF（含动态链接器、`#!` 脚本、`.sh` 特判），映射栈/堆，构造 `argv/envp/auxv` 栈数据，返回 `(entry, ustack_top)`；
5. ELF 缓存（`LRUCache<ElfCacheEntry, 32>`），支持 `clear_elf_cache()`。

## 2. 关键流程

### 2.1 `load_user_app`

```text
path = path.or(args[0])
若 path 以 ".sh" 结尾 → 递归改成 /bin/sh + 原 args        （busybox 兼容）
ELF_LOADER.lock().load(uspace, path):
    失败且 data 以 "#!" 开头 → 解析 shebang，递归 exec 解释器
    其他失败 → InvalidExecutable
ustack_top = USER_STACK_TOP; ustack_start = top - USER_STACK_SIZE
map(ustack_start, USER_STACK_SIZE, RW|USER, populate=false, Backend::new_alloc)
stack_data = app_stack_region(args, envs, &auxv, ustack_top)   // kernel_elf_parser
user_sp = ustack_top - stack_data.len(); 对齐向下 4K
populate_area(user_sp_aligned, ceil(top - user_sp_aligned), RW)
write(user_sp, stack_data)
map(USER_HEAP_BASE, USER_HEAP_SIZE, RW|USER, populate=true, Backend::new_alloc)   // 初始堆
返回 (entry, user_sp)
```

### 2.2 `ElfLoader::load`（缓存 + 动态链接）

```text
loc = resolve(path)
若缓存里没有该 Location：
    ElfCacheEntry::load(loc)：
        CachedFile::get_or_create(loc)          // 页缓存
        读前 4096 字节做 ELF header 解析；phdr 超出时再读一段
        解析失败 → 返回 Err(data)（用于 shebang 判断）
      插入 LRU
uspace.clear(); map_trampoline(uspace)
若存在 PT_INTERP：
    读取 interpreter 路径（CStr）
    把 ld.so 也加载进缓存
    分别 map_elf(elf, base=USER_SPACE_BASE) 与 map_elf(ldso, base=USER_INTERP_BASE)
    entry = ldso.entry(); auxv 用 elf.aux_vector(4K, Some(ldso.base()))
否则 entry = elf.entry()，auxv = elf.aux_vector(4K, None)
```

### 2.3 `map_elf`

- 遍历 `PT_LOAD`；`vaddr = ph.virtual_addr + base`；
- `seg_pad = vaddr % 4K`，断言 `seg_pad == ph.offset % 4K`（ELF 规范）；
- `seg_align_size` 向上取整到 4K；
- 用 `Backend::new_cow(seg_start, Size4K, FileBackend::Cached(cache), ph.offset, Some(ph.offset+file_size))` 映射 `populate=false`；
- `// TDOO: flush the I-cache`（拼写原样，未实现）。

### 2.4 `ElfCacheEntry`（ouroboros 自引用）

`cache: CachedFile` + `data: Vec<u8>` + `elf: ELFHeaders<'this>`（借用 data）；`try_new_or_recover` 在构建失败时回收 data（用于 shebang）。

## 3. 不变量与约束

- **pg 边界**：`seg_pad == ph.offset % 4K` 是 ELF 必须满足的条件；assert 失败说明镜像损坏。
- **缓存 key 是 `Location` 指针相等**（`borrow_cache().location().ptr_eq(&loc)`）；同一文件不同挂载点会被视为不同项。
- `uspace.clear()` 在装载前调用，会清空旧映射（execve 语义）；trampoline 在 clear 之后重新映射。
- `map_trampoline` 用 `map_linear` 映射**物理地址**（`virt_to_phys(signal_trampoline_address)`），因此用户态可直接执行内核中的 trampoline 代码。
- 栈的 `populate_area` 只填实际用到的部分（`user_sp_aligned` 到 `ustack_top`），不是整个 512KiB。
- 堆用 `populate=true`，即初始 64KiB 立即分配。
- `load_user_app` 递归：`.sh` 与 shebang 都通过重新调用自身实现，无递归深度限制（恶意镜像可构造深递归）。
- `copy_from_kernel` 的 cfg：`not(any(aarch64, loongarch64))`，两种架构用独立用户页表。

## 4. 与其他模块的关系

| 模块 | 关系 |
|---|---|
| `config::*` | 地址常量 |
| `mm/aspace/mod.rs` | `map/map_linear/populate_area/write/clear` |
| `mm/aspace/backend/{cow,linear}.rs` | 段映射与 trampoline |
| `file/fs.rs` | `CachedFile` 通过 `FileBackend::Cached` 交给 Cow |
| `entry.rs` | 首次装载 init |
| `syscall/task/execve.rs` | 重新装载 |
| `syscall/task/clone.rs` | `copy_from_kernel`（fork 新地址空间时） |
| `pseudofs/dev/memtrack.rs` | `clear_elf_cache()` 排除噪声 |

## 5. 已知局限 / TODO

- `// TDOO: flush the I-cache`（未实现，非一致性 I-cache 架构可能执行旧指令）。
- `// FIXME: impl /proc/self/exe to let busybox retry running`（第 282 行）。
- shebang/`.sh` 递归无深度/长度限制。
- ELF 缓存 32 项，淘汰策略是 LRU；缓存项持有 `CachedFile`（页缓存句柄），可能延长文件生命周期。
- 不支持 `PT_GNU_STACK`、`PT_TLS`（`CLONE_SETTLS` 由 clone 处理）。
- `map_elf` 未处理段之间的空洞（依赖 `seg_align_size` 与 vaddr 对齐）。
- `populate_area` 失败时栈数据未写入会返回错误，但已填充的页不会回收（随后进程失败退出）。

## 6. 变更影响面

改本文件 → `mm/aspace/mod.rs.md`、`mm/aspace/backend/cow.rs.md`、`entry.rs.md`、`syscall/task/execve.rs.md`、`syscall/task/clone.rs.md`、`CONSTRAINTS.md` C8、`ARCHITECTURE.md#5`。
