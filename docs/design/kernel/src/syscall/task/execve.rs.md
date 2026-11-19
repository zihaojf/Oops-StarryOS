# `kernel/src/syscall/task/execve.rs`

> 对应源码：`kernel/src/syscall/task/execve.rs`
> 所属子系统：execve
> 依赖：`mm::{load_user_app, vm_load_string}`、`file::FD_TABLE`、`config::USER_HEAP_BASE`、`axfs::FS_CONTEXT`、`starry_vm::vm_load_until_nul`
> 被依赖：`syscall/task/mod.rs`、`syscall/mod.rs`

## 1. 文件职责

实现 `execve(path, argv, envp)`：装载新程序、替换地址空间、重置进程状态、关闭 CLOEXEC fd、更新入口/栈指针。

## 2. 关键流程

```text
path = vm_load_string(path)?
args = argv 空 ? [] : vm_load_until_nul(argv)?.map(vm_load_string)
envs = envp 空 ? [] : vm_load_until_nul(envp)?.map(vm_load_string)
若 proc.threads().len() > 1 → error!("multi-thread not supported") + WouldBlock
aspace = proc_data.aspace.lock()
(entry, sp) = load_user_app(&mut aspace, Some(path), &args, &envs)?
drop(aspace)
loc = FS_CONTEXT.resolve(&path)?
curr.set_name(loc.name())
*proc_data.exe_path.write() = loc.absolute_path()?.to_string()
*proc_data.cmdline.write() = Arc::new(args)
proc_data.set_heap_top(USER_HEAP_BASE)
*proc_data.signal.actions.lock() = Default::default()     // 重置信号处理
curr.as_thread().set_clear_child_tid(0)                    // 旧地址失效
关闭所有 cloexec fd
uctx.set_ip(entry); uctx.set_sp(sp)
Ok(0)
```

## 3. 不变量与约束

- **C8**：`aspace` 锁只在 `load_user_app` 期间持有；之后释放（`loc`/fd 操作不持锁）。
- **C17**：`signal.actions` 重置为默认（execve 语义）；`clear_child_tid` 清零（旧虚拟地址已失效）。
- **C12**：CLOEXEC fd 必须在 execve 时关闭；`fd_table.ids()` 收集后再 `remove`（避免迭代时修改）。
- **多线程限制**：`threads().len() > 1` 直接 `WouldBlock`（EAGAIN），未实现"杀死其他线程"。
- `load_user_app` 会 `uspace.clear()`（丢弃旧地址空间），再映射新程序/栈/堆/trampoline。
- `argv`/`envp` 为空指针时视为空数组（Linux 容忍）。
- `set_heap_top(USER_HEAP_BASE)`：新程序从初始堆开始。
- 返回 0 后 `uctx` 的 ip/sp 指向新程序，`task/user.rs` 继续循环进入用户态。

## 4. 与其他模块的关系

| 模块 | 关系 |
|---|---|
| `mm/loader.rs` | `load_user_app` |
| `mm/aspace/mod.rs` | `clear`/映射 |
| `file/mod.rs` | `FD_TABLE`（CLOEXEC） |
| `task/mod.rs` | `exe_path/cmdline/heap_top/signal.actions` |
| `pseudofs/proc.rs` | `/proc/<pid>/exe`/`cmdline` |
| `syscall/task/clone.rs` | 新建的进程也可能 execve |
| `src/init.sh` | `exec /bin/sh` |

## 5. 已知局限 / TODO

- 多线程 execve 未实现（返回 EAGAIN）。
- `argv[0]` 不由内核校验（直接传给程序）。
- 不处理 `PT_INTERP` 之外的解释器（由 loader 处理）。
- 无 `execveat`。
- `signal.actions` 重置后，被阻塞的线程（若有）状态未处理。
- `set_name` 用文件 basename（`loc.name()`），长度受 `TaskInner` 限制。

## 6. 变更影响面

改本文件 → `mm/loader.rs.md`、`file/mod.rs.md`、`task/mod.rs.md`、`pseudofs/proc.rs.md`、`syscall/task/{clone,wait}.rs.md`、`CONSTRAINTS.md` C8/C12/C17。
