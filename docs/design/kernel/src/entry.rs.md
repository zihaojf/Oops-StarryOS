# `kernel/src/entry.rs`

> 对应源码：`kernel/src/entry.rs`
> 所属：启动 / init 进程创建
> 依赖：`pseudofs::mount_all`、`task::*`、`mm::*`、`file::*`、`starry_process::Process`
> 被依赖：`src/main.rs`

## 1. 文件职责

`init(args, envs)` 是内核的**总装配函数**：挂伪文件系统 → 启动 alarm 任务 → 创建 init 进程的地址空间与任务 → 绑定控制终端 → 建立 stdio → 加入调度 → 等待退出 → 卸载/刷盘。

## 2. 关键流程（逐行对应）

```rust
pseudofs::mount_all().expect(...)      // /dev /dev/shm /tmp /proc /sys
spawn_alarm_task()                     // task/timer.rs 的内核任务

let loc = FS_CONTEXT.lock().resolve(&args[0]).expect(...);
let path = loc.absolute_path().expect(...);
let name = loc.name();

let mut uspace = new_user_aspace_empty()          // AddrSpace::new_empty(USER_SPACE_BASE, USER_SPACE_SIZE)
    .and_then(|mut it| { copy_from_kernel(&mut it)?; Ok(it) })
    .expect(...);

let (entry_vaddr, ustack_top) = load_user_app(&mut uspace, None, args, envs)
    .unwrap_or_else(|e| panic!(...));

let uctx = UserContext::new(entry_vaddr.into(), ustack_top, 0);
let mut task = new_user_task(name, uctx, 0);
task.ctx_mut().set_page_table_root(uspace.page_table_root());

let pid = task.id().as_u64() as Pid;
let proc = Process::new_init(pid);
proc.add_thread(pid);
N_TTY.bind_to(&proc).expect(...);

let proc = ProcessData::new(proc, path, Arc::new(args.to_vec()),
                            Arc::new(Mutex::new(uspace)),
                            Arc::default(), None);

{ // 建立 fd 0/1/2
  let mut scope = proc.scope.write();
  crate::file::add_stdio(&mut FD_TABLE.scope_mut(&mut scope).write()).expect(...);
}

let thr = Thread::new(pid, proc);
*task.task_ext_mut() = Some(AxTaskExt::from_impl(thr));

let task = spawn_task(task);
add_task_to_table(&task);

let exit_code = task.join();           // 阻塞直到 init 退出
info!("Init process exited ...");

let cx = FS_CONTEXT.lock();
cx.root_dir().unmount_all().expect(...);
cx.root_dir().filesystem().flush().expect(...);
```

## 3. 关键设计点

- **进程与任务 id 绑定**：`pid = task.id()`，保证 `getpid() == 1` 类的假设成立（init 的 pid 为 axtask 分配的首个 id）。
- **`set_page_table_root`**：在 spawn 之前设置用户页表根，任务首次切入用户态时才能正确切换地址空间。
- **`copy_from_kernel`**：RISC-V/x86_64 需要把内核页表项复制进用户页表（aarch64/la64 用独立页表，函数内是空操作）。
- **`N_TTY.bind_to(&proc)`**：要求 `pg.session().sid() == proc.pid()`（init 是会话首进程）且会话尚无终端；成功后可 `TIOCSCTTY`/前后台控制。
- **stdin/stdout/stderr**：`add_stdio` 打开 `/dev/console` 三次（读一次、写两次），`cloexec = false`。
- **`task.join()`**：等待 init 任务退出（不等待其余子进程）；随后 unmount+flush 可能与仍在运行的子进程竞争，依赖 init 脚本 `exit 0` 前 `sync`。

## 4. 不变量与约束

- `args.len() >= 1` 且 `args[0]` 可解析、可执行；否则 panic（启动失败可接受）。
- `ProcessData::new(..., exit_signal=None)`：init 没有父进程，不需要退出信号。
- `FD_TABLE.scope_mut(&mut scope)` 是**新进程作用域**的 fd 表，必须通过 `scope` 访问，不能直接写全局 `FD_TABLE`（C11）。
- init 退出后必须 unmount all 再 flush，否则 ext4 元数据丢失。
- `spawn_alarm_task` 必须在 `spawn_task(init)` 之前，保证 init 使用的定时器有驱动者。

## 5. 与其他模块的关系

| 依赖 | 用途 |
|---|---|
| `pseudofs::mount_all` | 提供 `/dev/console`、`/proc`、`/tmp` |
| `pseudofs::dev::tty::N_TTY` | 控制终端 |
| `mm::{new_user_aspace_empty, copy_from_kernel, load_user_app}` | 用户地址空间与 ELF 装载 |
| `task::{new_user_task, spawn_alarm_task, ProcessData, Thread, add_task_to_table}` | 任务/进程数据 |
| `file::{FD_TABLE, add_stdio}` | 标准 fd |
| `starry_process::Process::new_init` | 进程对象 |
| `axtask::{spawn_task, AxTaskExt}` | 调度 |

## 6. 已知局限 / TODO

- `// TODO: wait for all processes to finish`：只 join init 任务，不等待其它子进程。
- 若 init 崩溃（非正常退出），unmount/flush 可能仍执行，但子进程状态不一致。
- 无 `init=/path` 命令行参数（路径由顶层 crate 传入）。
- `expect` 在启动失败时直接 panic，没有串口友好错误码。

## 7. 变更影响面

改本文件 → `docs/design/src/main.rs.md`、`kernel/src/task/mod.rs.md`、`kernel/src/mm/loader.rs.md`、`kernel/src/pseudofs/mod.rs.md`、`ARCHITECTURE.md#3`（启动时序）。
