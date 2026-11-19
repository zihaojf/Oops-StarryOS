# `kernel/src/task/futex.rs`

> 对应源码：`kernel/src/task/futex.rs`
> 所属子系统：同步原语（futex）
> 依赖：`axsync::Mutex`、`kspin::SpinNoIrq`、`hashbrown`、`axtask::future`
> 被依赖：`task/mod.rs`（重导出）、`task/ops.rs`、`syscall/sync/futex.rs`、`mm/aspace/backend/{shared,file}.rs`（`FutexKey`）

## 1. 文件职责

实现 Linux futex 的内核侧等待队列与键空间：

- `WaitQueue`：等待者集合（`Waker` + bitset），支持 `wait_if`/`wake`/`requeue`；
- `FutexKey`：区分**进程私有**地址与**共享**区域（`SharedPages` 或文件映射）；
- `FutexEntry`/`FutexTable`：地址 → 等待队列；
- `futex_table_for(key)`：私有走进程表，共享走全局按对象地址索引的表。

## 2. 关键类型

```rust
pub struct WaitQueue { queue: SpinNoIrq<VecDeque<(Waker, u32)>> }
pub enum FutexKey {
    Private { address: usize },
    Shared  { offset: usize, region: Result<Weak<SharedPages>, Weak<()>> },
}
pub struct FutexEntry { pub wq: WaitQueue, pub owner_dead: AtomicBool }
pub struct FutexTable(Mutex<HashMap<usize, Arc<FutexEntry>>>);
struct FutexTables { map: BTreeMap<usize, Arc<FutexTable>>, operations: usize }
static SHARED_FUTEX_TABLES: Mutex<FutexTables>;
```

## 3. 关键流程

### 3.1 `WaitQueue::wait_if(bitset, timeout, cond)`

```text
block_on(interruptible(future::timeout(timeout, poll_fn(|cx| {
    if 首次进入:
        lock queue;
        if !cond() { return Ready(Ok(false)); }     // 值不匹配 → 不等待
        queue.push_back((waker, bitset)); return Pending;
    else Ready(Ok(true))
}))))
```

- `bitset` 用于 `FUTEX_WAIT_BITSET/WAKE_BITSET`：唤醒时 `(bitset & mask) != 0` 才唤醒。
- 返回 `Ok(false)` 表示"条件已不满足，未等待"（`sys_futex` 映射为 `WouldBlock`）。

### 3.2 `wake(count, mask)`

在 `SpinNoIrq` 下遍历 `VecDeque`，`retain` 命中者（`bitset & mask != 0` 且未超 `count`）→ `waker.wake_by_ref()`，返回实际唤醒数。

### 3.3 `requeue(count, target)`

把队首 `count` 个（`count = min(count, len)`）搬到 `target.queue`，返回搬运数。

### 3.4 `FutexKey::new(aspace, address)`

```text
找 vaddr 所属 VMA：
  Backend::Shared(backend) → Shared{ offset: address - area.start, region: Ok(Weak<SharedPages>) }
  Backend::File(file)      → Shared{ offset: addr-area.start, region: Err(file.futex_handle()) }
  其他                     → Private{ address }
```

### 3.5 `futex_table_for(key)`

- `Private` → `current().as_thread().proc_data.futex_table`（每进程）。
- `Shared` → 用 `Weak::as_ptr` 作为稳定标识（`SharedPages` 或文件句柄），在 `SHARED_FUTEX_TABLES` 中取/建表。
- `FutexTables::get_or_insert` 每 100 次操作清理一次无人引用且为空的表（`operations` 计数）。

### 3.6 `FutexGuard`

`get`/`get_or_insert` 返回的守卫；`Drop` 时若 `Arc::strong_count <= 2 && wq.is_empty()` 就删除表项（避免表无限增长）。

## 4. 不变量与约束

- **id 稳定性**：共享 futex 表以 `Weak::as_ptr` 为键，必须在底层对象存活期间保持唯一；`SHARED_FUTEX_TABLES` 用 `Arc<FutexTable>` 保活表本身，但不保活 key 对象。
- **锁顺序**：`SHARED_FUTEX_TABLES` → `FutexTable.0` → `WaitQueue.queue`；`FutexGuard::drop` 内再取 `FutexTable.0`，调用方必须**先释放外层锁**（`drop_frame` 同理）。
- `wait_if` 在 `poll_fn` 的首次 poll 中才检查条件并注册 waker；条件闭包只能读用户内存一次（`uaddr.vm_read()`）。
- `Wake` 必须在持 `queue` 锁时调用 `waker.wake_by_ref()`；waker 的实现（`InterestWaker` 等）不能再获取同一把锁（否则死锁）。
- `owner_dead` 由 robust list 置位，`sys_futex` 在 `wait` 返回后 `swap(false)` 并返回 `EOWNERDEAD`。

## 5. 与其他模块的关系

| 模块 | 关系 |
|---|---|
| `syscall/sync/futex.rs` | 唯一调用者：`WAIT/WAKE/REQUEUE/CMP_REQUEUE` |
| `task/ops.rs` | `FutexKey::new_current` + `futex_table_for` 处理 robust list |
| `syscall/task/ops`（`do_exit`） | `clear_child_tid` 的唤醒 |
| `mm/aspace/backend/shared.rs` | `SharedPages` 作为共享 futex 的身份 |
| `mm/aspace/backend/file.rs` | `FileBackend::futex_handle()` 提供共享身份 |
| `mm/aspace/mod.rs` | `AddrSpace::find_area`（`FutexKey::new` 使用） |

## 6. 已知局限 / TODO

- 不支持 `FUTEX_WAIT_REQUEUE_PI`/`FUTEX_LOCK_PI`/`FUTEX_UNLOCK_PI`（`sys_futex` 返回 `Unsupported`）。
- 共享表的键是裸指针地址，对象释放后若同一地址被复用可能串表（概率低，`operations` 清理缓解）。
- `requeue` 不检查 `target` 是否同表/是否会造成重复等待者。
- 超时/可中断由调用方（`sys_futex` 的 `wait_if`）组合，本文件不感知信号。

## 7. 变更影响面

改本文件 → `syscall/sync/futex.rs.md`、`task/mod.rs.md`、`task/ops.rs.md`、`mm/aspace/backend/{shared,file}.rs.md`、`ARCHITECTURE.md#11`。
