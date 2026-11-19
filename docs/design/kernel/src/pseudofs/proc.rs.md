# `kernel/src/pseudofs/proc.rs`

> 对应源码：`kernel/src/pseudofs/proc.rs`
> 所属子系统：procfs（`/proc`）
> 依赖：`pseudofs::{SimpleFs, SimpleFile, SimpleDir, DirMapping, RwFile, ...}`、`task/{AsThread, TaskStat, get_task, tasks}`、`file::FD_TABLE`
> 被依赖：`pseudofs/mod.rs`（`mount_all` → `new_procfs`）

## 1. 文件职责

构造 `/proc` 的伪文件系统，提供：

- 静态文件：`mounts`、`meminfo`、`meminfo2`（分配器 usage）、`instret`（RISC-V 指令数）、`interrupts`（tick 计数）、`sys/kernel/pid_max`；
- 动态目录：`/<pid>`（以及 `/self`）→ `ThreadDir`；
- `/proc/<pid>` 下：`stat`、`status`、`oom_score_adj`、`task/`（线程列表）、`maps`（假数据）、`mounts`、`cmdline`、`comm`、`exe`、`fd/`。

## 2. 结构

```rust
pub fn new_procfs() -> Filesystem { SimpleFs::new_with("proc", 0x9fa0, builder) }

struct ProcFsHandler(Arc<SimpleFs>);       // /proc 根：动态 pid + "self"
struct ThreadDir { fs, task: WeakAxTaskRef }  // /proc/<tid>
struct ProcessTaskDir { fs, process: Weak<Process> }  // /proc/<pid>/task
struct ThreadFdDir { fs, task: WeakAxTaskRef }        // /proc/<pid>/fd
fn builder(fs) -> DirMaker              // 静态项 + ProcFsHandler.chain(root)
```

## 3. 关键内容生成

| 文件 | 生成方式 |
|---|---|
| `stat` | `TaskStat::from_thread(&task)` → `format!` |
| `status` | `task_status`：Tgid/Pid/Uid/Gid/Cpus_allowed/... |
| `oom_score_adj` | `RwFile`：读数字；写时 `set_oom_score_adj` |
| `task/<tid>` | `ProcessTaskDir::lookup_child`：校验 tid 属于该进程，返回 `ThreadDir` |
| `maps` | 硬编码 4 行 vdso（**未用** `AddrSpace::areas()`） |
| `mounts` | 硬编码 `proc /proc proc rw,...` |
| `cmdline` | `proc_data.cmdline`，每个参数后加 `\0` |
| `comm` | `RwFile`：读 16 字节；写时截断 15 字节 + NUL，`set_name` |
| `exe` | `SimpleFile(NodeType::Symlink)` → `exe_path` |
| `fd/` | `ThreadFdDir`：遍历 `FD_TABLE.scope(&proc_data.scope)` 的 ids；每项是 `Symlink` → `inner.path()` |
| `/proc/self` | `current().clone()` |

`meminfo2` 用 `axalloc::global_allocator().usages()`；`interrupts` 在 `builder` 时注册 `axtask::register_timer_callback` 自增计数（注意：**只在 procfs 创建时注册一次**，模块级 static）。

## 4. 不变量与约束

- **C27**：所有动态内容必须"访问时生成"；`SimpleDirOps::is_cacheable() == false`（`ThreadDir/ProcessTaskDir/ProcFsHandler/ThreadFdDir` 都覆盖为 false），否则 VFS 会缓存过期的 pid 目录。
- **C12**：`fd/` 的每个条目是 `Symlink` 类型，`readlink` 返回 `inner.path()`；不要改成普通文件。
- `ThreadDir.task` 用 `WeakAxTaskRef`：进程/线程退出后目录自然失效（`lookup_child` 返回 `NotFound`）。
- `ProcessTaskDir.lookup_child` 必须校验 `task.proc_data.proc.pid() == process.pid()`，否则 `/proc/<pid>/task/<其他进程tid>` 会泄漏。
- `comm` 写入用 `CStr::from_bytes_until_nul`：必须确保 `input` 中有 NUL（`copy_len < 15` 时 `input[copy_len]=0` 已置零）。
- `interrupts` 的 `IRQ_CNT` 是 `AtomicUsize` + 定时器回调；**不要**改成 `Mutex`（回调在定时器上下文，C33）。
- `builder` 内的 `root.add("sys", ...)` 与 `ProcFsHandler.chain(root)` 顺序：动态 pid 在前，静态项在后；同名冲突时 pid 优先（`ChainedDirOps` 先查 A）。
- `maps`/`mounts` 是假数据，修正时不要影响其他文件。

## 5. 与其他模块的关系

| 模块 | 关系 |
|---|---|
| `pseudofs/{fs,dir,file}.rs` | 基础设施 |
| `task/{ops,stat,user}.rs` | `tasks()/get_task/TaskStat`、`AsThread` |
| `file/mod.rs` | `FD_TABLE.scope` |
| `entry.rs` | `mount_all` 调用 |
| `pseudofs/dev/mod.rs` | `/dev/shm` 由 devfs 提供目录、tmpfs 挂载（与 proc 无关） |
| `syscall/fs/ctl.rs` | readdir 走 `DirNodeOps::read_dir` |

## 6. 已知局限 / TODO

- `maps` 硬编码 vdso；应用 `AddrSpace::areas()` 生成真实映射。
- `mounts` 硬编码，未反映实际挂载表。
- 无 `/proc/cpuinfo`、`/proc/loadavg`、`/proc/stat`、`/proc/uptime`、`/proc/version` 等。
- `/proc/<pid>/fdinfo`、`cwd`、`root`、`environ` 缺失。
- `status` 缺少绝大多数字段（只 Tgid/Pid/Uid/Gid/Cpus/Mems）。
- 无 `net/` 目录、无 `sys/` 的真实内容。
- `interrupts` 的计数在 `builder` 时注册，若 procfs 重建会重复注册。

## 7. 变更影响面

改本文件 → `pseudofs/{dir,file,fs}.rs.md`、`task/stat.rs.md`、`file/mod.rs.md`、`ARCHITECTURE.md#8`、`CONSTRAINTS.md` C27。
