# `kernel/src/syscall/ipc/mod.rs`

> 对应源码：`kernel/src/syscall/ipc/mod.rs`
> 所属子系统：System V IPC（公共定义）
> 依赖：`linux_raw_sys::{ctypes::*, general::*}`、`bytemuck::AnyBitPattern`
> 被依赖：`ipc/msg.rs`、`ipc/shm.rs`、`syscall/mod.rs`

## 1. 文件职责

1. 声明并重导出 `msg`/`shm`；
2. 全局 IPC id 分配器 `next_ipc_id()`；
3. IPC 常量（`IPC_PRIVATE/CREAT/EXCL/RMID/SET/STAT/INFO`、`MSG_STAT/MSG_INFO`、权限位）；
4. `IpcPerm` 结构（与 Linux ABI 一致）与权限判断 `has_ipc_permission`。

## 2. 关键内容

```rust
static IPC_ID: AtomicI32 = AtomicI32::new(0);
fn next_ipc_id() -> i32 { IPC_ID.fetch_add(1, Relaxed) }

const IPC_PRIVATE: i32 = 0;
const IPC_CREAT: i32 = 0o1000;  IPC_EXCL: 0o2000;
const IPC_RMID: i32 = 0;  IPC_SET: i32 = 1;  IPC_STAT: i32 = 2;  IPC_INFO: i32 = 3;
const MSG_STAT: i32 = 11;  MSG_INFO: i32 = 12;
const USER_READ: u32 = 0o400; ... OTHER_WRITE: 0o002;

#[repr(C)] #[derive(Clone, Copy, AnyBitPattern)]
pub struct IpcPerm {
    key: __kernel_key_t, uid, gid, cuid, cgid: __kernel_uid_t/gid_t,
    mode: __kernel_mode_t, seq: c_ushort, pad: c_ushort,
    unused0: c_long, unused1: c_long,
}

fn has_ipc_permission(perm, uid, gid, is_write) -> bool {
    uid == 0 → true;
    perm.uid == uid → (mode & (is_write ? USER_WRITE : USER_READ)) != 0
    perm.gid == gid → 同理 GROUP_*
    否则 → OTHER_*
}
```

## 3. 设计要点

- **id 从 0 开始自增**：`msgget`/`shmget` 共用同一计数器；Linux 的 id 也是递增的，但起点由内核分配（这里从 0 开始，第一个对象 id 为 0——与 `IPC_PRIVATE` 的 key 无关，可接受）。
- **权限检查**：root 全放行；否则按 owner/group/other 位；与 Linux 语义一致（未实现 capability）。
- `IpcPerm` 的字段类型来自 `linux_raw_sys`，布局与内核 `struct ipc_perm` 一致（含 `seq/pad/unused`）。

## 4. 不变量与约束

- **`IpcPerm` 必须是 `#[repr(C)]` 且字段顺序不可改**：`IPC_STAT` 直接把 `msqid_ds/shmid_ds` 的整体拷给用户，内部包含 `IpcPerm`。
- `IPC_RMID/SET/STAT` 的数值（0/1/2）必须与 Linux 一致；`MSG_STAT/MSG_INFO` 是 Linux 扩展值（11/12）。
- `next_ipc_id` 用 `Relaxed` 足够（单核 + 仅用于生成 id）。
- `has_ipc_permission` 的 `is_write` 决定用 READ 还是 WRITE 位；调用方必须传对。
- 常量 `IPC_PRIVATE = 0` 与"key 为 0 表示私有"语义绑定；不要改成负数。

## 5. 与其他模块的关系

| 模块 | 关系 |
|---|---|
| `ipc/msg.rs` | 消息队列（用 `IpcPerm/next_ipc_id/has_ipc_permission`） |
| `ipc/shm.rs` | 共享内存（同上） |
| `syscall/mod.rs` | 分派 `msg*/shm*` |
| `linux_raw_sys` | 类型定义 |

## 6. 已知局限 / TODO

- 权限模型不完整（无 capability、无 `CAP_IPC_OWNER`）。
- id 回绕（i32 溢出）未处理。
- 无 `IPC_INFO` 的完整实现（仅 msg 部分实现）。

## 7. 变更影响面

改本文件 → `ipc/{msg,shm}.rs.md`、`syscall/mod.rs.md`、`ARCHITECTURE.md#12`。
