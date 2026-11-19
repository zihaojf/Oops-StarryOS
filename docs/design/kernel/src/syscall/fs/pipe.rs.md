# `kernel/src/syscall/fs/pipe.rs`

> 对应源码：`kernel/src/syscall/fs/pipe.rs`
> 所属子系统：pipe syscall
> 依赖：`file::{Pipe, FileLike, close_file_like}`、`starry_vm::VmMutPtr`、`linux_raw_sys::general::{O_CLOEXEC, O_NONBLOCK}`
> 被依赖：`syscall/fs/mod.rs`

## 1. 文件职责

实现 `pipe2(fds, flags)`：

```rust
bitflags! { struct PipeFlags: u32 { CLOEXEC = O_CLOEXEC; NONBLOCK = O_NONBLOCK } }

pub fn sys_pipe2(fds: *mut [c_int; 2], flags: u32) -> AxResult<isize> {
    flags = from_bits_truncate(flags);   // 未知位只 warn
    (read_end, write_end) = Pipe::new();
    if NONBLOCK { 两端 set_nonblocking(true) }
    read_fd = read_end.add_to_fd_table(cloexec)?;
    write_fd = write_end.add_to_fd_table(cloexec)
        .inspect_err(|_| close_file_like(read_fd).unwrap())?;   // 失败回滚
    fds.vm_write([read_fd, write_fd])?;
    Ok(0)
}
```

## 2. 关键设计点

- **回滚**：写端登记失败时关闭已登记的读端，避免 fd 泄漏。
- **未知 flags**：`from_bits_truncate` + `warn!`（宽松），与 Linux 对未知位的容忍一致。
- `fds` 用 `*mut [c_int; 2]` + `vm_write`：一次性写两个 fd。
- `O_NONBLOCK` 同时作用于两端（Linux 语义）。
- `x86_64` 的 `pipe` syscall 在 `syscall/mod.rs` 里映射到 `sys_pipe2(fds, 0)`。

## 3. 不变量与约束

- **C13**：非阻塞语义由 `Pipe` 实现；本文件只设置标志。
- `add_to_fd_table` 的顺序必须是先读后写（fd 号通常 3/4）；调用方不保证顺序，但当前实现固定。
- `vm_write` 失败时**不回滚**已登记的两个 fd（Linux 也不回滚？实际上会关闭；当前是泄漏点，见局限）。
- `fds` 不能为空指针（`vm_write` 会返回 `AccessDenied`）。
- `PipeFlags` 的 `CLOEXEC/NONBLOCK` 值必须等于 `O_CLOEXEC/O_NONBLOCK`（不是 `EFD_*`）。

## 4. 与其他模块的关系

| 模块 | 关系 |
|---|---|
| `file/pipe.rs` | `Pipe::new`/`set_nonblocking`/`add_to_fd_table` |
| `file/mod.rs` | `close_file_like` |
| `syscall/fs/fd_ops.rs` | `F_GETPIPE_SZ/F_SETPIPE_SZ` |
| `syscall/fs/io.rs` | `splice` 使用 pipe 端 |
| `syscall/mod.rs` | 分派（`pipe` → `sys_pipe2(fds,0)`） |

## 5. 已知局限 / TODO

- `fds` 写失败时不关闭两个 fd（泄漏）。
- 未知 flags 被忽略而非拒绝。
- 没有 `pipe`（非 2）的独立实现（x86_64 用 0 flags）。

## 6. 变更影响面

改本文件 → `file/pipe.rs.md`、`syscall/fs/{fd_ops,io}.rs.md`、`syscall/mod.rs.md`。
