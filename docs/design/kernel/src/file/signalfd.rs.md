# `kernel/src/file/signalfd.rs`

> 对应源码：`kernel/src/file/signalfd.rs`
> 所属子系统：signalfd
> 依赖：`starry_signal::{SignalInfo, SignalSet}`、`axpoll::{PollSet, Pollable}`、`axtask::future::{block_on, poll_io}`、`zerocopy`
> 被依赖：`file/mod.rs`（`signalfd` 模块）、`syscall/fs/signalfd.rs`（`sys_signalfd4`）

## 1. 文件职责

实现 Linux `signalfd`：把"发给当前线程且被 mask 覆盖的待处理信号"通过 fd 读出来，格式为 128 字节 `signalfd_siginfo`。

## 2. 关键类型

```rust
const SIGNALFD_SIGINFO_SIZE: usize = 128;
#[repr(C)] struct SignalfdSiginfo {
    ssi_signo/ssi_errno/ssi_code/ssi_pid/ssi_uid/ssi_fd/ssi_tid/ssi_band/ssi_overrun/
    ssi_trapno/ssi_status/ssi_int/ssi_ptr/ssi_utime/ssi_stime/ssi_addr/ssi_addr_lsb,
    _pad: [u8; 46],      // 总长 128
}
pub struct Signalfd {
    mask: RwLock<SignalSet>,
    non_blocking: AtomicBool,
    poll_rx: PollSet,
}
```

`const _: [(); 128] = [(); size_of::<SignalfdSiginfo>()];` 在编译期校验布局。

## 3. 关键流程

```text
new(mask)                → 创建；update_mask(mask) → 写 mask + poll_rx.wake()
has_pending_signals()    → current().as_thread().signal.pending() & mask 非空
dequeue_signal()         → current().as_thread().signal.dequeue_signal(&mask)
read(dst)
  dst.remaining_mut() < 128 → InvalidInput
  block_on(poll_io(self, IN, nonblocking, || {
      dequeue_signal() 有 → 转 SignalfdSiginfo::from_signal_info → dst.write(bytes)
                            还有待处理 → poll_rx.wake()；Ok(128)
      无 → Err(WouldBlock)
  }))
write(...)               → BadFileDescriptor（只读）
poll()                   → IN = has_pending_signals()
```

`from_signal_info` 目前只填 `ssi_signo/ssi_errno/ssi_code`，其余为 0（`ssi_fd = -1`）。

## 4. 不变量与约束

- **C13**：阻塞语义走 `poll_io`；非阻塞时无信号立即 `WouldBlock`。
- **C17**：`has_pending_signals`/`dequeue_signal` 操作的是**当前任务**的线程信号管理器；signalfd 的读取必须发生在目标线程上下文（syscall 天然满足）。
- 结构布局必须严格 128 字节（编译期断言）；字段顺序/类型不得改，否则 `ssi_*` 偏移错位。
- `write` 返回 `BadFileDescriptor`（Linux 返回 `EINVAL`，当前实现差异已知）。
- `mask` 用 `RwLock` 保护，`sys_signalfd4(fd != -1)` 可更新。

## 5. 与其他模块的关系

| 模块 | 关系 |
|---|---|
| `syscall/fs/signalfd.rs` | `sys_signalfd4`：新建/更新、flags、cloexec |
| `syscall/signal.rs` | `check_sigset_size`（mask 大小校验） |
| `task/signal.rs` | 线程信号管理器 |
| `axpoll` | 等待 |

## 6. 已知局限 / TODO

- `ssi_pid/ssi_uid/ssi_ptr` 等未填（`SignalInfo` 有部分信息但未映射）。
- `write` 错误码与 Linux 不同。
- 未处理 `SIGKILL/SIGSTOP` 不可屏蔽的语义（依赖 starry_signal）。
- 没有 `signalfd` 的 `close-on-exec` 特殊处理（由 fd 表统一处理）。

## 7. 变更影响面

改本文件 → `syscall/fs/signalfd.rs.md`、`task/signal.rs.md`、`CONSTRAINTS.md` C13/C17。
