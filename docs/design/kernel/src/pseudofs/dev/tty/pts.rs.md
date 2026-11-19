# `kernel/src/pseudofs/dev/tty/pts.rs`

> 对应源码：`kernel/src/pseudofs/dev/tty/pts.rs`
> 所属子系统：pty 从端目录（`/dev/pts`）
> 依赖：`flatten_objects::FlattenObjects`、`kspin::SpinNoIrq`、`pseudofs::{Device, SimpleDirOps}`
> 被依赖：`pseudofs/dev/mod.rs`（注册 `pts` 目录）、`pseudofs/dev/tty/{pty,ptm}.rs`

## 1. 文件职责

维护 `/dev/pts/N` 的动态目录：

```rust
static PTS_TABLE: SpinNoIrq<FlattenObjects<Arc<Device>, 16>> = ...;

pub fn add_slave(fs: Arc<SimpleFs>, pty: Arc<PtyDriver>) -> AxResult<u32>;  // 分配 N，设置设备号
pub struct PtsDir;                                                          // SimpleDirOps
```

## 2. 关键流程

### 2.1 `add_slave`

```text
table.add(Device::new(fs, CharacterDevice, DeviceId::default(), pty)) → 索引 N（0..16）
terminal.pty_number.store(N, Release)
table.get(N).set_device_id(DeviceId::new(136, N))
返回 N
```

- 16 个槽位（`FlattenObjects<_, 16>`）：每次 `open("/dev/ptmx")` 占一个，slave 关闭后由 `Arc` 释放 → 槽位回收。
- 索引复用：`FlattenObjects` 会复用已释放的槽位；因此 `TIOCGPTN` 返回的编号可能复用（Linux 语义允许）。

### 2.2 `PtsDir`

- `child_names`：`PTS_TABLE` 的 `ids()` 转为字符串。
- `lookup_child`：`parse::<usize>()` → `table.get(id)` → `NodeOpsMux::File(pty)`（`Device` 实现 `FileNodeOps`）。

## 3. 不变量与约束

- **`set_device_id(136, N)` 必须与 `pty_number` 一致**：`/dev/pts/N` 的 `rdev` 用 136:N（Linux 惯例）；`TIOCGPTN` 读的是 `terminals.pty_number`。
- `PTS_TABLE` 用 `SpinNoIrq`：可在中断上下文安全（但当前只在 syscall 路径用）。
- `lookup_child` 的 `NodeOpsMux::File` 要求 `Device` 满足 `FileNodeOps + Send + Sync`；`Device` 已经是。
- 槽位上限 16（`FlattenObjects<_, 16>`）；超过返回 `TooManyOpenFiles`。
- `add_slave` 必须在 `create_pty_pair` 之后、返回 master 之前调用；否则 `/dev/pts/N` 不存在。
- `PtsDir` 不缓存（`SimpleDirOps` 默认 `is_cacheable = true`；但内容动态，**实际上应改为 false**——当前未改，见已知局限）。

## 4. 与其他模块的关系

| 模块 | 关系 |
|---|---|
| `pseudofs/dev/tty/pty.rs` | `create_pty_pair` |
| `pseudofs/dev/tty/ptm.rs` | `create_pty` 调 `add_slave` |
| `pseudofs/dev/mod.rs` | 注册 `pts` 目录 |
| `pseudofs/device.rs` | `Device` 作为文件节点 |
| `syscall/fs/fd_ops.rs` | 打开 `/dev/ptmx` 后把 master 的 `Reference` 指向 `/dev/pts/N` |

## 5. 已知局限 / TODO

- `PtsDir` 未设置 `is_cacheable = false`（动态目录被缓存后新 pty 不可见）。
- 槽位固定 16，不能配置。
- 无 `ptsname`/`unlockpt` 的完整语义。
- 权限/属主为默认值。

## 6. 变更影响面

改本文件 → `pseudofs/dev/tty/{pty,ptm,mod}.rs.md`、`syscall/fs/fd_ops.rs.md`、`pseudofs/dev/mod.rs.md`。
