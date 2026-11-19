# StarryOS 总体架构

> 本文是"全局图景"。单文件细节见 `docs/design/kernel/**`；硬性规则见 `CONSTRAINTS.md`；设计取舍见 `PRINCIPLES.md`。

## 1. 系统定位

StarryOS 是一个**基于 ArceOS unikernel 组件构建的单体内核（monolithic kernel）**，目标是提供 Linux 兼容的进程/线程、文件系统、网络、信号、IPC 与设备接口，从而直接运行 Linux 用户态程序（glibc 与 musl 两套 C 库、busybox、测试套件、X11 等）。

它不实现自己的硬件抽象层：CPU、页表、中断、定时器、串口、块设备、网卡、显示等都由 ArceOS 的 `axhal` / `axplat-*` crate 提供。StarryOS 的代码集中在**"Linux 语义层"**：

- 进程/线程/会话/进程组（基于 `starry_process`）
- 信号（基于 `starry_signal`）
- 用户地址空间与 mmap 语义（基于 `axmm` / `memory_set` / `axhal::paging`）
- 文件描述符与 VFS 语义（基于 `axfs` / `axfs_ng_vfs`）
- 系统调用分派（基于 `syscalls::Sysno` 与 `linux_raw_sys`）
- 伪文件系统（`/dev`、`/proc`、`/tmp`、`/sys`）
- 网络 socket 封装（基于 `axnet`）

版本基线：`Cargo.toml` 中 `version = "0.2.0-preview.2"`，`edition = "2024"`。

## 2. 分层结构

```text
┌──────────────────────────────────────────────────────────────────────┐
│ 用户态程序（glibc/musl/busybox/X11/测试点）                          │
│   ↕ syscall / page fault / interrupt                                  │
├──────────────────────────────────────────────────────────────────────┤
│ 顶层 crate: starryos (src/main.rs)                                   │
│   只做一件事：挑选 init 程序的 shell，调用 starry_kernel::entry::init │
├──────────────────────────────────────────────────────────────────────┤
│ kernel crate: starry-kernel (kernel/src/**)                          │
│                                                                      │
│   entry ──► task ──► syscall ──► file / mm / pseudofs / ipc / net    │
│                │          │            │                             │
│                └──────────┴────────────┴──► task(信号/时间/futex)     │
├──────────────────────────────────────────────────────────────────────┤
│ ArceOS 组件（workspace 依赖）                                        │
│   axruntime  启动与运行时                                             │
│   axhal      CPU/页表/中断/时间/串口/功耗                             │
│   axmm       内核地址空间                                             │
│   axtask     任务调度、TaskInner、future/block_on/poll_io             │
│   axfs       文件系统 + 页缓存 + ext4（axfs-ng）                      │
│   axfs_ng_vfs VFS 对象模型（DirEntry/Location/Filesystem/NodeOps）    │
│   axnet      TCP/UDP/Unix/vsock                                       │
│   axsync/kspin/spin  锁                                              │
│   axalloc    全局分配器（slab + buddy）                               │
│   axpoll     Pollable/IoEvents/PollSet                               │
│   axdisplay/axinput/axdriver  显示/输入/驱动                         │
├──────────────────────────────────────────────────────────────────────┤
│ axplat-* 平台 crate（riscv64-qemu-virt / loongarch64-qemu-virt ...）  │
└──────────────────────────────────────────────────────────────────────┘
```

### 2.1 顶层 crate 与内核 crate 的关系

- `src/main.rs`（crate `starryos`）是**唯一**的二进制入口，`#![no_std] #![no_main]`，导出 `main()` 符号。
- `axruntime` 完成早期启动（内存、页表、中断、平台初始化）后调用 `main()`。
- `main()` 探测 rootfs 上可用的 shell（`/bin/sh` → `/musl/busybox` → `/glibc/busybox` → `/busybox`），拼出 `argv`（busybox 需要 applet 名 `sh`），调用 `starry_kernel::entry::init(&args, &envs)`。
- `init()` **不会返回**：它创建 init 进程、等待其退出、卸载文件系统、刷盘，然后由 `axruntime`/`axhal` 关机路径结束。

## 3. 启动流程（时序）

```text
axruntime                                    starryos::main               starry_kernel::entry
   │                                              │                              │
   │ 平台/内存/页表/中断/调度器初始化              │                              │
   ├─────────────────────────────────────────────►│                              │
   │                                              │ 选择 shell + argv            │
   │                                              ├─────────────────────────────►│ init(args, envs)
   │                                              │                              │
   │                                              │        1. pseudofs::mount_all()
   │                                              │           /dev /dev/shm /tmp /proc /sys
   │                                              │        2. spawn_alarm_task()
   │                                              │        3. resolve(args[0]) 取绝对路径
   │                                              │        4. new_user_aspace_empty()
   │                                              │        5. copy_from_kernel()（非 aarch64/la64）
   │                                              │        6. load_user_app() → (entry, ustack_top)
   │                                              │        7. UserContext::new(entry, sp, 0)
   │                                              │        8. new_user_task() 建 TaskInner
   │                                              │        9. Process::new_init(pid); add_thread
   │                                              │       10. N_TTY.bind_to(&proc)
   │                                              │       11. ProcessData::new(...) + add_stdio
   │                                              │       12. spawn_task + add_task_to_table
   │                                              │       13. task.join()  ← init 退出前一直阻塞
   │                                              │       14. unmount_all() + flush()
   │                                              │                              │
   │                                              │        init 进程进入 user.rs 主循环：
   │                                              │        uctx.run() → Syscall/PageFault/...
```

关键点：

- **`Process::new_init(pid)` 与 pid 的关系**：init 任务的 `task.id().as_u64()` 直接作为初始进程 pid（`entry.rs:47-49`）。任务表、进程表都以 `Pid` 为键。
- **stdio 来源**：`/dev/console` 打开两次（读一次、写两次），分别作为 fd 0/1/2。
- **N_TTY 全局单例**：`pseudofs/dev/tty/ntty.rs` 的 `lazy_static N_TTY` 是控制台 tty，init 进程通过 `bind_to` 成为会话控制终端。
- **`join()` 之后**才卸载文件系统并 flush，保证 init 及其所有子进程（若未 detach）结束后数据落盘。

## 4. 进程 / 线程与资源模型

### 4.1 三层对象

| 层 | 类型 | 提供方 | 职责 |
|---|---|---|---|
| 调度实体 | `axtask::TaskInner` | ArceOS `axtask` | 内核栈、CPU 上下文、状态、调度 |
| 线程扩展 | `Thread`（`Box<Thread>` 放入 `task_ext`） | `kernel/src/task/mod.rs` | 线程私有：信号管理器、时间管理器、`clear_child_tid`、robust list、exit 标志 |
| 进程共享 | `Arc<ProcessData>` | `kernel/src/task/mod.rs` | 地址空间、fd 作用域、rlimit、信号动作、futex 表、umask、cmdline、退出事件 |
| 进程/组/会话 | `Arc<Process>` / `Arc<ProcessGroup>` / `Arc<Session>` | `starry_process` crate | pid/ppid/子进程/线程集合/退出码/组与会话关系 |

`Thread` 通过 `TaskExt` 的 `on_enter` / `on_leave` 回调在**任务切入/切出**时切换 `scope_local` 的活动作用域（`scope_local::ActiveScope`）。这是 fd 表与文件系统上下文"按进程隔离"的关键机制。

### 4.2 资源作用域（scope_local）

```text
TaskInner ──task_ext──► Box<Thread> ──proc_data──► Arc<ProcessData>
                                                     ├── aspace: Arc<Mutex<AddrSpace>>
                                                     ├── scope:  RwLock<Scope>   ← 资源作用域
                                                     └── ...
scope_local::scope_local! {
    FD_TABLE:   Arc<RwLock<FlattenObjects<FileDescriptor, 1024>>>
    FS_CONTEXT: FsContext
}
```

- `FD_TABLE` 与 `FS_CONTEXT` 都是 **scope-local 全局**：访问时自动取"当前任务所属 scope"的实例。
- `Thread::on_enter` 把 `proc_data.scope` 设为 active scope；`on_leave` 恢复 global scope。
- `clone()` 中 `CLONE_FILES` / `CLONE_FS` 决定子进程是**共享**还是**复制**该作用域（见 `syscall/task/clone.rs`）。

### 4.3 生命周期

- **创建**：`clone`/`clone3`/`fork` → `CloneArgs::do_clone` → `new_user_task` → `Process::fork`（或共享）→ `ProcessData::new` → `Thread::new` → `spawn_task` → `add_task_to_table`。
- **退出**：`sys_exit`/`sys_exit_group` → `task::do_exit`（`kernel/src/task/ops.rs`）：
  1. 清 `clear_child_tid` 并唤醒其 futex；
  2. 遍历 robust list 标记 owner-dead 并唤醒等待者；
  3. `Process::exit_thread`，最后一个线程时 `Process::exit()`，通知父进程（`exit_signal` + `child_exit_event`）、唤醒 `exit_event`、清理 SysV 共享内存；
  4. `group_exit` 时向同进程其他线程发 `SIGKILL`。
- **回收**：`wait4` 等到 zombie 子进程，`child.free()` 释放 `Process`。

## 5. 用户地址空间与内存管理

### 5.1 数据结构

```text
AddrSpace {
    va_range: VirtAddrRange,          // 用户空间范围（config::USER_SPACE_BASE/SIZE）
    areas:    MemorySet<Backend>,     // VMA 集合（memory_set crate）
    pt:       PageTable,              // 页表（axhal::paging）
}
```

`MemorySet` 负责区间查找/分裂/合并；`Backend`（`enum_dispatch`）负责每类映射的实际页操作：

| Backend | 语义 | 典型来源 |
|---|---|---|
| `Linear` | VA 与 PA 固定偏移 | `/dev/fb0` 等物理设备 mmap、信号 trampoline |
| `Cow` | 私有可写、写时复制；可带文件初始内容 | ELF 段（`MAP_PRIVATE`）、匿名 `MAP_PRIVATE`、栈/堆 |
| `Shared` | 多进程共享同一组物理页 | `MAP_SHARED|MAP_ANONYMOUS`、SysV `shmat` |
| `File` | 文件页缓存映射 + 驱逐回调 | `MAP_SHARED` 文件映射、`/dev/loopX` mmap |

### 5.2 惰性分配与 page fault

- `mmap`/`brk`/ELF 装载只建立 VMA（`map(..., populate=false)` 或 `Backend::map` 空实现），**不立即分配页**。
- 用户访问触发 page fault：
  1. `mm/access.rs` 注册的 `PAGE_FAULT` trap handler 检查 `Thread::is_accessing_user_memory()`；
  2. 只有在内核主动访问用户内存（`access_user_memory()` 包裹）时才处理，否则返回 `false` 交给上层；
  3. `AddrSpace::handle_page_fault` 检查 VMA 权限 → `backend.populate()` 分配/填页并建立映射。
- 用户态 fault（非内核访问用户内存）走 `task/user.rs` 的 `ReturnReason::PageFault` 分支，失败则 `SIGSEGV`。

### 5.3 地址空间布局（riscv64/loongarch64 相同）

```text
0x1000                                   USER_SPACE_BASE
0x400_0000                               USER_INTERP_BASE（动态链接器）
0x4000_0000                              USER_HEAP_BASE（初始堆）
0x4000_0000 + USER_HEAP_SIZE             初始堆结束（brk 从这里继续扩）
0x6000_1000                              SIGNAL_TRAMPOLINE（一页，RX）
0x4_0000_0000 - USER_STACK_SIZE          USER_STACK_TOP 下方，栈
0x4_0000_0000                            USER_STACK_TOP
0x40_0000_0000                           用户空间结束（USER_SPACE_BASE+SIZE）
```

x86_64/aarch64 的 `USER_SPACE_SIZE`/`USER_STACK_TOP` 在 `config/*.rs` 中不同。

## 6. 系统调用路径

```text
user.rs 主循环
  uctx.run()  ──ReturnReason::Syscall──►  syscall::handle_syscall(uctx)
                                             │ Sysno::new(uctx.sysno())
                                             │ match sysno { ... }        ← 单一大 match（mod.rs）
                                             │ 各 sys_xxx(args as _)      ← 参数从寄存器取出
                                             │ AxResult<isize> → 成功值 / -errno
                                             └► uctx.set_retval(...)
```

约定：

- 所有 `sys_*` 返回 `AxResult<isize>`；错误统一经 `-LinuxError::from(err).code()` 转成负 errno。
- 用户指针**永不裸解引用**：必须经 `starry_vm::{VmPtr, VmMutPtr, vm_load_string, vm_read_slice}`、`mm::UserPtr/UserConstPtr/VmBytes/VmBytesMut` 或 `IoVectorBuf`。
- 参数类型按 Linux ABI 从 `linux_raw_sys` 取（`stat`、`statx`、`sockaddr*`、`itimerval`…），结构体布局必须与内核 ABI 一致。
- 架构差异用 `#[cfg(target_arch = ...)]` 在同一文件内切换（典型：`x86_64` 才有 `open/stat/dup2/...`；`renameat` 在 riscv64 不存在；`fstatat` 在 riscv64 叫 `newfstatat`）。
- 未实现的系统调用返回 `AxError::Unsupported`（→ `ENOSYS`）并 `warn!`，不能 panic。

## 7. 文件与 VFS 模型

```text
syscall (open/read/write/stat/...)
   │
   ▼
file::FileLike（trait，DowncastSync + Pollable）
   ├── File       包装 axfs::File（普通文件/设备节点）
   ├── Directory  包装 Location（目录），带独立 readdir offset
   ├── Pipe       环形缓冲 + 两端共享 Arc<Shared>
   ├── Socket     包装 axnet::Socket
   ├── Epoll      就绪队列 + interest 表（LT/ET/ONESHOT）
   ├── EventFd    AtomicU64 计数器（semaphore 模式）
   ├── Signalfd   线程信号队列视图
   ├── PidFd      进程/线程退出事件
   └── DummyFd    未实现但被依赖的 fd（如 io_uring）
   │
   ▼
FD_TABLE（scope-local，FlattenObjects<FileDescriptor, AX_FILE_LIMIT=1024>）
```

- **阻塞语义**：`FileLike` 的 `read/write` 内部用 `block_on(poll_io(self, IoEvents::IN/OUT, nonblocking, || ...))`。返回 `WouldBlock` 且在阻塞模式下会让出 CPU；`nonblocking()` 为 true 时直接返回 `WouldBlock`。
- **路径解析**：`file::resolve_at(dirfd, path, flags)` 统一处理 `AT_FDCWD` / `AT_EMPTY_PATH` / `AT_SYMLINK_NOFOLLOW`，先 `with_fs` 切换到 `dirfd` 对应的当前目录。
- **文件系统上下文**：`axfs::FS_CONTEXT` 是 scope-local 的 `FsContext`，包含 root/cwd 与 mount 树；`sys_chroot` 会整体替换它。
- **页缓存**：`axfs::CachedFile` 提供 `read_at/write_at/with_page/with_page_or_insert`；`FileBackend` 通过 `add_evict_listener` 在页被驱逐时解除用户页表映射。

## 8. 伪文件系统（`pseudofs`）

`pseudofs` 用一套轻量 VFS 实现（`SimpleFs` + `SimpleDir` + `SimpleFile` + `Device`）在内存中构造文件树，并在 `mount_all()` 挂载：

| 挂载点 | 文件系统 | 内容 |
|---|---|---|
| `/dev` | `devfs`（`SimpleFs`） | null/zero/full/random/urandom/rtc0/fb0/tty/console/ptmx/pts/log*/memtrack*/cpu_dma_latency/loop0..15/input/* |
| `/dev/shm` | `tmpfs`（`MemoryFs`） | POSIX 共享内存文件 |
| `/tmp` | `tmpfs` | 临时文件（memfd 也落在这里） |
| `/proc` | `procfs`（`SimpleFs`） | `mounts/meminfo/meminfo2/instret/interrupts/sys/...` + `<pid>/{stat,status,oom_score_adj,task,maps,mounts,cmdline,comm,exe,fd}` |
| `/sys` | `tmpfs` | 额外创建 `/sys/class/graphics/fb0/device/subsystem → whatever` 软链接供 Xorg 探测 |

设计动机：不依赖磁盘即可提供 Linux 程序启动所需的 `/dev`、`/proc` 接口；`SimpleFs` 的目录项动态生成（`DirMaker` 闭包）使得 `/proc/<pid>` 这类"按访问时状态生成"的节点实现成本极低。

## 9. 信号

- 两层管理器（`starry_signal`）：
  - `ProcessSignalManager`（进程级：信号动作表 `SignalActions`、待处理进程信号、trampoline 地址）
  - `ThreadSignalManager`（线程级：阻塞掩码、线程待处理信号、信号栈、`check_signals` 的实际执行）
- 投递：`send_signal_to_thread/process/process_group`（`task/signal.rs`），成功投递后 `task.interrupt()` 让目标任务从阻塞中返回处理信号。
- 处理时机：每次从用户态返回（`task/user.rs` 循环末尾 `while check_signals(...) {}`），以及在可中断等待（`interruptible`）被唤醒时。
- 用户栈帧：`starry_signal` 负责构造 `SignalFrame`，RET 落到 `SIGNAL_TRAMPOLINE`（由 `load_user_app` 映射到每个用户地址空间）。
- 特殊处理：`block_next_signal()` 用于 `rt_sigreturn` 后避免同一信号被立即重复处理。

## 10. 时间与定时器

- 每个线程一个 `TimeManager`（`task/timer.rs`），统计 `utime_ns/stime_ns`，维护 3 个 interval timer（`ITIMER_REAL/VIRTUAL/PROF`）。
- 状态机 `TimerState::{None,User,Kernel}` 在用户态/内核态切换时由 `task/user.rs` 设置；`poll()` 按状态累计时间并按到期情况发 `SIGALRM/SIGVTALRM/SIGPROF`。
- 全局 `ALARM_LIST: BinaryHeap<Entry>` + `EVENT_NEW_TIMER` 事件，由 `alarm_task`（`spawn_raw` 创建的内核任务）驱动；`Entry.task` 用 `WeakAxTaskRef` 避免阻止任务退出。
- 定时器到期时 `poll_timer(task)` 在 alarm 任务上下文里代目标任务推进时间并发信号。

## 11. 同步原语

| 机制 | 位置 | 说明 |
|---|---|---|
| `Mutex`/`RwLock`/`SpinNoIrq` | `axsync` / `kspin` / `spin` | 通用锁；单核假设下大量使用 |
| `PollSet`/`Pollable`/`IoEvents` | `axpoll` | fd 就绪通知；`poll_io` 组合成可取消的 future |
| `block_on(poll_io(...))` | `axtask::future` | 内核态阻塞的统一写法；不阻塞 CPU |
| futex | `task/futex.rs` | 私有 futex 表（每进程）+ 共享 futex 表（按 `SharedPages`/文件句柄区分） |
| `WaitQueue` | `task/futex.rs` | `VecDeque<(Waker, u32 bitset)>`，支持 bitset/requeue |

## 12. System V IPC

- **消息队列**：`syscall/ipc/msg.rs` 的 `MSG_MANAGER: Mutex<MsgManager>`，`key ↔ msqid`、`msqid → Arc<Mutex<MessageQueue>>`；消息按 `mtype` 放在 `BTreeMap<i64, Vec<Message>>`。
- **共享内存**：`syscall/ipc/shm.rs` 的 `SHM_MANAGER: Mutex<ShmManager>`，维护 `key ↔ shmid`（`BiBTreeMap`）、`shmid → ShmInner`、`pid → shmid ↔ vaddr`；物理页由 `SharedPages` 持有，`shmat` 用 `Backend::new_shared` 映射。
- **id 分配**：`syscall/ipc/mod.rs` 的 `next_ipc_id()`（全局原子自增，msg 与 shm 共用）。
- **回收**：`shmdt`/进程退出（`do_exit` 调 `clear_proc_shm`）时解除映射；`IPC_RMID` 在最后一个 attach 消失后真正删除。

## 13. 网络

- `file/net.rs` 的 `Socket(axnet::Socket)` 把 TCP/UDP/Unix(Stream/Dgram)/vsock 统一成 `FileLike`。
- `syscall/net/*` 负责 ABI 转换：
  - `addr.rs`：`sockaddr` ↔ `SocketAddrEx`（INET/INET6/UNIX/VSOCK），处理抽象 Unix socket 与末尾 NUL。
  - `io.rs`：send/recv/sendmsg/recvmsg，含 `SCM_RIGHTS` 控制消息（fd 传递）。
  - `opt.rs`：socket option 的 level/optname 分派与类型转换（宏 `call_dispatch!`）。
  - `socket.rs`：创建/绑定/连接/监听/accept/shutdown/socketpair。
- 阻塞与超时由 `axnet` 的 `SocketOps` + `Pollable` 提供；`connect` 的 `WouldBlock` 被映射成 `EINPROGRESS`。

## 14. 设备与显示

- `pseudofs::Device` 把 `DeviceOps` 适配成 VFS 节点；`DeviceMmap` 决定 mmap 行为（`None/Physical/ReadOnly/Cache`）。
- `/dev/fb0`：`fb.rs` 直接操作 `axdisplay::framebuffer_info()` 的帧缓冲 VA，并提供 `FBIOGET_VSCREENINFO`/`FBIOGET_FSCREENINFO` 等 ioctl；`mmap` 走 `DeviceMmap::Physical` → `Backend::Linear`。
- 刷新：`fb.rs` 起一个 60Hz 的 `fb-refresh` 任务调用 `axdisplay::framebuffer_flush()`。
- `/dev/input/*`：`event.rs` 把 `axinput` 的输入设备包装成 evdev，支持 `EVIOCG*` 与 `input_event` 读取；鼠标设备（含 `BTN_MOUSE`）挂到 `mice`。
- `/dev/loopX`：`loop.rs` 把普通文件包装成块设备（`LOOP_SET_FD` 等 ioctl），供 `mount` 使用。

## 15. 构建与产物

```text
make all
 ├── prepare          : 生成 .cargo/config.toml（vendor 离线源）+ 恢复 vendor checksum
 ├── rv               : cargo build --release --target riscv64gc-unknown-none-elf
 │                       --features "qemu,axfeat/bus-mmio"
 │                      → target/riscv64gc-unknown-none-elf/release/starryos
 │                    → scripts/mkphyself 重写 PT_LOAD 地址与 e_entry → ./kernel-rv
 └── la               : cargo build --release --target loongarch64-unknown-none-softfloat
                        --features qemu
                       → target/loongarch64-unknown-none-softfloat/release/starryos
                     → scripts/mkphyself → ./kernel-la
```

- 工具链固定 `nightly-2025-05-20`（`rust-toolchain.toml`），目标是 `*-none-elf` / `*-none-softfloat`。
- 依赖全部来自 `vendor/`（`config.toml.temp` 的 source replace），**离线构建**。
- `mkphyself` 的必要性：内核链接在高半区虚拟地址，而 QEMU `-kernel` 需要按物理地址装载；它把每个 `PT_LOAD` 的 `p_vaddr/p_paddr` 减去 VA→PA 偏移（`.percpu` 的 vaddr=0 例外保留），并把 `e_entry` 设为物理入口。
- QEMU 启动参数见 `Makefile` 的 `run-rv` / `run-la`（块设备 virtio-blk、网卡 virtio-net user 模式、`-rtc base=utc`）。

## 16. 关键不变量（速查）

完整版见 `PRINCIPLES.md` 与 `CONSTRAINTS.md`，最重要的几条：

1. 用户指针只能通过受控的 VM 访问接口读写；内核态访问用户内存必须包在 `access_user_memory` 中。
2. 修改用户页表/地址空间必须持有 `proc_data.aspace` 锁，且不能持锁做长阻塞 I/O。
3. 所有 `FileLike` 实现必须满足：阻塞模式下 `read/write` 用 `poll_io` 等待，非阻塞模式返回 `WouldBlock`。
4. `FD_TABLE` / `FS_CONTEXT` 只能通过当前 scope 访问，不能把某个进程的引用缓存到全局静态。
5. 任务退出/进程退出必须走 `task::do_exit`，不要自行清理，否则 futex、信号、SHM、父进程通知会漏。
6. 新增 syscall 必须：加 `Sysno` 分支、实现 `sys_*`、返回 `AxResult<isize>`、错误经 `-errno`、必要时在对应文件文档登记。
7. 任何架构相关代码必须用 `#[cfg(target_arch = ...)]` 隔离，且保证四种 arch 至少能通过编译（当前 CI/构建覆盖 rv/la）。
