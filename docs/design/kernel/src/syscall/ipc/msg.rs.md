# `kernel/src/syscall/ipc/msg.rs`

> 对应源码：`kernel/src/syscall/ipc/msg.rs`
> 所属子系统：System V 消息队列
> 依赖：`super::{IpcPerm, has_ipc_permission, next_ipc_id, IPC_*, MSG_*}`、`BTreeMap`、`axsync::Mutex`、`monotonic_time_nanos`、`starry_vm::{vm_load, vm_write_slice}`
> 被依赖：`syscall/ipc/mod.rs`、`syscall/mod.rs`

## 1. 文件职责

实现 `msgget/msgsnd/msgrcv/msgctl` 与消息队列管理。

## 2. 关键类型

```rust
pub struct msqid_ds { msg_perm: IpcPerm, msg_stime/rtime/ctime, msg_cbytes/qnum/qbytes, msg_lspid/lrpid }
pub struct Message { pub mtype: i64, pub data: Vec<u8> }
pub struct MessageQueue {
    pub msqid_ds: msqid_ds,
    pub messages: BTreeMap<i64, Vec<Message>>,   // mtype → 同类型消息（FIFO）
    pub total_bytes: usize,
    pub mark_removed: bool,
}
pub struct MsgManager {
    key_msqid: BTreeMap<i32, i32>,
    msqid_queues: BTreeMap<i32, Arc<Mutex<MessageQueue>>>,
}
pub const MSGMNI: usize = 32000;  // 队列数上限
pub const MSGMNB: usize = 16384;  // 单队列字节上限
pub const MSGMAX: usize = 8192;   // 单消息上限
pub static MSG_MANAGER: Mutex<MsgManager>;
```

`MsgRcvFlags`：`IPC_NOWAIT(0o4000)`、`MSG_NOERROR(0o10000)`、`MSG_COPY(0o20000)`、`MSG_EXCEPT(0o2000)`；`MsgSndFlags`：`IPC_NOWAIT`。

## 3. 关键流程

### 3.1 `sys_msgget(key, msgflg)`

```text
若队列总数 >= MSGMNI → ENOSPC
key == IPC_PRIVATE → 直接新建（msqid = next_ipc_id），不建 key 映射
已存在 key：
    取队列；权限（读）不通过 → EACCES
    mark_removed → EIDRM
    (msgflg & IPC_EXCL) && (msgflg & IPC_CREAT) → EEXIST
    返回 msqid
不存在：
    无 IPC_CREAT → ENOENT
    新建 + 建 key 映射
```

### 3.2 `sys_msgsnd(msqid, msgp, msgsz, msgflg)`

```text
msgsz > MSGMAX → EINVAL
取队列（无 → EINVAL）；权限（写）→ EACCES
读 mtype（vm_read）；<= 0 → EINVAL
读 mtext（vm_load(msgsz)）
若 total_bytes + len > msg_qbytes 或 msg_qnum+1 > msg_qbytes：
    IPC_NOWAIT → EAGAIN
    否则 warn!（阻塞发送未实现）→ EAGAIN
enqueue_message（再次检查字节上限）
更新 msg_lspid/msg_stime
warn!("wakeup of waiting receivers not implemented")
返回 0
```

### 3.3 `sys_msgrcv(msqid, msgp, msgsz, msgtyp, msgflg)`

- 校验 `MSG_COPY` 必须配 `IPC_NOWAIT`，且与 `MSG_EXCEPT` 互斥。
- 匹配规则：
  | msgtyp | 行为 |
  |---|---|
  | `0` | 第一条消息（最小 mtype） |
  | `> 0` + 无 EXCEPT | 该类型的第一条 |
  | `> 0` + EXCEPT | 第一个类型 != msgtyp 的消息 |
  | `< 0` | 类型 <= \|msgtyp\| 的最小类型的第一条 |
  | `MSG_COPY` | `msgtyp` 当作索引（0-based，按 mtype 顺序） |
- 无匹配：`IPC_NOWAIT` → `ENOMSG`；否则 `warn!` + `ENOMSG`（阻塞接收未实现）。
- 大小检查：`data.len() > msgsz` 且无 `MSG_NOERROR` → `E2BIG`。
- 写回 `mtype` + `mtext`（`copy_len = min(len, msgsz)`）。
- 普通模式移除消息并更新 `msg_lrpid/msg_rtime`；`MSG_COPY` 不移除。
- `warn!("wakeup of waiting senders not implemented")`。

### 3.4 `sys_msgctl(msqid, cmd, buf)`

| cmd | 行为 |
|---|---|
| `IPC_INFO` | 用局部 `MsgInfo` 返回系统限制（`msgmax/msgmnb/msgmni`） |
| `MSG_INFO` | 返回临时 `msqid_ds`：`msg_cbytes = total_bytes`、`msg_qnum = queue_count`、返回队列数 |
| `MSG_STAT` | `msqid` 当作第 n 个 active 队列的索引；写其 `msqid_ds`，返回真实 msqid |
| `IPC_STAT` | 权限（读）后写 `msqid_ds` |
| `IPC_SET` | 需 owner/creator/root；更新 `uid/gid/mode&0o777`、`msg_qbytes`（超 `MSGMNB` 需 root）、`msg_ctime` |
| `IPC_RMID` | 标记 `mark_removed`；队列空则立即删除；否则等消息取空（但**没有实现**自动删除逻辑） |
| 其他 | `EINVAL` |

## 4. 不变量与约束

- **锁顺序（C20/C32）**：`MSG_MANAGER` → `MessageQueue(Mutex)`；持 `MSG_MANAGER` 时不能阻塞。
- **`MSGMNB` 双重含义**：既限制字节数，又被 `msgsnd` 用来限制消息条数（`msg_qnum+1 > msg_qbytes`，注释说明这是 Linux 实际行为）。
- **阻塞收发未实现**：`msgsnd/msgrcv` 在队列满/空时返回 `EAGAIN/ENOMSG`（`warn!`），调用方必须用 `IPC_NOWAIT` 或重试。
- `IPC_RMID` 的"延迟删除"：队列非空时只标记 `mark_removed`，之后 `msgrcv` 会因 `mark_removed` 返回 `EIDRM`（**无法再取空**），因此延迟删除逻辑不完整。
- `MessageQueue::enqueue_message` 会再次检查字节上限（调用方已检查，冗余但安全）。
- `find_message_less_equal` 遍历 `BTreeMap` 找 <= abs_typ 的最小类型；`MSG_COPY` 的索引顺序 = `BTreeMap` 的 mtype 顺序（稳定）。
- **C6**：`UserMsgbuf` 的 `mtype/mtext` 用 `addr_of!` + `vm_read/vm_load`；`mtext` 是柔性数组（`[u8; 0]`）。
- `msqid_ds` 布局必须与 Linux 一致（`IpcPerm` 开头）。

## 5. 与其他模块的关系

| 模块 | 关系 |
|---|---|
| `syscall/ipc/mod.rs` | `IpcPerm/next_ipc_id/has_ipc_permission/常量` |
| `syscall/sys.rs` | `sys_getuid/getgid`（权限判断） |
| `syscall/mod.rs` | 分派 |
| `mm/access.rs` | `vm_load/vm_write_slice`（`starry_vm`） |
| `pseudofs/proc.rs` | 未暴露 `/proc/sysvipc/msg` |

## 6. 已知局限 / TODO

- 阻塞收发未实现（`warn!` + 立即错误）。
- 无等待队列，无法唤醒等待者。
- `IPC_RMID` 延迟删除不完整（非空队列标记后无法清空）。
- `msgctl(MSG_STAT)` 的索引语义（active 队列的第 n 个）与 Linux 一致，但 `IPC_STAT` 用 msqid 查找。
- `msg_ctime/stime/rtime` 用 `monotonic_time_nanos` 当秒/纳秒（单位混乱，`__kernel_time_t` 是秒）。
- 无 `MSG_COPY` 的 `IPC_NOWAIT` 自动补全（要求调用方显式传）。
- 无 namespace（IPC_NS）。

## 7. 变更影响面

改本文件 → `ipc/mod.rs.md`、`syscall/mod.rs.md`、`ARCHITECTURE.md#12`、`CONSTRAINTS.md` C6/C20/C32。
