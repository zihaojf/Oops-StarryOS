# 术语表

> 按主题分组。缩写/类型名与源码保持一致。

## 总体

| 术语 | 含义 |
|---|---|
| ArceOS | 组件化 unikernel 框架；StarryOS 的上层依赖集合（`ax*` crates） |
| axplat | ArceOS 的平台抽象 crate，如 `axplat-riscv64-qemu-virt` |
| unikernel | 与应用同地址空间/同一镜像形态的内核；本项目中仅指 ArceOS 基础层 |
| 内核态 / 用户态 | `uctx.run()` 之前/之后；对应 `TimerState::{Kernel,User}` |
| syscall | 系统调用；入口 `syscall::handle_syscall` |
| errno | 负的错误码约定；`AxError → LinuxError::code()` |
| ABI | 应用二进制接口；结构体布局/寄存器约定/调用约定 |
| StarryOS 语义层 | `kernel/src/**` 中实现 Linux 行为的代码，不含硬件驱动 |

## 进程与任务

| 术语 | 含义 |
|---|---|
| `TaskInner` | `axtask` 的调度实体（内核栈 + 上下文） |
| `Thread` | StarryOS 的线程扩展，`Box<Thread>` 放进 `TaskInner::task_ext` |
| `ProcessData` | 一个进程内所有线程共享的数据（`Arc<ProcessData>`） |
| `Process` | `starry_process` 的进程对象：pid、父子关系、线程集合、退出码 |
| `ProcessGroup` / `Session` | 进程组 / 会话；`starry_process` 提供 |
| `Pid` | 进程/线程/组/会话的统一 id 类型（`starry_process::Pid`） |
| init 进程 | pid 即 init 任务 id 的进程；`Process::new_init` |
| zombie | 已退出但未被父进程 `wait` 回收的进程 |
| `clone` 子进程 | Linux 语义：`exit_signal != SIGCHLD` 的子进程 |
| group_exit | 退出整个线程组（`exit_group`），向其他线程发 `SIGKILL` |
| `clear_child_tid` | `set_tid_address`/`CLONE_CHILD_CLEARTID` 设置；退出时清零并唤醒 futex |
| robust list | `set_robust_list` 注册的 futex 链表；线程死亡时标记 owner-dead |
| `Scope` | `scope_local` 的资源作用域；承载 `FD_TABLE`、`FS_CONTEXT` |
| `ActiveScope` | 当前任务的活动作用域；由 `Thread::on_enter/on_leave` 切换 |

## 内存

| 术语 | 含义 |
|---|---|
| `AddrSpace` | 用户地址空间：`va_range + MemorySet<Backend> + PageTable` |
| VMA | 虚拟内存区域；本项目中是 `memory_set::MemoryArea<Backend>` |
| `Backend` | 映射后端枚举：`Linear/Cow/Shared/File` |
| `BackendOps` | 后端操作 trait，`enum_dispatch` 静态分派 |
| `MemorySet` | ArceOS 的 VMA 集合：查找/分裂/合并/映射 |
| COW | copy-on-write；`Backend::Cow`，写时复制并降权 |
| `FRAME_TABLE` | COW 的物理帧引用计数表（`BTreeMap<PhysAddr, Arc<SpinNoIrq<FrameRefCnt>>>`） |
| `SharedPages` | 一组共享物理页（`Vec<PhysAddr>` + `PageSize`） |
| `CachedFile` | `axfs` 的文件页缓存句柄 |
| 驱逐（evict） | 页缓存回收某页；`FileBackend` 注册 listener 同步解除用户映射 |
| `PageSize` | `Size4K/Size2M/Size1G`；由 `MAP_HUGETLB` 选择 |
| `MappingFlags` | 页权限：READ/WRITE/EXECUTE/USER |
| `PageFaultFlags` | 缺页访问类型；`flags.contains(access_flags)` 判定 |
| 惰性分配 | 只登记 VMA，缺页时才 `populate` |
| `populate` | backend 分配/填充页并建立映射的入口 |
| `copy_from_kernel` | 把内核地址空间复制进用户页表（aarch64/loongarch64 不需要） |
| `SIGNAL_TRAMPOLINE` | 信号返回蹦床的固定用户虚拟地址 |

## 文件与 VFS

| 术语 | 含义 |
|---|---|
| `FileLike` | 所有 fd 的统一 trait（读/写/stat/ioctl/path/nonblocking） |
| `FD_TABLE` | fd 表，`FlattenObjects<FileDescriptor, 1024>`，scope-local |
| `FlattenObjects` | 连续 id 分配容器；`add/add_at/get/remove/ids` |
| `FileDescriptor` | `{ inner: Arc<dyn FileLike>, cloexec: bool }` |
| `FS_CONTEXT` | `axfs::FsContext`，root/cwd/mount 树，scope-local |
| `Location` | `axfs_ng_vfs` 的"挂载点 + 目录项"定位对象 |
| `DirEntry` / `DirNode` / `FileNode` | VFS 对象模型：目录项 / 目录节点 / 文件节点 |
| `NodeOps` | 节点元数据与操作 trait（inode/metadata/update_metadata/flags） |
| `FileNodeOps` | 文件读写操作（read_at/write_at/append/set_len/set_symlink/ioctl） |
| `DirNodeOps` | 目录操作（read_dir/lookup/create/link/unlink/rename） |
| `DeviceOps` | 设备操作（read_at/write_at/ioctl/as_any/mmap/flags） |
| `DirMaker` | `Arc<dyn Fn(WeakDirEntry) -> Arc<dyn DirNodeOps>>`，目录节点工厂 |
| `NodeOpsMux` | `Dir(DirMaker) | File(Arc<dyn FileNodeOps>)` |
| `SimpleFs` | 伪文件系统基类（slab inode + 根 DirEntry） |
| `SimpleDir` / `SimpleFile` / `Device` | 基于 `SimpleFs` 的目录/文件/设备实现 |
| `SimpleDirOps` | 伪目录的内容提供者（`child_names`/`lookup_child`/`is_cacheable`） |
| `tmpfs` / `MemoryFs` | 内存文件系统，挂 `/tmp`、`/sys`、`/dev/shm` |
| `devfs` | `/dev` 的 `SimpleFs` 实例 |
| `procfs` | `/proc` 的 `SimpleFs` 实例 |
| 页缓存 | `axfs` 的 `CachedFile`；读写走缓存 |
| `O_*` / `AT_*` | open/at 系列 flag，来自 `linux_raw_sys` |
| `cloexec` | close-on-exec 标志；`execve` 时关闭 |

## I/O 与同步

| 术语 | 含义 |
|---|---|
| `Pollable` | 可被 poll/epoll/select 等待的对象 |
| `IoEvents` | 就绪事件位集；`IN/OUT/ERR/HUP/ALWAYS_POLL/RDNORM/WRNORM` |
| `PollSet` | 一组 waker；`register/wake` |
| `poll_io` | `axtask::future` 的组合器：先检查，未就绪则注册 waker 并等待 |
| `block_on` | 在当前任务上驱动一个 future 到完成 |
| `interruptible` | 可被信号打断的 future（`wait4`、`nanosleep` 用） |
| futex | 快速用户态互斥量；本项目的等待队列 `WaitQueue` 挂在 `FutexEntry` |
| `FutexKey` | `Private{address}` 或 `Shared{offset, region}` |
| `bitset` | futex `WAIT_BITSET/WAKE_BITSET` 的匹配掩码 |
| requeue | `FUTEX_REQUEUE/CMP_REQUEUE`：把等待者移到另一个 futex |
| `Ldisc` | line discipline；tty 的输入处理层 |
| `Termios` / `Termios2` | 终端属性（VINTR/ICANON/ECHO…） |
| `JobControl` | 终端的会话/前台进程组管理 |
| PTY / PTM / PTS | 伪终端 / 主端 / 从端 |
| `Epoll` interest | `epoll_ctl(ADD)` 建立的一条关注项 |
| LT / ET / ONESHOT | epoll 三种触发模式 |
| `eventfd` | 64 位计数器 fd，可作信号量 |
| `signalfd` | 用 fd 读取待处理信号 |

## 信号与时间

| 术语 | 含义 |
|---|---|
| `SignalSet` | 信号位集 |
| `SignalInfo` | `siginfo` 的内核表示（signo/code/errno/payload） |
| `SignalActions` | 进程级信号动作表（handler/mask/flags） |
| trampoline | `rt_sigreturn` 的入口；`SIGNAL_TRAMPOLINE` |
| `SignalFrame` | 信号处理时压在用户栈上的上下文 |
| `ITimerType` | `Real/Virtual/Prof`，对应 `ITIMER_*` |
| `TimeManager` | 每线程时间统计 + 3 个 interval timer |
| `TimerState` | `None/User/Kernel`，决定时间计入 utime 还是 stime |
| `ALARM_LIST` | 全局定时器最小堆（`BinaryHeap<Entry>`） |
| alarm task | 驱动 `ALARM_LIST` 的内核任务（`spawn_raw`） |
| `timespec` / `timeval` / `itimerval` | Linux 时间结构体；`TimeValueLike` 负责转换 |
| `TimeValue` | `axhal::time` 的时间值（secs + nanos） |

## IPC 与网络

| 术语 | 含义 |
|---|---|
| `MSGMNI/MSGMNB/MSGMAX` | 消息队列系统限制（32000/16384/8192） |
| `msqid_ds` | 消息队列状态结构（与 Linux ABI 一致） |
| `IpcPerm` | IPC 对象权限结构（key/uid/gid/mode/seq） |
| `MSG_MANAGER` | 全局消息队列管理器 |
| `SHM_MANAGER` | 全局共享内存管理器 |
| `BiBTreeMap` | 双向 BTreeMap（key↔value 都可查） |
| `shmid_ds` | 共享内存段状态结构 |
| `shm_nattch` | 当前 attach 计数 |
| `IPC_PRIVATE/IPC_CREAT/IPC_EXCL/IPC_RMID/IPC_SET/IPC_STAT` | System V IPC 命令/标志 |
| `sockaddr` / `sockaddr_in` / `sockaddr_in6` / `sockaddr_un` / `sockaddr_vm` | socket 地址 ABI |
| `SocketAddrEx` | `axnet` 的地址枚举：`Ip/Unix/Vsock` |
| `cmsghdr` / `SCM_RIGHTS` | 控制消息 / 传递 fd |
| `msg_iov` / `iovec` | 分散聚集 I/O 向量 |
| `SOCK_STREAM/DGRAM/SEQPACKET` | socket 类型 |
| `SOL_SOCKET` / `IPPROTO_TCP` / `IPPROTO_IP` | socket option level |

## 设备与显示

| 术语 | 含义 |
|---|---|
| `DeviceMmap` | 设备的 mmap 行为：`None/Physical/ReadOnly/Cache` |
| evdev | Linux 输入事件设备；`/dev/input/eventN` |
| `EVIOCG*` | evdev ioctl（名称/ID/位图/按键状态） |
| fbdev | 帧缓冲设备；`/dev/fb0`，`FBIOGET_*` ioctl |
| `loop` 设备 | 把文件当块设备；`LOOP_SET_FD` 等 |
| `MemTrack` | `/dev/memtrack`：写 `start\n`/`end\n` 触发分配统计 |
| `RTC_RD_TIME` | 读 RTC 时间的 ioctl |

## 构建

| 术语 | 含义 |
|---|---|
| `mkphyself` | 把高半区内核 ELF 的 PT_LOAD 地址/入口改写成物理地址的工具 |
| VA→PA 偏移 | `virt_to_phys` 常量；mkphyself 的第三个参数 |
| vendor | 离线依赖目录；`.cargo/config.toml` 的 source replace |
| `AX_*` 环境变量 | ArceOS 构建/运行时配置（`AX_CONFIG_PATH/AX_ARCH/AX_PLATFORM/AX_LOG/AX_IP/AX_GW/AX_MODE`） |
| `qemu` feature | 顶层 crate 的默认板级特性集合（display/input/vsock/bus-pci…） |
| `smp` feature | 可选多核支持 |
| `vf2` feature | VisionFive2 板级支持（riscv64） |
| LTO | `make LTO=y` 打开 release LTO |
| DWARF | `make DWARF=y` 保留调试信息 + `dwarf.sh` 后处理 |
