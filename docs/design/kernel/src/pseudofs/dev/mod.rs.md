# `kernel/src/pseudofs/dev/mod.rs`

> 对应源码：`kernel/src/pseudofs/dev/mod.rs`
> 所属子系统：devfs（`/dev`）
> 依赖：`pseudofs::{Device, DeviceOps, SimpleFs, SimpleDir, DirMapping}`、`rand`、`axdisplay`
> 被依赖：`pseudofs/mod.rs`（`mount_all` → `new_devfs`）、`file/fs.rs`（`/dev/console`）

## 1. 文件职责

构造 `/dev` 设备树，注册所有字符/块设备节点与子目录：

| 节点 | 类型 | `DeviceId` | 实现 |
|---|---|---|---|
| `null` | chr | 1:3 | `Null`（读 0、写丢弃） |
| `zero` | chr | 1:5 | `Zero`（读全 0） |
| `full` | chr | 1:7 | `Full`（写返回 `StorageFull`） |
| `random`/`urandom` | chr | 1:8 / 1:9 | `Random`（`SmallRng`，固定种子） |
| `rtc0` | chr | 250:0 | `rtc::Rtc` |
| `fb0` | chr | 29:0 | `fb::FrameBuffer`（仅 `axdisplay::has_display()`） |
| `tty` | chr | 5:0 | `tty::CurrentTty`（打开时解析到当前终端） |
| `console` | chr | 5:1 | `tty::N_TTY`（全局控制台） |
| `ptmx` | chr | 5:2 | `tty::Ptmx`（每次 open 创建 pty） |
| `pts` | dir | — | `tty::PtsDir` |
| `log` | socket | — | `SimpleFile` 空内容（`dev-log` feature） |
| `memtrack` | chr | 114:514 | `memtrack::MemTrack`（`memtrack` feature） |
| `cpu_dma_latency` | chr | 10:1024 | `CpuDmaLatency`（写接受、读 EINVAL） |
| `shm` | dir | — | 空目录（被 `/dev/shm` tmpfs 覆盖挂载） |
| `loop0..15` | blk | 7:0 | `loop::LoopDevice` |
| `input/` | dir | — | `event::input_devices`（`input` feature） |

## 2. 关键类型

```rust
pub(crate) fn new_devfs() -> Filesystem { SimpleFs::new_with("devfs", 0x01021994, builder) }
const RANDOM_SEED: &[u8; 32] = b"0123456789abcdef0123456789abcdef";
```

各 `DeviceOps` 实现要点：

- `Null`/`Zero`/`Random`/`Full`：`flags = NON_CACHEABLE | STREAM`。
- `CpuDmaLatency`：只写；读 `InvalidInput`。
- `Random`：`Mutex<SmallRng>`（固定种子 → 可复现，非密码学安全）。

## 3. 不变量与约束

- **`flags` 必须设置 `NON_CACHEABLE|STREAM`**：否则 `read` 会被页缓存/预读影响（`/dev/zero` 会读到缓存）。
- **`DeviceId` 必须与 Linux 惯例一致**：`/dev/pts/N` 用 136:N、`/dev/ptmx` 用 5:2、`/dev/console` 用 5:1；用户态工具（如 `login`/`agetty`）依赖这些值。
- `Ptmx`/`CurrentTty` 是"打开时替换"的占位设备：`syscall/fs/fd_ops.rs::add_to_fd` 检测 `downcast::<Device>().as_any()` 后替换成真实 pty/console 文件；本文件里的 `read_at/write_at/ioctl` 实现为 `unreachable!()`/`Ok(0)`。
- `fb0` 只在 `axdisplay::has_display()` 时注册；无显示时 `/dev/fb0` 不存在。
- `loop0..15` 的 `DeviceId` 全部是 `7:0`（循环设备传统），`LoopDevice` 自己记录 `number` 与 `dev_id`。
- `cpu_dma_latency` 的 `DeviceId` 是 `10:1024`（与 Linux 一致）。
- `shm` 目录在 devfs 中创建，但 `mount_all` 会在其上挂 tmpfs；`/dev/shm` 的实际内容由 tmpfs 决定。
- `input/` 子目录的 `mice` 与 `eventN` 命名由 `event::input_devices` 决定（含 `BTN_MOUSE` 的设备归入 `mice`）。

## 4. 与其他模块的关系

| 模块 | 关系 |
|---|---|
| `pseudofs/mod.rs` | `mount_all` 挂载 `/dev` |
| `pseudofs/device.rs` | `Device`/`DeviceOps` |
| `pseudofs/dev/{event,fb,log,loop,memtrack,rtc}.rs` | 具体设备 |
| `pseudofs/dev/tty/*` | tty/ptmx/pts/console |
| `file/mod.rs` | `add_stdio` 打开 `/dev/console` |
| `syscall/fs/fd_ops.rs` | ptmx/current-tty 的打开替换逻辑 |
| `syscall/mm/mmap.rs` | fb0/loop 的 mmap |

## 5. 已知局限 / TODO

- 没有 `/dev/console` 之外的虚拟终端（`/dev/tty1..N`）。
- `random` 是固定种子的 `SmallRng`，不可用于安全场景。
- 没有设备热插拔（input 设备在启动时枚举一次）。
- `memtrack`/`log`/`input` 由 feature 控制，默认构建不含。
- `loop0..15` 数量硬编码。

## 6. 变更影响面

改本文件 → `pseudofs/device.rs.md`、`pseudofs/dev/*.md`、`file/mod.rs.md`、`syscall/fs/fd_ops.rs.md`、`CONSTRAINTS.md` C28。
