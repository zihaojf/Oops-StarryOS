# `kernel/src/file/epoll.rs`

> 对应源码：`kernel/src/file/epoll.rs`
> 所属子系统：I/O 多路复用（epoll 实例）
> 依赖：`axpoll::{PollSet, Pollable, IoEvents}`、`hashbrown::HashMap`、`kspin::SpinNoPreempt`、`linux_raw_sys::{EPOLLET, EPOLLONESHOT, epoll_event}`
> 被依赖：`file/mod.rs`、`syscall/io_mpx/epoll.rs`

## 1. 文件职责

实现 epoll **实例**（`Epoll`）：维护 interest 表 + 就绪队列，支持 LT/ET/ONESHOT 三种触发模式，并把被关注 fd 的 waker 转成"加入就绪队列"。

## 2. 关键类型

```rust
pub struct EpollEvent { pub events: IoEvents, pub user_data: u64 }
pub struct EpollFlags: EPOLLET | EPOLLONESHOT
enum TriggerMode { Level, Edge, OneShot { fired: bool } }
struct EntryKey { fd: i32, file: Weak<dyn FileLike> }   // Hash/Eq 用 (fd, Weak ptr)
struct EpollInterest {
    key: EntryKey, event: EpollEvent,
    mode: SpinNoPreempt<TriggerMode>, in_ready_queue: AtomicBool,
}
struct InterestWaker { epoll: Weak<EpollInner>, interest: Weak<EpollInterest> }
struct EpollInner {
    interests: SpinNoPreempt<HashMap<EntryKey, Arc<EpollInterest>>>,
    ready_queue: SpinNoPreempt<VecDeque<Weak<EpollInterest>>>,
    poll_ready: PollSet,
}
pub struct Epoll { inner: Arc<EpollInner> }   // Default
```

## 3. 关键流程

### 3.1 `add(fd, event, flags)`

1. `EntryKey::new(fd)`（要求 fd 有效）；
2. 已存在 → `AlreadyExists`；
3. 插入 interest；
4. `check_and_register_waker`：先 `file.poll() & events`，已就绪则 `waker.wake_by_ref()`（直接入队）；否则 `file.register(context, events)`，再复查一次（避免注册前已有事件丢失）。

### 3.2 `modify` / `delete`

- `modify`：用新 `EpollInterest` 替换；若旧条目已在就绪队列，新条目继承 `in_ready_queue = true`；重新注册 waker。
- `delete`：仅从 interest 表移除；就绪队列中的弱引用会在消费时失效。

### 3.3 `InterestWaker::wake`

```text
interest.try_mark_in_queue()（CAS false→true）成功才 push_back(Weak(interest)) 并 epoll.poll_ready.wake()
```

`in_ready_queue` 保证同一 interest 不会在队列里重复（ET 的关键）。

### 3.4 `poll_events(out)`

```text
loop {
    pop_front → 无 → break
    输出数组满 → push_front 放回 → break
    weak.upgrade 失败 → continue（已删除）
    key.get_file 失败 → 移除 interest、mark_not_in_queue、continue（fd 已关闭）
    interest.consume(file):
        当前 events = file.poll() & interest.events
        空 → NoEvent：mark_not_in_queue + 只注册 waker（不入队）
        否则按 mode.should_notify()：
            Level    → EventAndKeep：写 out，count++，push_back 回队列（下次仍可读）
            Edge/OneShot → EventAndRemove：写 out，count++，mark_not_in_queue，只注册 waker
        OneShot 已 fired → NoEvent
}
count == 0 → WouldBlock；否则 Ok(count)
```

### 3.5 `TriggerMode::should_notify`

- `Level`：恒 true（只要条件仍满足）。
- `Edge`：恒 true（每次被 wake 才入队，入队即消费一次）。
- `OneShot`：首次 true 并置 `fired`；之后 false（直到 `modify` 重建）。

## 4. 不变量与约束

- **`in_ready_queue` 与 `mode` 的一致性**：只有 `try_mark_in_queue` 成功才能入队；`mark_not_in_queue` 必须在"消费完/不可通知"后调用，否则 ET 会丢事件。
- `EpollInner.interests` 与 `ready_queue` 用 `SpinNoPreempt`（不能在持有期间阻塞）；`poll_events` 在循环内多次释放/获取锁，**不能**改成持锁调用 `file.poll()`（后者可能触发回调用 `InterestWaker` → 再次加锁，死锁）。
- 消费时 `file` 可能已被关闭（weak upgrade 失败），必须移除 interest 防泄漏。
- `EntryKey` 的 Eq 用 `Weak::ptr_eq`：同一 fd 被关闭后重新 open 得到不同 `Arc`，视为新 key（正确）。
- `Epoll::poll`（作为 `Pollable`）只表示"就绪队列非空"；`register` 注册 `poll_ready`。
- LT 模式 `push_back` 回队列会与 `InterestWaker` 的 CAS 竞争：`EventAndKeep` 分支**没有**重新 `try_mark_in_queue`（`in_ready_queue` 仍为 true），所以直接 push_back 是配对操作。

## 5. 与其他模块的关系

| 模块 | 关系 |
|---|---|
| `syscall/io_mpx/epoll.rs` | `epoll_create1/epoll_ctl/epoll_pwait/epoll_pwait2` |
| `file/mod.rs` | `Epoll implements FileLike`（`path = anon_inode:[eventpoll]`） |
| 所有 `FileLike` 实现 | `poll/register` 被消费/注册 |
| `syscall/io_mpx/{poll,select}.rs` | 用 `FdPollSet` 而非 `Epoll` |
| `axpoll` | `PollSet`/`IoEvents` |

## 6. 已知局限 / TODO

- 只支持 `EPOLL_CTL_ADD/MOD/DEL`；未实现 `EPOLLEXCLUSIVE`、`EPOLLWAKEUP`。
- `epoll_ctl` 的 `event.events` 用 `IoEvents::from_bits_truncate` + 剩余位当 flags，未知位返回 `InvalidInput`（`EpollFlags::from_bits`）。
- ET 模式依赖底层 `register` 的语义；若底层只在状态变化时唤醒，ET 正常；若底层反复唤醒，会重复入队但被 `in_ready_queue` 抑制。
- 未实现嵌套 epoll 的特殊语义（epoll fd 加入另一个 epoll）。
- 就绪队列用 `Weak`，interest 被删除后残留项会被跳过（不清理队列，仅靠消费时跳过）。

## 7. 变更影响面

改本文件 → `syscall/io_mpx/epoll.rs.md`、`file/mod.rs.md`、`file/*.rs.md`（poll/register 契约）、`ARCHITECTURE.md#11`。
