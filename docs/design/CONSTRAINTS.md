# 硬约束清单

> 这些是**违反即视为错误改动**的规则。每条给出：规则 → 原因 → 检查方式 → 反例。
> 编号供代码评审/Agent 引用（如"违反 C5"）。

## A 类：构建与依赖

### C1. 只编译到 `no_std` 裸机目标
- **规则**：`kernel` 与顶层 crate 都是 `#![no_std]`；不得引入需要 `std` 的代码路径。
- **原因**：目标是 `riscv64gc-unknown-none-elf` / `loongarch64-unknown-none-softfloat`。
- **检查**：`cargo build --target ...` 能过；不使用 `std::`、`println!`（用 `ax_println!`/`info!`）。
- **反例**：引入 `std::collections::HashMap`（应使用 `hashbrown` 或 `alloc::collections::BTreeMap`）。

### C2. 不修改 `vendor/`（除非同时修 checksum 并说明）
- **规则**：`vendor/` 是离线构建源，cargo 会校验 `.cargo-checksum.json`。禁止手改、禁止 `dos2unix`。
- **原因**：`make prepare` 从 `cargo-checksum.json` 恢复 `.cargo-checksum.json`；校验失败整个构建失败。
- **反例**：为修一个编译错误直接编辑 `vendor/axxxx/src/lib.rs`。

### C3. 工具链固定
- **规则**：`nightly-2025-05-20`（`rust-toolchain.toml` + Makefile 的 `TOOLCHAIN`）。不得依赖更新的 nightly 特性。
- **原因**：评测镜像提供该工具链；`make TOOLCHAIN=` 只是逃生口，不是常规路径。
- **反例**：使用 `slice::assume_init_mut` 等新稳定 API（`file/pipe.rs` 注释就记录了这个坑）。

### C4. `make all` 必须产出 `kernel-rv` 与 `kernel-la`
- **规则**：改动不得破坏顶层 `Makefile` 的 `rv`/`la` 目标与 `scripts/mkphyself.rs` 的输入参数约定。
- **原因**：评测系统执行 `make all` 并直接 `-kernel kernel-rv`。
- **检查**：`make all` 后 `file kernel-rv kernel-la` 为对应架构 ELF。

### C5. 不新增 workspace 之外的在线依赖
- **规则**：新依赖必须已在 `vendor/` 中；否则 `--offline` 构建失败。
- **原因**：`.cargo/config.toml` 将 crates.io 替换为 `vendor/`。
- **检查**：`cargo build --offline` 成功；`Cargo.lock` 与 `vendor/` 同步。

## B 类：内存与用户态访问

### C6. 用户指针不得裸解引用
- **规则**：所有来自系统调用参数、用户结构体字段的指针，必须通过 `starry_vm` 或 `mm::UserPtr/UserConstPtr/VmBytes/VmBytesMut/IoVectorBuf` 访问；在 `no_mangle` 之外不得直接 `*ptr`。
- **原因**：用户地址可能未映射/越权，裸解引用会触发内核页错误且无法恢复。
- **检查**：搜索 `as *mut`/`as *const` 后是否 `vm_read/vm_write/get_as_ref/get_as_mut/slice`。
- **反例**：`unsafe { *(arg as *const u32) }`。

### C7. 内核访问用户内存必须包在 `access_user_memory` 中
- **规则**：凡是在内核态可能触发用户缺页的访问，必须在 `mm::access_user_memory(|| ...)` 内；`PAGE_FAULT` handler 只会为这种"正在访问用户内存"的线程处理缺页。
- **原因**：`mm/access.rs` 的 trap handler 用 `Thread::is_accessing_user_memory()` 做门禁，否则返回 false → 内核 panic 或错误。
- **反例**：在 `copy_from_user` 风格手写循环里直接读用户地址。

### C8. 修改地址空间必须持 `aspace` 锁，且不得持锁做阻塞 I/O
- **规则**：`map/unmap/protect/populate/handle_page_fault` 均需 `proc_data.aspace.lock()`；锁内不得调用会 `block_on` 的文件/网络操作。
- **原因**：page fault 路径会再次尝试拿同一把锁（重入死锁）；`aspace` 是 `Arc<Mutex<..>>` 非重入。
- **反例**：在 `mmap` 持锁时读文件填充内容（应通过 backend 的 `populate` 在缺页时读）。

### C9. COW 帧引用计数必须与 `FRAME_TABLE` 一致
- **规则**：`alloc_frame` 后必须 `FRAME_TABLE.lock().init_frame(paddr)`；`unmap`/COW 复制减少引用时必须走 `FrameRefCnt::drop_frame`（它负责先摘表再释放）。
- **原因**：否则物理页会被重复分配（UAF）或永久泄漏；`cow.rs` 注释说明了"先 dealloc 再摘表会有竞争"。
- **反例**：直接 `dealloc_frame` 一个仍被两个页表引用的帧。

### C10. `SharedBackend` 不支持部分 range（已知限制）
- **规则**：不要对 `Backend::Shared` 的映射做部分 munmap/mprotect 后假设正确；新增调用点必须整段操作。
- **原因**：`shared.rs` 顶部 `FIXME: This implementation does not allow map or unmap partial ranges.`，`pages_starting_from` 只按起始页切片。
- **反例**：对一段 shared mmap 只 munmap 中间一页。

## C 类：文件描述符与 VFS

### C11. `FD_TABLE`/`FS_CONTEXT` 只能通过当前 scope 访问
- **规则**：业务代码写 `FD_TABLE.read()` / `FS_CONTEXT.lock()`；**不可**把别的进程的 `Arc<...>` 表或 `FsContext` 存进全局。
- **原因**：它们是 scope-local 全局（`scope_local!`），跨 scope 缓存会串进程、泄漏、破坏 fork 语义。
- **反例**：`static FOO: Mutex<Option<FsContext>>`。

### C12. 新增 fd 类型必须实现 `FileLike + Pollable` 并登记
- **规则**：必须实现 `path()`（`/proc/<pid>/fd` 依赖）；`read/write` 默认应返回 `InvalidInput` 而不是 panic；必须实现 `Pollable::poll/register`。
- **原因**：`FileLike` 默认实现+`DowncastSync` 是 `from_fd` 的基础；`/proc` 与 epoll 都依赖 `poll`。
- **反例**：新 fd 不实现 `Pollable`，导致 `poll/epoll` 永远不唤醒。

### C13. 阻塞语义必须一致
- **规则**：`nonblocking() == false` 时，`read/write` 内部用 `block_on(poll_io(...))` 等待；`nonblocking() == true` 时返回 `WouldBlock`。
- **原因**：`sys_read/write` 不做额外等待，全部语义由 `FileLike` 承担。
- **反例**：在 `read` 里自旋等数据（占满 CPU 且无法被信号打断）。

### C14. 关闭/退出必须唤醒等待者
- **规则**：任何有等待队列的 fd（pipe/pty/epoll/pidfd）在 `Drop` 或对端关闭时必须 `PollSet::wake()`。
- **原因**：否则阻塞的读者永远不返回（`Pipe::drop` 唤醒 `poll_close` 就是范例）。

### C15. 目录操作失败必须返回正确 errno
- **规则**：`create/link/unlink/rename` 在 `SimpleDirOps` 未实现时返回 `OperationNotPermitted`；`lookup` 缺失返回 `NotFound`；不要返回 `InvalidInput` 糊弄。
- **原因**：`mkdir`、`rm`、`mv`、tar 等工具依赖具体 errno 决定行为。

## D 类：任务、信号、调度

### C16. 任务退出只能走 `task::do_exit`
- **规则**：不要在 syscall 或业务代码里自行设置 `exit` 标志/清理线程状态。
- **原因**：`do_exit` 负责 `clear_child_tid` + futex 唤醒、robust list、`Process::exit_thread`、父进程通知、SHM 清理、group_exit 广播。绕过会导致资源泄漏和父进程收不到 `SIGCHLD`。
- **反例**：`thr.set_exit(); return 0;`。

### C17. 信号处理只能在用户态返回路径
- **规则**：信号的实际递送（构造栈帧）由 `check_signals` 在 `uctx.run()` 返回后执行；不要在任意内核上下文改用户 `UserContext`。
- **原因**：`starry_signal` 需要完整的用户寄存器上下文来构造 `SignalFrame`；`task/user.rs` 的循环顺序（syscall/pagefault → check_signals → 设置 TimerState → clear_interrupt）不可打乱。

### C18. 定时器状态切换必须成对
- **规则**：进入内核态前 `set_timer_state(Kernel)`，返回用户态前 `set_timer_state(User)`（`task/user.rs`）。
- **原因**：`TimeManager::poll` 依据状态累计 `utime/stime`，否则 `times()/getrusage/getitimer` 统计错误。

### C19. `clone` 标志组合必须校验
- **规则**：新增/修改 `CloneFlags` 用法时保持 `CloneArgs::validate` 的约束（`THREAD` 需要 `VM|SIGHAND`、`SIGHAND` 需要 `VM`、`VFORK` 与 `THREAD` 互斥等）。
- **原因**：否则会构造出共享地址空间但不共享信号处理等非法组合，导致随机崩溃。

### C20. 不得在持锁状态下阻塞或跨任务唤醒
- **规则**：`Mutex`/`SpinNoIrq` 内不得 `block_on`、不得 `yield_now`、不得等待其他任务释放同一把锁。
- **原因**：单核阻塞会让持锁任务永远无法被调度，直接死锁。
- **反例**：`let mut g = MSG_MANAGER.lock(); block_on(...)`。

## E 类：系统调用层

### C21. syscall 错误必须映射为负 errno
- **规则**：`sys_*` 返回 `AxResult<isize>`；`handle_syscall` 统一 `-LinuxError::from(err).code()`。不要直接返回 `-1`。
- **原因**：libc 依赖具体 errno。

### C22. 未实现 syscall 返回 `Unsupported`
- **规则**：`Sysno` 未覆盖的分支落到 `_ => warn + Err(AxError::Unsupported)`；不要 `panic!`、不要静默返回 0（除非有注释说明"探测型调用"）。
- **原因**：返回 `ENOSYS` 让 libc 走回退路径；静默成功会隐藏 bug。

### C23. 参数 `as _` 转换必须确认宽度
- **规则**：`handle_syscall` 中 `uctx.argN() as _` 的截断必须与目标类型宽度一致（如 `u32`、`i32`、`usize`）；有符号/无符号转换要显式。
- **原因**：Linux ABI 中 `-1` 表示"默认/无"（如 `pid_t`、`dirfd`、`timeout`），错误转换会把 `-1` 变成巨大正数。

### C24. `sigsetsize`、`cpusetsize`、`optlen` 等长度参数必须校验
- **规则**：使用 `check_sigset_size`、`cpusetsize * 8 >= cpu_num()`、`get::<T>()` 中的 `size_of::<T>()` 比较等既有模式。
- **原因**：防止越界读写用户内存（安全）与 ABI 不一致（兼容）。

### C25. 新增 syscall 必须同时登记三处
1. `syscall/mod.rs` 的 `match sysno`；
2. 对应 `syscall/<子系统>/*.rs` 的实现；
3. `docs/design/kernel/src/syscall/<文件>.md` 的"支持列表/已知局限"。

## F 类：伪文件系统与设备

### C26. `SimpleFs` 的 inode 号生命周期由 `SimpleFsNode` 管理
- **规则**：`SimpleFsNode::new` 分配 inode，`Drop` 释放；不要手工 `alloc_inode` 而不释放。
- **原因**：`Slab` 槽位泄漏会导致反复创建/删除文件后 inode 耗尽。

### C27. `/proc` 节点必须反映"访问时状态"
- **规则**：动态内容用 `RwFile`/闭包生成（见 `proc.rs`），不要缓存到静态变量。
- **原因**：每个进程看到的 `/proc/self` 必须不同；`is_cacheable()` 返回 false 的目录不得被 VFS 缓存。

### C28. 设备 `DeviceOps` 不能假设调用者持锁
- **规则**：`read_at/write_at/ioctl` 需要自行加锁保护内部状态（如 `EventDev::inner`、`LoopDevice::file`）。
- **原因**：VFS 与 syscall 层不会为设备串行化。

## G 类：文档

### C29. 源码变更必须同步文档
- **规则**：修改/新增/删除源文件时，同一提交内更新 `docs/design/<同样路径>.md`；结构性改动同步更新 `ARCHITECTURE.md`/`PRINCIPLES.md`/`TRACEABILITY.md`。
- **原因**：文档是 Agent 的输入，过期文档比没有文档更危险。
- **检查**：`git status` 对照（人工评审时执行；Agent 会话禁止 git 操作，见 C31）。

### C30. 文档只描述现状
- **规则**：文档中"已实现"的行为必须能在源码找到对应；未实现的只能进"已知局限/TODO"。

### C31. Agent 会话禁止在 StarryOS 仓库执行 git 操作
- **规则**：Agent 只能在用户明确要求且确认后执行 `git`；默认只做文件读写。当前工作流要求：改完由人类自行 `git add/commit`。
- **原因**：仓库可能处于多人协作/特殊权限状态（见 `docs/team-docker.md` 第 3 节：root 容器里执行 git 会破坏 `.git` 属主）。

## H 类：并发与锁顺序（新增代码时自检）

### C32. 锁顺序固定：`aspace` 之外不要嵌套持锁
- **规则**：推荐获取顺序：`FD_TABLE/FS_CONTEXT` → `aspace` → backend 内部锁（`FRAME_TABLE`、`CachedFile`）。**禁止**反向嵌套（如持 `aspace` 再取 `FD_TABLE`）。
- **原因**：page fault 路径会从 `aspace` 进入 backend/cache；反向会死锁。
- **反例**：`let mut a = aspace.lock(); FD_TABLE.write()...`。

### C33. 中断上下文可用性
- **规则**：标记为 `SpinNoIrq` 的锁可以在关中断上下文使用；`Mutex`（可能睡眠）不得在中断/timer callback 中使用。
- **原因**：`pseudofs/proc.rs` 的 `register_timer_callback` 在定时器上下文运行，只能做原子操作。

### C34. `Weak` 与生命周期
- **规则**：`TASK_TABLE/PROCESS_TABLE/PROCESS_GROUP_TABLE/SESSION_TABLE` 存 `Weak`，不能改成 `Arc`（否则进程/任务永不释放）。
- **原因**：`task/ops.rs` 的表设计就是为了不延长生命周期；`cleanup_task_tables` 仅清理失效条目。

## 违规处理

1. 评审发现违反 → 打回并按编号引用规则。
2. 如确需突破某条约束：在对应文件文档的"不变量与约束"中写明**例外原因、范围、回收条件**，并同步 `CONSTRAINTS.md`。
3. Agent 遇到与约束冲突的需求 → 停止实现，输出"冲突点 + 建议方案"，交由人类决定。
