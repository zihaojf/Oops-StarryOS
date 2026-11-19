# `kernel/src/task/stat.rs`

> 对应源码：`kernel/src/task/stat.rs`
> 所属子系统：任务自省（`/proc/<pid>/stat`）
> 依赖：`axtask::{TaskInner, TaskState}`、`starry_process`、`starry_signal`
> 被依赖：`task/mod.rs`（重导出）、`pseudofs/proc.rs`

## 1. 文件职责

定义与 Linux `/proc/[pid]/stat` **字段顺序完全一致**的 `TaskStat` 结构，并实现：

- `TaskStat::from_thread(&TaskInner) -> AxResult<Self>`：从任务快照填充可获取的字段；
- `Display`：按 Linux 格式输出一行（空格分隔）。

## 2. 字段与 Linux 对应

共 52 个字段，顺序与 `man 5 proc_pid_stat` 一致：`pid comm state ppid pgrp session tty_nr tpgid flags minflt ... exit_code`。

实际填充的字段（其余 `Default`）：

| 字段 | 来源 |
|---|---|
| `pid` | `proc.pid()` |
| `comm` | `task.name()` 截断到 16 字节 |
| `state` | `TaskState::{Running,Ready}→'R'`，`Blocked→'S'`，`Exited→'Z'` |
| `ppid` | `proc.parent().map_or(0, pid)` |
| `pgrp` | `proc.group().pgid()` |
| `session` | `proc.group().session().sid()` |
| `num_threads` | `proc.threads().len()` |
| `exit_signal` | `proc_data.exit_signal.unwrap_or(SIGCHLD) as u8` |
| `exit_code` | `proc.exit_code()` |

## 3. 关键设计点

- `comm` 必须 ≤ 16 字节：`comm[..comm.len().min(16)]`，避免越界。
- `state` 只映射三种：没有 `T`（stopped）/`D`（uninterruptible）等状态。
- `Display` 用一次 `writeln!` 拼接全部字段，避免多次 I/O；`comm` 已保证无空格/括号（内核任务名可控）。
- `TaskStat::from_thread` 返回 `AxResult`，但当前实现不会失败（为将来 `proc` 查询失败预留）。

## 4. 不变量与约束

- **字段顺序不可变**：`top`/`ps`/`busybox` 按列数解析；新增/删除字段会破坏工具兼容（应改为追加到末尾且同步 Linux 新版本）。
- `comm` 中的 `)` 会被解析器误判；当前只截断不转义（与 Linux 行为一致：Linux 也不转义，但保证名字里没有括号——用户可通过 `prctl(PR_SET_NAME)` 设置，存在被 `)` 截断的风险）。
- `exit_signal` 用 `u8`，`None` 时按 `SIGCHLD` 显示（与 Linux init/内核线程一致）。
- `Display` **不要**加额外换行（`writeln!` 已含一个）。

## 5. 与其他模块的关系

- `pseudofs/proc.rs` 的 `/proc/<pid>/stat` 用 `SimpleFile::new_regular(fs, move || Ok(format!("{}", TaskStat::from_thread(&task)?).into_bytes()))`。
- `axtask::TaskInner` 提供 `name/state/id`；`starry_process::Process` 提供 pid/ppid/组/会话/线程数/退出码。
- `task/mod.rs` 的 `Thread.proc_data` 提供 `exit_signal`。

## 6. 已知局限 / TODO

- 大量字段为 0（`utime/stime/vsize/rss/startcode...`），`top`/`ps` 显示信息不完整。
- `state` 无 `T/D/I` 等状态。
- `tty_nr/tpgid` 未填充（终端相关）。
- 未处理 `comm` 含空格/括号的转义。

## 7. 变更影响面

改本文件 → `pseudofs/proc.rs.md`（`stat` 输出）、`ARCHITECTURE.md#8`（procfs）。
