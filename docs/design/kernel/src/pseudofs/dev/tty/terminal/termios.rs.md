# `kernel/src/pseudofs/dev/tty/terminal/termios.rs`

> 对应源码：`kernel/src/pseudofs/dev/tty/terminal/termios.rs`
> 所属子系统：终端属性（termios）
> 依赖：`bytemuck::AnyBitPattern`、`linux_raw_sys::general::{tcflag_t, speed_t, ...}`、`starry_signal::Signo`
> 被依赖：`terminal/{mod,ldisc}.rs`、`pseudofs/dev/tty/mod.rs`（ioctl）

## 1. 文件职责

定义与 Linux ABI 一致的 `Termios`/`Termios2`，提供 flag 查询与特殊字符语义：

```rust
#[repr(C)] pub struct Termios {
    c_iflag: tcflag_t, c_oflag: tcflag_t, c_cflag: tcflag_t, c_lflag: tcflag_t,
    c_line: u8, c_cc: [u8; 19],
}
#[repr(C)] pub struct Termios2 { termios: Termios, c_ispeed: speed_t, c_ospeed: speed_t }
```

## 2. 默认值

| 字段 | 默认 |
|---|---|
| `c_iflag` | `ICRNL | IXON` |
| `c_oflag` | `OPOST | ONLCR` |
| `c_cflag` | `B38400 | CS8 | CREAD` |
| `c_lflag` | `ICANON|ECHO|ISIG|ECHOE|ECHOK|ECHOCTL|ECHOKE|IEXTEN` |
| `c_cc` | `VINTR=^C, VQUIT=^\ , VERASE=0x7f, VKILL=^U, VEOF=^D, VREPRINT=^R, VDISCARD=^O, VWERASE=^W, VLNEXT=^V`；`VEOL/VEOL2=0` |
| `c_ispeed/c_ospeed` | `B38400` |

## 3. 方法

| 方法 | 语义 |
|---|---|
| `special_char(index)` | `c_cc[index]` |
| `has_iflag/oflag/cflag/lflag(flag)` | 位测试 |
| `echo()` | `ECHO` |
| `canonical()` | `ICANON` |
| `contains_iexten()` | `IEXTEN` |
| `is_eol(ch)` | `\n`、`VEOL`；`IEXTEN` 时也认 `VEOL2` |
| `signo_for(ch)` | `VINTR→SIGINT`、`VQUIT→SIGQUIT`，其他 `None` |

`Termios2` 用 `Deref/DerefMut` 转发到内部 `Termios`，因此 `TCGETS2/TCSETS2` 可按 `Termios2` 布局读写。

## 4. 不变量与约束

- **`#[repr(C)]` 与字段顺序不可改**：用户态 `tcgetattr/tcsetattr` 直接按结构布局读写；`c_cc` 长度 19（NCCS）。
- **`Termios2` 的 `Deref`**：`tcgetattr` 写 `Termios`（不含 speed），`TCGETS2` 写完整 `Termios2`；两者不能混用。
- `c_cc` 的默认值必须在合法范围（`ctl(ch) = ch - 0x40`），否则用户态读到的特殊字符错误。
- `special_char(VEOF)` 默认 `^D`；`VKILL` 默认 `^U`——与 Linux 一致。
- `is_eol` 依赖 `IEXTEN` 才认 `VEOL2`；`VEOL` 默认 `0`，因此 `\0` 也会被当成行结束符（与 Linux 一致：`VEOL=0` 表示禁用，但 `is_eol` 没区分——**潜在行为**：输入 `\0` 会结行）。
- `signo_for` 只处理 `VINTR/VQUIT`；`VSUSP` 未映射。
- `Termios` 默认带 `IEXTEN` 但行规程未实现 `VLNEXT/VWERASE/VREPRINT`（见 `ldisc.rs`）。

## 5. 与其他模块的关系

| 模块 | 关系 |
|---|---|
| `terminal/mod.rs` | `Terminal.termios: SpinNoPreempt<Arc<Termios2>>` |
| `terminal/ldisc.rs` | `has_iflag/canonical/echo/special_char/is_eol/signo_for` |
| `pseudofs/dev/tty/mod.rs` | `TCGETS/TCGETS2/TCSETS*` 直接读写本结构 |
| `starry_signal` | `Signo` |

## 6. 已知局限 / TODO

- `VSUSP`/`VDISCARD` 等信号/控制字符未实现。
- `IEXTEN` 的扩展编辑（`VLNEXT` 字面下一字符、`VWERASE` 删词）未实现。
- `c_line` 恒 0（无线路规程切换）。
- `VEOL=0` 被当成有效行结束符（`is_eol` 未区分 0 表示禁用）。
- 无 `TCFLSH`/`TIOCM*`（modem 控制）。

## 7. 变更影响面

改本文件 → `terminal/{mod,ldisc}.rs.md`、`pseudofs/dev/tty/mod.rs.md`。
