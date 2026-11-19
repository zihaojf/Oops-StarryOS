# `kernel/src/file/pipe.rs`

> 对应源码：`kernel/src/file/pipe.rs`
> 所属子系统：pipe / FIFO
> 依赖：`ringbuf`（`HeapRb`）、`axpoll::{PollSet, Pollable}`、`axtask::future::{block_on, poll_io}`、`starry_signal`
> 被依赖：`file/mod.rs`（重导出）、`syscall/fs/pipe.rs`、`syscall/fs/fd_ops.rs`（F_GETPIPE_SZ）、`syscall/fs/io.rs`（splice）

## 1. 文件职责

实现匿名管道：一对 `Pipe`（读端/写端）共享 `Arc<Shared>`。

```rust
struct Shared {
    buffer: Mutex<HeapRb<u8>>,      // 初始 64KiB
    poll_rx: PollSet,               // 可读唤醒
    poll_tx: PollSet,               // 可写唤醒
    poll_close: PollSet,            // 任一端关闭唤醒
}
pub struct Pipe { read_side: bool, shared: Arc<Shared>, non_blocking: AtomicBool }
```

## 2. 关键流程

### 2.1 读

```text
若 !is_read → BadFileDescriptor；dst 已满 → Ok(0)
block_on(poll_io(self, IN, nonblocking, || {
    从 ring buffer 取 slices 写入 dst（left 写满再写 right）
    advance_read_index(count)
    count > 0 → poll_tx.wake(); Ok(count)
    else if closed() → Ok(0)                 // 写端全关闭 → EOF
    else Err(WouldBlock)
}))
```

### 2.2 写

```text
若 !is_write → BadFileDescriptor；src 为空 → Ok(0)
block_on(poll_io(self, OUT, nonblocking, || {
    if closed() { raise_pipe(); return Err(BrokenPipe) }     // 无读者 → SIGPIPE + EPIPE
    取空闲 slices 写入，advance_write_index
    写了 >0：
        poll_rx.wake()
        写满或非阻塞 → Ok(total)     // 注意：非阻塞下部分写也返回 Ok
    Err(WouldBlock)
}))
```

### 2.3 `closed()`

`Arc::strong_count(&self.shared) == 1`：只剩自己，说明对端已 drop。`Drop for Pipe` 唤醒 `poll_close`，让阻塞的读端立即醒来。

### 2.4 容量调整

`resize(new_size)`：向上取整到 4K、至少一页；小于已占用返回 `ResourceBusy`；换新 `HeapRb` 后按 `as_slices` 把旧数据 push 回。

### 2.5 `Pollable`

- `poll`：读端 set `IN = occupied>0`、`HUP = closed`；写端 set `OUT = vacant>0`。
- `register`：按事件注册 `poll_rx`/`poll_tx`，并总是注册 `poll_close`。

## 3. 不变量与约束

- **C13/C14**：阻塞读在写端关闭时返回 `Ok(0)`（EOF），阻塞写无读者时返回 `BrokenPipe` 并先发 `SIGPIPE`；`Drop` 必须 `poll_close.wake()`。
- `raise_pipe()` 用 `send_signal_to_process(pid, SIGPIPE)`；忽略 `SIGPIPE` 的进程仍会收到 `BrokenPipe`（符合 Linux）。
- 非阻塞写允许**部分写**返回（`written > 0` 即返回），调用方（`FileLike::write` → `sys_write`）会把它当作完整写；这是与 Linux 一致的行为（`write` 可短写）。
- 环形缓冲的 `vacant_slices_mut` 返回 `&mut [MaybeUninit<u8>]`，代码用裸指针 cast 成 `&mut [u8]`（工具链限制，注释说明）；不要删掉 `unsafe` 注释。
- `resize` 的 `new_size` 必须 4K 对齐（`div_ceil(4K).max(1)*4K`）；`F_SETPIPE_SZ` 传任意值会被规范化。
- pipe 的 `mode` 按端伪造（读 `0444`、写 `0222`）；`path` 是 `pipe:[ptr]`。

## 4. 与其他模块的关系

| 模块 | 关系 |
|---|---|
| `file/mod.rs` | `Pipe` 实现 `FileLike`；`from_fd` 用于 downcast |
| `syscall/fs/pipe.rs` | `sys_pipe2` 创建并登记两端 |
| `syscall/fs/fd_ops.rs` | `F_GETPIPE_SZ`/`F_SETPIPE_SZ` |
| `syscall/fs/io.rs` | `sys_splice` 检查 pipe 端方向 |
| `task/signal.rs` | `raise_pipe` → `send_signal_to_process` |
| `axio` | `IoDst/IoSrc` 的 `write/read` |

## 5. 已知局限 / TODO

- 无 `pipe2(O_DIRECT)`、无 `FIONREAD` 之外的高级 ioctl（`FIONREAD` 已实现）。
- 无命名管道（FIFO）文件系统节点；`S_IFIFO` 只在 stat 中伪造。
- `resize` 不是原子替换（换缓冲期间有锁，但读写者会看到短暂空缓冲——实际上锁保护了整个替换，安全）。
- 写端关闭后写入返回 `BrokenPipe`，但已缓冲数据仍可被读端读完（正确）。

## 6. 变更影响面

改本文件 → `file/mod.rs.md`、`syscall/fs/{pipe,fd_ops,io}.rs.md`、`task/signal.rs.md`、`CONSTRAINTS.md` C13/C14。
