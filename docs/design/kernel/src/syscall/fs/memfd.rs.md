# `kernel/src/syscall/fs/memfd.rs`

> 对应源码：`kernel/src/syscall/fs/memfd.rs`
> 所属子系统：memfd syscall
> 依赖：`axfs::{FS_CONTEXT, OpenOptions}`、`file::{File, FileLike}`、`mm::UserConstPtr`、`linux_raw_sys::general::MFD_CLOEXEC`
> 被依赖：`syscall/fs/mod.rs`

## 1. 文件职责

实现 `memfd_create(name, flags)`——当前是"用 `/tmp` 下的普通文件模拟"：

```text
for id in 0..0xffff:
    name = format!("/tmp/memfd-{id:04x}")
    在 FS_CONTEXT 中 resolve；不存在则创建（READ|WRITE|CREATE）并返回 fd
    （MFD_CLOEXEC → cloexec）
否则 TooManyOpenFiles
```

## 2. 设计要点

- **不是真正的匿名内存文件**：`// TODO: correct memfd implementation`。文件出现在 `/tmp`（tmpfs），有名字、可被其他进程打开。
- 探测-创建循环有竞争窗口（单核下无并发，安全；SMP 下可能撞名）。
- `flags` 只识别 `MFD_CLOEXEC`；`MFD_ALLOW_SEALING`/`MFD_HUGETLB` 忽略。
- `_name` 参数被完全忽略（未用于文件名）。

## 3. 不变量与约束

- **C11**：文件创建在 `FS_CONTEXT` 下（当前进程的 root/namespace）；不能直接用绝对路径 `open` 绕过 scope。
- 探测循环上限 `0xffff`（65535 个 memfd），超限 `TooManyOpenFiles`。
- `create(true)` 必须配合 `read(true).write(true)`（否则可能只创建不可读写）。
- 返回 fd 必须 `cloexec` 按 flags 设置；`MFD_CLOEXEC` 之外未知位被忽略（宽松）。
- 文件名格式 `memfd-XXXX` 可被用户看到（`ls /tmp`），与 Linux 的 `memfd:name (deleted)` 不同。

## 4. 与其他模块的关系

| 模块 | 关系 |
|---|---|
| `file/mod.rs` | `add_to_fd_table` |
| `pseudofs/tmp.rs` | `/tmp` 的 tmpfs |
| `pseudofs/mod.rs` | 挂载 `/tmp` |
| `syscall/fs/ctl.rs` | 文件可被 unlink/stat |
| `syscall/mm/mmap.rs` | memfd 可被 mmap |

## 5. 已知局限 / TODO

- 非匿名（有 `/tmp` 路径，可被 open）。
- 忽略 name 与大部分 flags。
- 无 sealing（`F_ADD_SEALS`/`F_GET_SEALS`）。
- 无 `MFD_HUGETLB`/`MFD_NOEXEC_SEAL`。
- 名字复用竞争（SMP）。

## 6. 变更影响面

改本文件 → `pseudofs/tmp.rs.md`、`file/mod.rs.md`、`syscall/fs/mod.rs.md`、`syscall/mm/mmap.rs.md`。
