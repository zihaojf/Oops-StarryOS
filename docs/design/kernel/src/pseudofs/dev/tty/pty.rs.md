# `kernel/src/pseudofs/dev/tty/pty.rs`

> 对应源码：`kernel/src/pseudofs/dev/tty/pty.rs`
> 所属子系统：伪终端（pty 对）
> 依赖：`ringbuf::{HeapRb, Cons, Prod}`、`kspin::SpinNoPreempt`、`axpoll::PollSet`
> 被依赖：`pseudofs/dev/tty/mod.rs`（`create_pty_master`）、`ptm.rs`、`pts.rs`

## 1. 文件职责

实现 pty 的主/从一对 `Tty`，两侧通过两个环形缓冲互联：

```text
master.writer ──(master_to_slave)──► slave.reader
slave.writer  ──(slave_to_master)──► master.reader
```

## 2. 关键类型

```rust
const PTY_BUF_SIZE: usize = 4096;
type Buffer = Arc<HeapRb<u8>>;
pub struct PtyReader(Cons<Buffer>);                          // 从环读
#[derive(Clone)]
pub struct PtyWriter(Arc<SpinNoPreempt<Prod<Buffer>>>, Arc<PollSet>);  // 写入环并唤醒对端
pub type PtyDriver = Tty<PtyReader, PtyWriter>;
```

## 3. `create_pty_pair()`

```text
master_to_slave = HeapRb(4096); slave_to_master = HeapRb(4096)
poll_rx_slave / poll_rx_master = PollSet
terminal = Arc::new(Terminal::default())

master = Tty::new(terminal.clone(), TtyConfig {
    reader: PtyReader(slave_to_master),
    writer: PtyWriter(master_to_slave, poll_rx_slave),
    process_mode: ProcessMode::None(poll_rx_master),     // 主端不处理行规程
})
slave = Tty::new(terminal, TtyConfig {
    reader: PtyReader(master_to_slave),
    writer: PtyWriter(slave_to_master, poll_rx_master),
    process_mode: ProcessMode::External(|waker| poll_rx_slave.register(&waker)),
})
```

- 两侧共享同一个 `Terminal`（termios/winsize/job control 对主从一致）。
- **主端 `ProcessMode::None`**：不跑行规程，数据直通（读以 `SimpleReader` 方式把 `\n` 转 `\r\n`）。
- **从端 `ProcessMode::External`**：由 `poll_rx_slave` 驱动 `tty-reader` 任务处理输入（行编辑/信号）。

## 4. 不变量与约束

- **`PtyWriter::write` 不阻塞**：`push_slice` 写不下的部分被丢弃并 `warn!`；pty 的写端没有背压（阻塞在 `Tty::write_at` 之外，`write_at` 直接 `writer.write(buf); Ok(buf.len())`）。这意味着大量写入时数据会丢。
- `PtyWriter` 克隆时共享 `Prod` 与 `PollSet`：`Tty` 里 `writer` 是字段，`ldisc` 也持有同类型 writer（`TtyConfig.writer` 移交给 `InputReader` 后又被 `Tty.writer = config.writer.clone()` 复用）——两侧写入顺序由 `SpinNoPreempt` 保证。
- `PollSet` 的配对：`poll_rx_slave` 在 slave 的 `External` 里注册；`master` 写完 `master_to_slave` 后 `poll_rx_slave.wake()`；反之 `poll_rx_master`。
- 缓冲区固定 4K；`PTY_BUF_SIZE` 改大需注意内存（每个 pty 8K）。
- `create_pty_pair` 返回 `(master, slave)`；`Ptmx::create_pty` 把 slave 加入 `/dev/pts`，返回 master 作为打开的文件。

## 5. 与其他模块的关系

| 模块 | 关系 |
|---|---|
| `pseudofs/dev/tty/mod.rs` | `create_pty_master`（创建 + 注册 slave） |
| `pseudofs/dev/tty/ptm.rs` | `/dev/ptmx` 打开时创建 pty |
| `pseudofs/dev/tty/pts.rs` | slave 加入 `PTS_TABLE` |
| `pseudofs/dev/tty/terminal/ldisc.rs` | 行规程与 `ProcessMode` |
| `pseudofs/dev/tty/terminal/mod.rs` | 共享 `Terminal` |
| `syscall/fs/fd_ops.rs` | 打开 `/dev/ptmx` → `Ptmx::create_pty` |

## 6. 已知局限 / TODO

- 写满丢数据（无阻塞/无背压）。
- 缓冲区大小不可配（无 `TIOCPKT` 之外的控制）。
- 主端不处理行规程（符合 Linux，但 `ProcessMode::None` 的 `SimpleReader` 会把 `\n` 变 `\r\n`，与 Linux 主端原始模式略有差异）。
- 没有 `TIOCGPTN` 之外的 pty 专用 ioctl 支持（`TIOCSPTLCK` 是 no-op）。

## 7. 变更影响面

改本文件 → `pseudofs/dev/tty/{mod,ptm,pts}.rs.md`、`pseudofs/dev/tty/terminal/ldisc.rs.md`、`syscall/fs/fd_ops.rs.md`。
