# `kernel/src/syscall/task/wait.rs`

> 对应源码：`kernel/src/syscall/task/wait.rs`
> 所属子系统：子进程回收（wait4）
> 依赖：`starry_process::{Pid, Process}`、`axtask::future::{block_on, interruptible}`、`starry_vm::{VmMutPtr, VmPtr}`、`linux_raw_sys::general::{WNOHANG, ...}`
> 被依赖：`syscall/task/mod.rs`、`syscall/mod.rs`

## 1. 文件职责

实现 `wait4(pid, exit_code, options)`（`sys_waitpid`）：

1. 按 `pid` 选择等待目标（任意/指定 pid/进程组）；
2. 在子进程中找 zombie，回收并写退出码；
3. 无可回收子进程时阻塞在 `child_exit_event`，被信号打断则**重试**。

## 2. 关键流程

```text
options = WaitOptions::from_bits_truncate
pid 解析：-1 → Any；0 → Pgid(本组)；>0 → Pid；<0 → Pgid(-pid)
children = proc.children().filter(pid.apply)
若 children 为空 → ECHILD
check_children = || {
    if 有 zombie child:
        !WNOWAIT → child.free()
        写 exit_code（nullable）
        Ok(Some(child.pid()))
    else if WNOHANG → Ok(Some(0))
    else Ok(None)
}
loop {
    match block_on(interruptible(poll_fn(|cx| {
        match check_children().transpose() {
            Some(res) → Ready(res)
            None → proc_data.child_exit_event.register(cx.waker()); Pending
        }
    }))) {
        Ok(res) → return res,
        Err(_) → continue,          // 被信号打断 → 重试（不返回 EINTR）
    }
}
```

## 3. 关键设计点

- **信号打断重试**：注释说明 `shell`/`timeout` 依赖"waitpid 不因 SIGCHLD 返回 EINTR"，所以 `Err` 时 `continue` 而不是返回 `Interrupted`。
- **`child.free()`**：回收 `Process`（zombie → 释放）；`WNOWAIT` 时不回收。
- **`child_exit_event`**：`do_exit` 里父进程的 `child_exit_event.wake()`。
- `WaitOptions` 包含 `WUNTRACED/WCONTINUED/WNOTHREAD/WALL/WCLONE`，但当前只用 `WNOHANG/WNOWAIT`（`WALL/WCLONE` 注释说明因 `ProcessData` 可能先于 `Process` drop 而暂不支持）。
- **`pid` 解析**：`0` 表示"同进程组"（`Pgid(本组)`）。

## 4. 不变量与约束

- **C6**：`exit_code` 经 `nullable!` + `vm_write`。
- **C16**：父进程通知依赖 `do_exit` 的 `child_exit_event.wake()`；本文件只等待。
- `children` 是 `proc.children()` 的**快照**：等待期间新 fork 的子进程不会被本调用回收（Linux 同样如此）。
- `WNOHANG` 无 zombie 时返回 0（不是错误）。
- `ECHILD` 只在**初始** children 为空时返回；等待期间子进程全部退出后仍会等到 zombie（正确）。
- `child.free()` 必须在写 `exit_code` 之前或之后均可（`free()` 后 `child` 的 `Arc` 仍在本作用域，`exit_code` 已读取？——代码先 `free()` 再 `child.exit_code()`，依赖 `free()` 不销毁对象本身，只标记回收）。
- 循环重试不会无限忙等：`interruptible` 返回 Err 通常意味着有信号待处理，`task/user.rs` 随后会处理。

## 5. 与其他模块的关系

| 模块 | 关系 |
|---|---|
| `task/ops.rs` | `do_exit` 的父进程通知 |
| `task/mod.rs` | `child_exit_event` |
| `starry_process::Process` | `children/is_zombie/free/exit_code/pid` |
| `syscall/task/exit.rs` | `exit_code` 的产生 |
| `syscall/task/job.rs` | `pgid` 语义 |
| `syscall/task/mod.rs` | 门面 |

## 6. 已知局限 / TODO

- 不支持 `WUNTRACED/WCONTINUED`（无 stop/continue）。
- `WALL/WCLONE` 注释说明暂不支持（clone 子进程的过滤不完整）。
- `waitid` 未实现。
- 被信号打断后无条件重试，可能吞掉本该返回 EINTR 的场景（注释说明为兼容 shell）。
- `WNOWAIT` 只影响本次是否 `free()`，zombie 仍保留（正确）。

## 7. 变更影响面

改本文件 → `task/ops.rs.md`、`task/mod.rs.md`、`syscall/task/{exit,job}.rs.md`、`CONSTRAINTS.md` C16。
