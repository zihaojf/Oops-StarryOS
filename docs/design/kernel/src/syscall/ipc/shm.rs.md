# `kernel/src/syscall/ipc/shm.rs`

> 对应源码：`kernel/src/syscall/ipc/shm.rs`
> 所属子系统：System V 共享内存
> 依赖：`mm::{Backend, SharedPages, UserPtr, nullable}`、`BTreeMap`、`PAGE_SIZE_4K`、`mapping_flags`、`task/AsThread`
> 被依赖：`syscall/ipc/mod.rs`、`syscall/mod.rs`、`task/ops.rs`（`clear_proc_shm`）

## 1. 文件职责

实现 `shmget/shmat/shmctl/shmdt` 与共享内存管理器。

## 2. 关键类型

```rust
struct ShmAtFlags: SHM_RDONLY(0o10000), SHM_RND(0o20000), SHM_REMAP(0o40000)
pub struct ShmidDs { shm_perm: IpcPerm, shm_segsz, shm_atime/dtime/ctime, shm_cpid/lpid, shm_nattch }
pub struct ShmInner {
    pub shmid: i32, pub page_num: usize,
    va_range: BTreeMap<Pid, VirtAddrRange>,          // 每个 attach 进程的 VA
    pub phys_pages: Option<Arc<SharedPages>>,        // 首次 attach 时分配
    pub rmid: bool,
    pub mapping_flags: MappingFlags,
    pub shmid_ds: ShmidDs,
}
pub struct BiBTreeMap<K,V> { forward: BTreeMap<K,V>, reverse: BTreeMap<V,K> }   // key↔shmid
pub struct ShmManager {
    key_shmid: BiBTreeMap<i32, i32>,
    shmid_inner: BTreeMap<i32, Arc<Mutex<ShmInner>>>,
    pid_shmid_vaddr: BTreeMap<Pid, BiBTreeMap<i32, VirtAddr>>,
}
pub static SHM_MANAGER: Mutex<ShmManager>;
```

## 3. 关键流程

### 3.1 `sys_shmget(key, size, shmflg)`

```text
page_num = align_up_4k(size)/4K；0 → InvalidInput
mapping_flags = USER + (shmflg & 0o400 → READ) + (0o200 → WRITE) + (0o100 → EXECUTE)
key != IPC_PRIVATE 且已存在 → 校验 size/mapping_flags 与旧 shmid_ds 一致（否则 InvalidInput），更新 lpid，返回旧 shmid
否则新建 ShmInner + 插入 key_shmid/shmid_inner
```

### 3.2 `sys_shmat(shmid, addr, shmflg)`

```text
取 ShmInner；按 SHM_RDONLY 去掉 WRITE
（TODO: SHM_RND/SHM_REMAP 未实现）
找空闲 VA（优先 addr 对齐后，否则从 base 起）
插入 pid→shmid→vaddr 映射
若 ShmInner.phys_pages 为空（首次）：SharedPages::new(length, 4K)，map(Backend::new_shared)，map_to_phys
否则：map(已有 SharedPages)
attach_process(pid, va_range)：nattch+1、lpid/atime
返回 start_addr
```

### 3.3 `sys_shmctl(shmid, cmd, buf)`

| cmd | 行为 |
|---|---|
| `IPC_SET` | `shmid_ds = *buf.get_as_mut()?`（**直接覆盖整个结构**） |
| `IPC_STAT` | 写 `shmid_ds`（buf 可空） |
| `IPC_RMID` | `rmid = true`（真正的删除在最后一个 detach 时） |
| 其他 | `InvalidInput` |

### 3.4 `sys_shmdt(shmaddr)`

```text
按 (pid, vaddr) 找 shmid；取 ShmInner 与 va_range
aspace.unmap(va_range)
remove_shmaddr(pid, shmaddr)
detach_process(pid)：nattch-1、lpid/dtime
若 rmid && nattch == 0 → remove_shmid
```

### 3.5 `clear_proc_shm(pid)`（进程退出时）

遍历该进程所有 shmid → `detach_process` → 若 `rmid && nattch==0` 删除 → `remove_pid`。

## 4. 不变量与约束

- **C10**：`shmat` 的映射整段操作；`SharedBackend` 不支持部分 unmap（`shmdt` 用整段 `va_range`，安全）。
- **锁顺序**：`SHM_MANAGER` → `ShmInner(Mutex)` → `aspace(Mutex)`。`sys_shmat` 先取 `shm_inner`（释放 manager 锁）再取 `aspace`；`sys_shmdt` 先取 `shm_inner` 再 `aspace`，最后再取 `SHM_MANAGER`——**注意**：`sys_shmdt` 在持有 `shm_inner` 锁时取 `SHM_MANAGER`，与 `sys_shmat` 的 `SHM_MANAGER` → `shm_inner` 顺序相反，存在死锁风险（单核下由于不会并发进入，暂不触发）。
- `ShmInner.page_num` 固定；后续 `shmget` 用同 key 不同 size 会 `InvalidInput`。
- `attach_process` 断言该 pid 尚未 attach；`detach_process` 断言存在（否则 panic）。
- `BiBTreeMap` 保证 key↔value 双向唯一；`insert` 会移除旧映射的两端。
- `ShmidDs` 的 `shm_perm.mode` 被用来存 `mapping_flags.bits()`（不是权限位）——这是**语义挪用**，`IPC_STAT` 返回的 mode 不是权限。
- `shared_pages` 的物理页由 `SharedPages` 持有；`unmap` 不释放，`remove_shmid` 后 `Arc` drop 才释放。

## 5. 与其他模块的关系

| 模块 | 关系 |
|---|---|
| `mm/aspace/backend/shared.rs` | `SharedPages`/`Backend::new_shared` |
| `mm/aspace/mod.rs` | `map/unmap/find_free_area` |
| `task/ops.rs` | `do_exit` → `SHM_MANAGER.clear_proc_shm` |
| `syscall/ipc/mod.rs` | `IpcPerm/next_ipc_id` |
| `task/futex.rs` | 共享 futex 以 `SharedPages` 为 key |
| `syscall/mod.rs` | 分派 |

## 6. 已知局限 / TODO

- `SHM_RND/SHM_REMAP` 未实现。
- `IPC_SET` 直接覆盖 `shmid_ds`（无字段校验/权限检查）。
- `shm_perm.mode` 存 mapping flags（非权限）。
- 无 `SHM_LOCK/SHM_UNLOCK`。
- `sys_shmat` 用 `unwrap()` 取 inner（shmid 无效会 panic）。
- `sys_shmdt` 的锁顺序与 `shmat` 相反（潜在死锁）。
- 无 `/proc/sysvipc/shm`。

## 7. 变更影响面

改本文件 → `mm/aspace/backend/shared.rs.md`、`mm/aspace/mod.rs.md`、`task/ops.rs.md`、`ipc/mod.rs.md`、`ARCHITECTURE.md#12`、`CONSTRAINTS.md` C10/C32。
