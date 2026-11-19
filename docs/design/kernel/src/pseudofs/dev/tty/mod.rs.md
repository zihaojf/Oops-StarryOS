# `kernel/src/pseudofs/dev/tty/mod.rs`

> 对应源码：`kernel/src/pseudofs/dev/tty/mod.rs`
> 所属子系统：tty（终端）
> 依赖：`axpoll`、`starry_process::{Process, Session}`、`linux_raw_sys::ioctl::*`
> 被依赖：`pseudofs/dev/mod.rs`、`file/mod.rs`（`/dev/console`）、`syscall/fs/fd_ops.rs`（ptmx/current-tty）

## 1. 文件职责

tty 子系统的公共外壳：

1. `Tty<R, W>`：泛型 tty 设备（reader/writer + `LineDiscipline` + `Terminal`）；
2. `bind_to`：把 tty 绑定为进程会话的控制终端；
3. `DeviceOps` 实现：读（前台进程组检查）、写、ioctl（termios/pgrp/winsize/pty 号/ctty）；
4. `Pollable` 实现：就绪条件 = job control + ldisc；
5. `create_pty_master`：创建 pty 对并把 slave 加到 `/dev/pts`；
6. `CurrentTty`：占位设备（打开时由 `fd_ops` 替换为真实终端）。

## 2. 关键类型

```rust
pub struct Tty<R, W> {
    this: Weak<Self>,                         // 自引用，供 TIOCSCTTY/TIOCNOTTY
    terminal: Arc<Terminal>,
    ldisc: Mutex<LineDiscipline<R, W>>,
    writer: W,
    is_ptm: bool,                             // 主端不做前台检查
}
pub struct CurrentTty;                        // 打开即替换
```

## 3. 关键流程

### 3.1 `bind_to(proc)`

```text
pg = proc.group()
若 pg.session().sid() != proc.pid() → OperationNotPermitted   // 只有会话首进程能设控制终端
assert!(session.set_terminal_with(|| { job_control.set_session(&session); self.clone() }))
job_control.set_foreground(&pg)?            // 前台进程组 = 自己
```

### 3.2 `read_at`

```text
block_on(poll_io(job_control, IN, false, || {
    if is_ptm || job_control.current_in_foreground() {
        ldisc.lock().read(buf)
    } else { Err(WouldBlock) }              // 后台进程读终端 → 阻塞（Linux 会发 SIGTTIN）
}))
```

### 3.3 `ioctl`

| cmd | 行为 |
|---|---|
| `TCGETS` / `TCGETS2` | 写 `Termios`/`Termios2` |
| `TCSETS/TCSETSF/TCSETSW` 与 `2` 变体 | 从用户读并替换 termios；`TCSETSF*` 同时 `drain_input()` |
| `TIOCGPGRP` | 写前台进程组 pgid；无前台 → `NoSuchProcess` |
| `TIOCSPGRP` | 设当前进程组为前台（校验同一会话） |
| `TIOCGWINSZ` / `TIOCSWINSZ` | 窗口大小 |
| `TIOCSPTLCK` | 无操作 |
| `TIOCGPTN` | 写 pty 号 |
| `TIOCSCTTY` | `bind_to(current.proc)` |
| `TIOCNOTTY` | `session.unset_terminal(self)`；失败 `warn!` |
| 其他 | `NotATty` |

### 3.4 `Pollable`

```text
poll: events = OUT | job_control.poll(); 若 is_ptm 或 IN 则 set(IN, ldisc.poll_read())
register: 非 ptm 注册 job_control；IN 时注册 ldisc 的 rx waker
```

## 4. 不变量与约束

- **会话控制**：`bind_to` 只能由会话首进程调用，且会话不得已有终端（`set_terminal_with` 断言）；`TIOCSCTTY` 走同一路径。
- **`is_ptm`** 由 `ProcessMode::None(_)` 推断：主端不做"前台进程组"限制，否则向 pty 写数据会被自己阻塞。
- **`this: Weak<Self>`** 用 `Arc::new_cyclic` 初始化；`TIOCSCTTY/TIOCNOTTY` 依赖它，不能改成 `Option`。
- 读终端时若不在前台，返回 `WouldBlock`（Linux 发 `SIGTTIN`；这里不发信号，是已知差异）。
- `TCSETSF*` 的 `drain_input` 会丢弃未读输入（`buf_rx.clear()` + 清行缓冲）。
- `ioctl` 的 `Ok(0)` 返回值：所有命令成功都返回 0。
- `CurrentTty` 的 `read_at/ioctl` 是 `unreachable!()`：**只有在打开时未被 `fd_ops` 替换的情况下才会 panic**；新增"打开 `/dev/tty`"的路径必须走替换逻辑。

## 5. 与其他模块的关系

| 模块 | 关系 |
|---|---|
| `pseudofs/dev/tty/{ntty,pty,ptm,pts}.rs` | 构造具体 tty 设备 |
| `pseudofs/dev/tty/terminal/*` | `Terminal/JobControl/LineDiscipline/Termios` |
| `pseudofs/device.rs` | `DeviceOps`/`as_pollable` |
| `syscall/fs/fd_ops.rs` | 打开 `/dev/ptmx` 创建 pty；打开 `/dev/tty` 替换为当前终端 |
| `entry.rs` | `N_TTY.bind_to(&init_proc)` |
| `starry_process` | `Process/ProcessGroup/Session` |
| `pseudofs/proc.rs` | `/proc/<pid>/stat` 的 tty 字段（未填充） |

## 6. 已知局限 / TODO

- 后台读不发 `SIGTTIN`（只阻塞）。
- `TCSETSW/TCSETSF` 的"等待输出排空"未实现（`// TODO: drain output?`）。
- `TIOCNOTTY` 未发 `SIGHUP/SIGCONT`（注释 TODO）。
- 无 `TIOCSTI`/`TIOCLINUX`/`FIOASYNC` 等。
- `CurrentTty` 的占位实现依赖 `fd_ops` 替换，脆弱。

## 7. 变更影响面

改本文件 → `pseudofs/dev/tty/*.md`、`pseudofs/device.rs.md`、`syscall/fs/fd_ops.rs.md`、`entry.rs.md`、`file/mod.rs.md`、`ARCHITECTURE.md#8`。
