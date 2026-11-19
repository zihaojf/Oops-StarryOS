# `kernel/src/pseudofs/dev/tty/ptm.rs`

> 对应源码：`kernel/src/pseudofs/dev/tty/ptm.rs`
> 所属子系统：pty 主端（`/dev/ptmx`）
> 依赖：`pseudofs::{Device, DeviceOps, SimpleFs}`、`pseudofs/dev/tty/pty`
> 被依赖：`pseudofs/dev/mod.rs`（注册 `/dev/ptmx`）、`syscall/fs/fd_ops.rs`（打开时特殊处理）

## 1. 文件职责

`/dev/ptmx` 的占位设备：每次 `open("/dev/ptmx")` 时由 `syscall/fs/fd_ops.rs` 调用 `Ptmx::create_pty()` 创建一对新 pty，把 master 作为打开的文件返回、slave 注册到 `/dev/pts/N`。

```rust
pub struct Ptmx(pub Arc<SimpleFs>);
impl Ptmx {
    pub fn create_pty(&self) -> AxResult<(Arc<Device>, u32)> {
        let (master, slave) = create_pty_pair();
        add_slave(self.0.clone(), slave)?;
        let pty_number = master.pty_number();
        let device = Device::new(self.0.clone(), CharacterDevice, DeviceId::new(128, pty_number), master);
        Ok((device, pty_number))
    }
}
```

## 2. 设计要点

- `Ptmx` 自身是 `DeviceOps`，但 `read_at/write_at/ioctl` 都是 `unreachable!()`：**打开后永远不会使用 Ptmx 这个实例**，`fd_ops` 会把它替换成新建的 `Device`。
- `DeviceId::new(128, pty_number)` 是 master 的设备号；slave 用 `136:N`（`pts::add_slave` 里设置）。
- `create_pty` 返回的 `Arc<Device>` 被 `fd_ops` 包成 `FileNode` + `Reference`（名为 pty 号），从而 `/proc/<pid>/fd/N` 显示 `/dev/pts/N`。
- `add_slave` 失败（表满 16 个）→ `TooManyOpenFiles`。

## 3. 不变量与约束

- **`unreachable!()` 的契约**：`fd_ops::add_to_fd` 必须先检测 `Ptmx` 并替换；任何绕过 `sys_openat` 直接持有 `Ptmx` 的代码都会 panic。
- `create_pty` 必须在打开时调用一次（不能重复创建同一实例）。
- `Ptmx` 持有 `Arc<SimpleFs>`（devfs），因为 `Device::new` 需要它；不要改成全局。
- `ptmx` 的 `DeviceId (5,2)` 在 `dev/mod.rs` 注册；master 的 `(128,N)` 与 slave 的 `(136,N)` 是 Linux 惯例。

## 4. 与其他模块的关系

| 模块 | 关系 |
|---|---|
| `pseudofs/dev/tty/pty.rs` | `create_pty_pair` |
| `pseudofs/dev/tty/pts.rs` | `add_slave`、`PTS_TABLE` |
| `pseudofs/dev/mod.rs` | 注册 `/dev/ptmx` |
| `syscall/fs/fd_ops.rs` | 打开时替换：`ptmx.create_pty()` → `DirEntry::new_file` → `Location` → `File` |
| `pseudofs/dev/tty/mod.rs` | `pty_number()` |

## 5. 已知局限 / TODO

- 只有打开时创建，没有 `unlockpt`/`grantpt` 的真实语义（`TIOCSPTLCK` 是 no-op）。
- 无 `/dev/pts/N` 的权限（属主）设置。
- `Ptmx` 的 `DeviceOps` 实现是 panic 占位（依赖外部替换，脆弱）。

## 6. 变更影响面

改本文件 → `pseudofs/dev/tty/{pty,pts,mod}.rs.md`、`syscall/fs/fd_ops.rs.md`、`pseudofs/dev/mod.rs.md`。
