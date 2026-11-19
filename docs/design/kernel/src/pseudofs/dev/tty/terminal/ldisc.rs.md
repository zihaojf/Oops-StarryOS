# `kernel/src/pseudofs/dev/tty/terminal/ldisc.rs`

> 对应源码：`kernel/src/pseudofs/dev/tty/terminal/ldisc.rs`
> 所属子系统：终端行规程（line discipline）
> 依赖：`ringbuf`（`StaticRb<u8, 80>`）、`axpoll`、`axtask::future`、`linux_raw_sys::general::{ICRNL, IGNCR, ISIG, ECHO...}`、`starry_signal`
> 被依赖：`pseudofs/dev/tty/{mod,ntty,pty}.rs`、`terminal/{mod,termios}.rs`

## 1. 文件职责

实现串口/pty 的输入处理与读取：

- `Processor` 三种模式（`Manual/External/None`）决定输入从哪里、何时被处理；
- 行编辑（canonical）：`VKILL`/`VERASE`/`VEOF`/行结束符；
- 信号生成：`VINTR`→SIGINT、`VQUIT`→SIGQUIT（发给前台进程组）；
- 回显（`ECHO`/`ECHOCTL`/`ECHOK`）；
- 读取语义：canonical 模式按行返回，非 canonical 按 `VMIN`；
- `drain_input`（`TCSETSF` 用）。

## 2. 关键类型

```rust
const BUF_SIZE: usize = 80;
type ReadBuf = Arc<ringbuf::StaticRb<u8, BUF_SIZE>>;

pub enum ProcessMode {
    Manual,                              // 只在 read 时处理
    External(Box<dyn Fn(Waker) + Send + Sync>),  // 独立 tty-reader 任务
    None(Arc<PollSet>),                  // pty 主端：不处理行规程
}
pub struct TtyConfig<R, W> { reader: R, writer: W, process_mode: ProcessMode }
pub trait TtyRead { fn read(&mut self, buf: &mut [u8]) -> usize }
pub trait TtyWrite { fn write(&self, buf: &[u8]) }

struct InputReader<R, W> { terminal, reader, writer, buf_tx, read_buf, read_range, line_buf, line_read, clear_line_buf }
struct SimpleReader<R> { reader, read_buf, buf_tx }
enum Processor<R, W> { Manual(InputReader<R,W>), External(Arc<PollSet>), None(SimpleReader<R>, Arc<PollSet>) }
pub struct LineDiscipline<R, W> { terminal, buf_rx, poll_tx, clear_line_buf, processor }
```

## 3. 关键流程

### 3.1 `InputReader::poll`

```text
若 clear_line_buf 置位 → line_buf.clear()
若无待处理输入 → reader.read(&mut read_buf)
term = terminal.load_termios()
loop {
  若有 line_read（正在把 line_buf 推入 buf_tx）：推完则清空并结束
  buf_tx 满 或 输入耗尽 → break
  取一字节 ch：
    若 '\r'：IGNCR → 丢弃；ICRNL → 变 '\n'
    check_send_signal(term, ch)        // ISIG + canonical 时发 SIGINT/SIGQUIT 给前台组
    若 echo() → output_char(term, ch)  // 回显（含 ECHOCTL 的 ^X）
    若 !canonical() → buf_tx.try_push(ch)
    否则（canonical）：
      ECHOK + ch==VKILL → line_buf.clear()
      ch==VERASE → line_buf.pop()
      是行结束符或 VEOF → （非 VEOF 才 push）若非空则 line_read = Some(0)
      可打印字符 → line_buf.push(ch)
}
返回 sent > 0
```

### 3.2 `SimpleReader::poll`（pty 主端）

把 `\n` 展开成 `\r\n` 后直接 push（不处理回显/信号）。

### 3.3 构造 `LineDiscipline::new`

- `ReadBuf::default().split()` 得到 `(buf_tx, buf_rx)`；
- `Manual` → `Processor::Manual(InputReader)`；
- `External(register)` → 起 `tty-reader` 任务：循环 `reader.poll()` → `poll_rx.wake()`，注册 `poll_tx` 与外部 waker；
- `None(poll_rx)` → `Processor::None(SimpleReader, poll_rx)`。

### 3.4 `read(buf)`

```text
buf 空 → Ok(0)
None 模式 → buf_rx.pop_slice；0 → WouldBlock
term = terminal.termios.lock().clone()
vmin = canonical ? 1 : (VTIME>0 ? todo!() : VMIN)
buf.len() < vmin → WouldBlock
block_on(poll_io(WaitPollable(Some/None), IN, false, || {
    total += buf_rx.pop_slice(...); poll_tx.wake();
    total >= vmin ? Ok(total) : Err(WouldBlock)
}))
```

- `WaitPollable`：`Manual` 无 poll set → 立即 `wake_by_ref()`（自旋式轮询）；`External` 注册 `poll_rx`。
- `VTIME > 0` 会 `todo!()` panic（**已知未实现**）。

### 3.5 `poll_read` / `register_rx_waker`

- `poll_read`：`Manual/None` 时主动 poll 一次；返回 `!buf_rx.is_empty()`。
- `register_rx_waker`：`Manual` → 立即 wake；`External/None` → 注册对应 `PollSet`。

## 4. 不变量与约束

- **`VTIME != 0` 会 panic**：非 canonical 模式下 `termios` 的 `VTIME` 非零时 `todo!()`；调用方（glibc/musl 的 `cfmakeraw` 等）可能设置它，需要改成支持或明确返回错误。
- `buf_rx` 容量 80 字节：块设备/慢消费者下会丢数据（`try_push` 失败被忽略）；pty 主端同。
- canonical 模式必须读到整行才返回（`vmin = 1` 但靠 `line_read` 阻塞到行结束）；`read` 的 `poll_io` 循环保证。
- `check_send_signal` 只在 `canonical && ISIG` 时发信号；非 canonical 的 Ctrl+C 不会中断进程（与 Linux 一致？Linux 在 raw 模式下 ISIG 也可关闭——这里先看 ISIG）。
- `output_char` 对未知控制字符 `warn!`；`\n` 只输出 `\n`（不做 `ONLCR`，因为 `termios` 的 oflag 未用于输出）。
- `drain_input` 清 `buf_rx` 并标记清行缓冲（下一次 poll 生效）。
- `External` 任务的退出：`tty-reader` 在 tty 释放时仍持有 `Arc<PollSet>`/`Arc<Terminal>`，会泄漏一个阻塞任务（当前没有取消机制）。

## 5. 与其他模块的关系

| 模块 | 关系 |
|---|---|
| `terminal/mod.rs` | `Terminal`/`load_termios` |
| `terminal/termios.rs` | `Termios2` 的 `has_iflag/canonical/echo/special_char/signo_for` |
| `terminal/job.rs` | `foreground()` 用于发信号 |
| `pseudofs/dev/tty/mod.rs` | `TtyConfig`/`read_at`/`poll` |
| `pseudofs/dev/tty/{ntty,pty}.rs` | 提供 reader/writer 与 `ProcessMode` |
| `task/signal.rs` | `send_signal_to_process_group` |
| `axpoll` | `PollSet`/`poll_io` |

## 6. 已知局限 / TODO

- `VTIME > 0` 未实现（`todo!()`）。
- 输出不处理 `OPOST/ONLCR`（\n 到 \r\n 只在 pty 主端 SimpleReader 里做）。
- 输入缓冲 80 字节，易丢数据。
- `External` 任务无取消机制。
- 无 `IEXTEN` 的 `VLNEXT/VWERASE/VREPRINT` 处理（只在 termios 里定义了字符）。
- 无 `ISIG` 之外的信号（`VSUSP`→SIGTSTP 未实现）。
- `IGNCR/ICRNL` 之外的行结束符（`VEOL/VEOL2`）由 `term.is_eol` 处理，但 `output_char` 不区分。

## 7. 变更影响面

改本文件 → `pseudofs/dev/tty/{mod,ntty,pty}.rs.md`、`terminal/{mod,termios,job}.rs.md`、`task/signal.rs.md`、`file/mod.rs.md`（tty 的 `FileLike` 行为）。
