# `kernel/src/pseudofs/dev/tty/terminal/mod.rs`

> 对应源码：`kernel/src/pseudofs/dev/tty/terminal/mod.rs`
> 所属子系统：终端核心状态
> 依赖：`kspin::SpinNoPreempt`、`bytemuck`、子模块 `job/ldisc/termios`
> 被依赖：`pseudofs/dev/tty/{mod,ntty,pty}.rs`、`terminal/ldisc.rs`

## 1. 文件职责

定义 tty 的共享状态 `Terminal`，一个 tty 对（pty 主从）共享一个：

```rust
#[repr(C)] #[derive(AnyBitPattern)]
pub struct WindowSize { ws_row, ws_col, ws_xpixel, ws_ypixel: u16 }

pub struct Terminal {
    pub job_control: job::JobControl,                       // 会话/前台进程组
    pub window_size: SpinNoPreempt<WindowSize>,             // 默认 28x110
    pub termios: SpinNoPreempt<Arc<termios::Termios2>>,     // 用 Arc 便于无锁快照
    pub pty_number: AtomicU32,                              // /dev/pts/N 的 N
}
pub fn load_termios(&self) -> Arc<Termios2>;                // 快照
```

## 2. 设计要点

- **`Arc<Termios2>`**：`load_termios()` 返回 `Arc` 快照，读者无需持锁；写者 `TCSETS*` 用 `*self.termios.lock() = Arc::new(new)` 原子替换。这是 `SpinNoPreempt` + `Arc` 的经典组合，避免读路径持锁。
- **`SpinNoPreempt`**：终端状态在中断上下文（tty 中断）也可能被访问，用不可抢占自旋锁。
- **`WindowSize`** 保持 `#[repr(C)]` 与 Linux `winsize` 一致（4 个 u16）。
- `pty_number` 用 `AtomicU32`（`TIOCGPTN` 读）。

## 3. 默认值

| 字段 | 默认 |
|---|---|
| `window_size` | `ws_row=28, ws_col=110` |
| `termios` | `Termios2::default()`（ICANON/ECHO/ISIG…，见 `termios.rs`） |
| `pty_number` | 0（pty 创建后由 `pts::add_slave` 设置） |
| `job_control` | 空（无会话/无前台） |

## 4. 不变量与约束

- **同一个 pty 对的主从必须共享 `Terminal`**（`pty::create_pty_pair` 里 `terminal.clone()`）；否则 termios/winsize 不同步。
- `load_termios()` 返回的是快照：调用方看到的是某一时刻的值；不要在它上面写回。
- `termios` 的替换必须整体 `Arc::new`（不要在旧 `Arc` 上 `DerefMut`，因为它是共享的）。
- `WindowSize` 字段顺序不可改（`TIOCGWINSZ/TIOCSWINSZ` 直接按结构写用户内存）。
- `SpinNoPreempt` 内不得阻塞（C20）。
- `job_control` 内部有自己的锁，与 `terminal` 的锁无嵌套要求。

## 5. 与其他模块的关系

| 模块 | 关系 |
|---|---|
| `terminal/job.rs` | `JobControl`（会话/前台组） |
| `terminal/ldisc.rs` | 行规程读 `load_termios()` |
| `terminal/termios.rs` | `Termios/Termios2` |
| `pseudofs/dev/tty/mod.rs` | `ioctl` 读写这些字段；`bind_to` 调 `job_control` |
| `pseudofs/dev/tty/pty.rs` | 主从共享 |
| `pseudofs/dev/tty/ntty.rs` | `N_TTY` 的 `Terminal` |

## 6. 已知局限 / TODO

- 无 `termiox`、无 `devpts` 的 per-pty 选项。
- `WindowSize` 默认值硬编码（28x110 像 80x25 的变体）。
- 终端状态的"锁粒度"是每个字段独立，跨字段一致性（如同时读 termios 和 winsize）无法保证。

## 7. 变更影响面

改本文件 → `terminal/{job,ldisc,termios}.rs.md`、`pseudofs/dev/tty/{mod,ntty,pty}.rs.md`。
