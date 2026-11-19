# `kernel/src/pseudofs/dev/tty/ntty.rs`

> 对应源码：`kernel/src/pseudofs/dev/tty/ntty.rs`
> 所属子系统：控制台 tty（`/dev/console`、`/dev/tty`）
> 依赖：`lazy_static`、`axhal::console`、`axtask::future::register_irq_waker`
> 被依赖：`pseudofs/dev/mod.rs`（`/dev/console`、`/dev/tty`）、`entry.rs`（`N_TTY.bind_to`）、`file/mod.rs`（`add_stdio`）

## 1. 文件职责

构造**全局唯一**的控制台 tty：

```rust
pub type NTtyDriver = Tty<Console, Console>;   // reader/writer 都是串口
pub struct Console;                            // TtyRead: axhal::console::read_bytes
                                               // TtyWrite: axhal::console::write_bytes
lazy_static! { pub static ref N_TTY: Arc<NTtyDriver> = new_n_tty(); }
```

## 2. 关键流程

```rust
fn new_n_tty() -> Arc<NTtyDriver> {
    Tty::new(Arc::default() /* Terminal */, TtyConfig {
        reader: Console,
        writer: Console,
        process_mode: if let Some(irq) = axhal::console::irq_num() {
            ProcessMode::External(Box::new(move |waker| register_irq_waker(irq, &waker)))
        } else {
            ProcessMode::Manual
        },
    })
}
```

- **`ProcessMode::External`**：为串口中断注册 waker，由 `LineDiscipline` 起 `tty-reader` 任务在中断到来时处理输入。
- **`ProcessMode::Manual`**：无中断号时退化为"只在 read 时处理输入"（无法用 Ctrl+C 打断前台程序，除非它正阻塞在 read）。

## 3. 不变量与约束

- **全局单例**：`N_TTY` 是 `lazy_static`；`entry.rs` 用它给 init 绑定控制终端，`add_stdio` 打开 `/dev/console` 也复用同一个 `Terminal`（通过 devfs 的 `Device`）。
- `Console` 的 read/write 直接调 `axhal::console`；串口是阻塞的（读不到返回 0）。
- `ProcessMode::External` 的闭包必须能被 `register_irq_waker` 反复调用（注册 waker）；`irq_num()` 在启动后不变。
- `N_TTY` 的 `Terminal` 是 `Arc::default()`（`Terminal::default()`：job_control 空、winsize 28x110、termios 默认）。
- 不要在 `N_TTY` 上做 `bind_to` 之外的会话操作（会与 init 的会话冲突）。

## 4. 与其他模块的关系

| 模块 | 关系 |
|---|---|
| `pseudofs/dev/tty/mod.rs` | `Tty::new`/`bind_to` |
| `pseudofs/dev/tty/terminal/ldisc.rs` | `ProcessMode::External` 的 reader 任务 |
| `pseudofs/dev/mod.rs` | `/dev/console` 与 `/dev/tty` 设备 |
| `entry.rs` | `N_TTY.bind_to(&proc)` |
| `file/mod.rs` | `add_stdio` 打开 `/dev/console` |
| `axhal::console` | 串口读写与中断号 |

## 5. 已知局限 / TODO

- 无 `/dev/tty0`/虚拟终端切换。
- `Manual` 模式下无法中断阻塞在非 read 的进程（注释说明）。
- 只有一个控制台（无多 tty）。
- `irq_num()` 为 None 的平台（如某些虚拟平台）会退化。

## 6. 变更影响面

改本文件 → `pseudofs/dev/tty/mod.rs.md`、`pseudofs/dev/tty/terminal/ldisc.rs.md`、`pseudofs/dev/mod.rs.md`、`entry.rs.md`、`file/mod.rs.md`。
