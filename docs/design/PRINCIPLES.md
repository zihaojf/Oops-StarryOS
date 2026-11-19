# 设计原则与不变量

> 本文解释 StarryOS **为什么这样设计**。每条原则都给出源码依据，便于评审时对照。
> 与本文配套：`CONSTRAINTS.md`（必须遵守的具体规则）、`ARCHITECTURE.md`（结构图）。

## P1. 站在 ArceOS 上，不重复造轮子

**原则**：硬件抽象、页表、调度、块设备/网卡驱动、ext4、TCP/IP 全部复用 ArceOS 组件；StarryOS 只实现 Linux 语义层。

**源码依据**：
- `kernel/Cargo.toml` 依赖 `axhal/axmm/axtask/axfs/axnet/axruntime`，`axfeat` 打开 `uspace`、`multitask`、`fs-ng-ext4`、`net-ng` 等。
- `kernel/src/lib.rs` 只有 8 个模块，没有 `arch/`、`driver/` 目录。
- `mm/aspace/mod.rs` 直接用 `axhal::paging::PageTable` 与 `memory_set::MemorySet`。

**推论**：
- 新功能优先找 ArceOS 是否已提供（`axhal::console`、`axdisplay`、`axnet::unix`、`axtask::future`…）。
- 不要在内核里手写页表遍历/中断控制器/驱动；确需扩展应改 ArceOS（但本仓库的 `vendor/` 不允许随意改，见 `CONSTRAINTS.md` C2）。

## P2. Linux ABI 兼容优先于"内部优雅"

**原则**：对外行为以 Linux 为准，包括 errno、结构体布局、边界条件、`/proc` 文本格式；内部实现可以为兼容性妥协。

**源码依据**：
- 系统调用参数与结构体全部来自 `linux_raw_sys`（`stat`、`statx`、`sysinfo`、`sockaddr_*`、`itimerval`、`msqid_ds`…）。
- 大量刻意模仿 Linux 的细节：`sys_mkdirat` 对已存在路径返回 `AlreadyExists`（注释说明 `mkdir -p` 依赖它）；`sys_getcwd` 返回 buf 指针而非 0（注释标 FIXME 但保留以兼容）；`sys_chown` 清除 setuid/setgid 位；`sys_waitpid` 被信号打断后**重试**而非返回 `EINTR`（注释说明 shell/`timeout` 依赖）。
- `/proc/<pid>/stat` 字段顺序与 man page 一致（`task/stat.rs`）。

**推论**：改 syscall 语义前，先查 Linux man page 与 glibc/musl 的实际用法；"看起来多余"的兼容代码不要删。

## P3. 用枚举后端 + trait 分派抽象映射类型

**原则**：不同 mmap 语义用 `Backend` 枚举表达，公共操作走 `BackendOps` trait，用 `enum_dispatch` 静态分派；VFS 侧用 `FileLike` + `DowncastSync` 做运行时分派。

**源码依据**：
- `mm/aspace/backend/mod.rs`：`#[enum_dispatch(BackendOps)] pub enum Backend { Linear, Cow, Shared, File }`。
- `Backend` 同时实现 `memory_set::MappingBackend`，从而能塞进 `MemorySet<Backend>`。
- `file/mod.rs`：`trait FileLike: Pollable + DowncastSync`，各 fd 类型只实现自己关心的操作，默认返回 `InvalidInput`/`NotATty`。

**推论**：
- 新增映射类型 → 加一个 `Backend` 变体 + 实现 `BackendOps` + 在 `clone_map` 里正确处理跨页表复制。
- 新增 fd 类型 → 实现 `FileLike`（至少 `path`）与 `Pollable`，然后 `add_to_fd_table`。

## P4. 资源"作用域化"，为共享/隔离留出空间

**原则**：fd 表与文件系统上下文不是"全局唯一"，而是绑定到 `ProcessData.scope`；任务切换时切换活动作用域。

**源码依据**：
- `file/mod.rs`：`scope_local! { pub static FD_TABLE ... }`；`pseudofs`/`syscall` 全部通过 `FD_TABLE.read()` 访问，自动落到当前 scope。
- `task/mod.rs`：`Thread::on_enter/on_leave` 操作 `scope_local::ActiveScope`。
- `clone.rs`：`CLONE_FILES`/`CLONE_FS` 决定 `clone_from`（共享）还是 `clone_from` 到新 scope（复制）。

**推论**：
- 任何新资源若要支持 `CLONE_*` 共享语义，也应放进 `Scope`，而不是新增全局静态。
- **不要**把 `Arc<dyn FileLike>`、`FsContext` 等 scope 内对象存进跨进程的全局结构（会破坏隔离并泄漏）。

## P5. 惰性分配 + 写时复制

**原则**：mmap/ELF 装载只登记 VMA；真正分配在 page fault 或显式 `MAP_POPULATE`/`populate_area` 时发生；`MAP_PRIVATE` 用 COW。

**源码依据**：
- `mm/loader.rs`：ELF 段用 `Backend::new_cow(...)` 且 `populate=false`；只有栈顶实际用到的部分 `populate_area`。
- `mm/aspace/backend/cow.rs`：`map()` 是空实现；`populate()` 时才 `alloc_new_at` 或 `handle_cow_fault`。
- `mm/aspace/backend/shared.rs`：`SharedPages::new` 一次性分配并零初始化，供多进程共享。
- `syscall/mm/mmap.rs`：`map_flags.contains(POPULATE)` 才传 `populate=true`。

**推论**：不要在 `mmap`/`clone` 里预分配大量物理页（会 OOM、变慢）；分配逻辑必须放在 backend 的 `populate`。

## P6. 阻塞用 future，不用"睡在内核里"

**原则**：内核态等待统一写成 `block_on(poll_io(...))` / `block_on(interruptible(...))` / `block_on(future::timeout(...))`，把当前任务挂起并让出 CPU。

**源码依据**：
- `file/fs.rs`：非阻塞节点用 `block_on(poll_io(self, IoEvents::IN/OUT, nonblocking, || ...))`。
- `file/pipe.rs`、`file/epoll.rs`、`pseudofs/dev/tty/mod.rs`、`syscall/io_mpx/*` 同一模式。
- `axtask::future::interruptible` 用于可被信号打断的等待（`wait4`、`nanosleep`）。

**推论**：
- 任何可能长时间等待的操作都不得忙等/自旋。
- 持锁时不得调用会阻塞的接口（见 C7）。

## P7. 单核串行假设 + 明确的锁层级

**原则**：默认单核（`max-cpu-num = 1`），大量逻辑依赖"当前任务是唯一执行者"；跨任务共享状态用粗粒度锁保护；`smp` 是可选特性而非默认正确性前提。

**源码依据**：
- `configs/*.toml`：`max-cpu-num = 1`；顶层 `Cargo.toml` 的 `smp` feature 是可选。
- `task/timer.rs`、`task/futex.rs`、`syscall/ipc/*` 用全局 `Mutex`/`SpinNoIrq`。
- 多处 `assert!`/`unwrap` 表达"此处不可能并发"的假设（如 `SimpleFs::root` 必须已初始化、`SharedBackend` 不允许部分 map/unmap）。

**推论**：新增并发路径时先问"单核下会不会重入/死锁"；若引入新锁，必须写进对应文件文档的"不变量"。

## P8. 显式记录未实现，不静默假装支持

**原则**：未实现的功能要么返回 `Unsupported`/`NoSuchDevice` 并 `warn!`，要么用 `TODO`/`FIXME` 注释明确标注；允许"宽松返回成功"的地方必须有注释说明原因（通常是程序探测行为）。

**源码依据**：
- `syscall/mod.rs` 末尾：未匹配 syscall → `warn!("Unimplemented syscall")` + `Unsupported`。
- `sys_dummy_fd`：对 `qemu-*` 任务名返回 `Unsupported`（注释：要对 QEMU 诚实，它会回退到其他策略）。
- `sys_sync`/`sys_msync`/`sys_madvise`/`sys_mlock2` 返回 0 但 `warn!` 或注释 `TODO`。
- `syscall/ipc/msg.rs`：阻塞发送/接收未实现，显式 `warn!` 后返回 `EAGAIN`/`ENOMSG`。

**推论**：新增 stub 必须：返回合理 errno + `warn!`/`debug!` + 文件文档"已知局限"登记；**禁止**用 panic 表示未实现。

## P9. 可观测性与可调试性

**原则**：关键路径打日志（`debug!`/`trace!`）、提供 `/proc` 自省、提供内存泄漏/占用分析入口。

**源码依据**：
- 每个 syscall 入口 `debug!`，分派结束 `debug!` 返回值（`syscall/mod.rs`）。
- `pseudofs/proc.rs`：`/proc/meminfo2`（分配器 usage）、`/proc/interrupts`（tick 计数）、`/proc/instret`（RISC-V 指令数）。
- `pseudofs/dev/memtrack.rs`：写 `start\n`/`end\n` 触发按 backtrace 分类的分配统计；`clear_elf_cache()`/`cleanup_task_tables()` 用于排除噪声。
- `scripts/ci-test.py` 通过串口抓 `starry:~#` 提示符做启动冒烟测试。

**推论**：新增子系统时同步提供 `/proc` 或 `/dev` 观测点；日志不要出现在热路径的高频 `info!`。

## P10. 失败必须可控：错误码而非崩溃

**原则**：内核态遇到用户错误输入应返回 errno；`panic!`/`expect` 只用于"违反内部不变量"，或启动阶段失败（无法继续）。

**源码依据**：
- 系统调用层几乎全部返回 `AxResult`；`ax_bail!` 宏用于带信息的错误返回（`mm/aspace/mod.rs`）。
- 启动阶段使用 `expect`：`entry.rs` 的挂载/装载/绑定 stdio。
- `file/fs.rs` 的 `path_for` 用 `"<error>"` 兜底而不是 panic；`stat()` 对 socket 返回伪造 mode。

**推论**：新增代码里出现的 `unwrap/expect/assert` 必须能一句话解释"为什么这里不可能失败"；否则改成错误返回。

## P11. 兼容分支显式化

**原则**：架构差异、C 库差异、测试镜像差异都用显式条件表达，不靠隐式假设。

**源码依据**：
- `#[cfg(target_arch = ...)]` 遍布 `syscall/mod.rs`、`config/*`、`mm/loader.rs`。
- `sys_dummy_fd` 用任务名 `qemu-` 前缀判断"是否在 QEMU 下"。
- `src/main.rs` 探测 `/bin/sh`→busybox 回退；`init.sh` 探测 `/musl`、`/glibc` 测试目录。

**推论**：新增兼容分支要写注释说明"为谁兼容"（哪个 libc/哪个工具/哪个平台）。

## P12. 文档即约束

**原则**：设计文档不是事后描述，而是**下一次改动的输入**。文档中的"不变量/影响面"等同于代码契约。

**推论**：改代码必须同步改文档；文档与代码冲突时，以"能解释当前代码且能指导下一步"为准，并在同一提交修正。
