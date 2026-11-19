# `kernel/src/syscall/fs/signalfd.rs`

> 对应源码：`kernel/src/syscall/fs/signalfd.rs`
> 所属子系统：signalfd syscall
> 依赖：`file::{Signalfd, add_file_like, FileLike}`、`syscall/signal::check_sigset_size`、`starry_signal::SignalSet`、`starry_vm::VmPtr`
> 被依赖：`syscall/fs/mod.rs`

## 1. 文件职责

实现 `signalfd4(fd, mask, sigsetsize, flags)`：

```rust
const SFD_CLOEXEC: u32 = O_CLOEXEC;
const SFD_NONBLOCK: u32 = O_NONBLOCK;
bitflags! { struct SignalfdFlags: u32 { CLOEXEC, NONBLOCK } }

check_sigset_size(sigsetsize)?;
flags = from_bits(flags).ok_or(InvalidInput)?;     // 严格
if fd != -1 && flags.contains(CLOEXEC) → InvalidInput
mask = vm_read_uninit()?.assume_init();            // 先读 mask
if fd != -1:
    signalfd = Signalfd::from_fd(fd)?;
    signalfd.update_mask(mask); set_nonblocking(...)
    return Ok(fd)
signalfd = Signalfd::new(mask); set_nonblocking(...)
add_file_like(signalfd, CLOEXEC)
```

## 2. 语义要点

- `fd == -1` → 新建；否则更新已有 signalfd 的 mask（并返回同一 fd）。
- `fd != -1` 时不允许 `CLOEXEC`（Linux 语义：已有 fd 的 cloexec 不能用 signalfd 改）。
- `sigsetsize` 必须 0 或 `size_of::<SignalSet>()`。
- mask 先读后判断：即使要更新已有 fd，也必须先读（避免后续校验失败时 mask 未读？实际上顺序是为了一致性）。
- `SignalfdFlags::from_bits` 严格：未知位 `InvalidInput`。

## 3. 不变量与约束

- **C12/C13**：`Signalfd` 的阻塞读在 `file/signalfd.rs` 实现；本文件只处理参数。
- **C24**：`sigsetsize` 必须校验；`mask` 必须 `vm_read`（不能裸读）。
- `fd != -1` 时 `Signalfd::from_fd` 失败（非 signalfd）→ `InvalidInput`（默认 downcast 错误）。
- 返回的 fd 在 `fd == -1` 时由 `add_file_like` 分配；`fd != -1` 时原样返回。
- `SFD_*` 值必须与 `O_CLOEXEC/O_NONBLOCK` 相同（Linux 就是这样定义的）。

## 4. 与其他模块的关系

| 模块 | 关系 |
|---|---|
| `file/signalfd.rs` | `Signalfd` 实现 |
| `syscall/signal.rs` | `check_sigset_size` |
| `file/mod.rs` | `add_file_like`/`from_fd` |
| `task/signal.rs` | 读取线程待处理信号 |
| `syscall/io_mpx/*` | 通过 `Pollable` 等待 |

## 5. 已知局限 / TODO

- 未校验 `fd != -1` 时传入的 `mask` 是否合法信号集（依赖 `SignalSet` 位图）。
- 无 `signalfd`（旧 syscall）实现（只有 signalfd4）。
- `write` 语义错误（见 `file/signalfd.rs`）。

## 6. 变更影响面

改本文件 → `file/signalfd.rs.md`、`syscall/signal.rs.md`、`syscall/fs/mod.rs.md`、`CONSTRAINTS.md` C12/C24。
