# `kernel/src/task/resources.rs`

> 对应源码：`kernel/src/task/resources.rs`
> 所属子系统：资源限制
> 依赖：`linux_raw_sys::{RLIM_NLIMITS, RLIMIT_NOFILE, RLIMIT_STACK}`
> 被依赖：`task/mod.rs`（`ProcessData.rlim`）、`syscall/resources.rs`（`prlimit64`）、`file/mod.rs`（`RLIMIT_NOFILE`）

## 1. 文件职责

实现 Linux 资源限制表：

```rust
pub const AX_FILE_LIMIT: usize = 1024;                 // 每个 fd 表硬上限
pub struct Rlimit { pub current: u64, pub max: u64 }   // soft / hard
pub struct Rlimits([Rlimit; RLIM_NLIMITS as usize]);   // 固定数组
```

## 2. 默认值

`Rlimits::default()`：

| 资源 | 默认值 | 说明 |
|---|---|---|
| `RLIMIT_STACK` | `config::USER_STACK_SIZE`（512KiB） | 与 `mm/loader.rs` 实际映射的栈大小一致 |
| `RLIMIT_NOFILE` | `AX_FILE_LIMIT`（1024） | 与 `FlattenObjects<_, 1024>` 容量一致 |
| 其他 | `Rlimit::default()`（0/0） | 未实现限制 |

`From<u64>` 把单个值同时设为 soft/hard。

## 3. 索引与访问

```rust
impl Index<u32> for Rlimits  { ... self.0[index as usize] }
impl IndexMut<u32> for Rlimits
```

- 调用方必须自行保证 `index < RLIM_NLIMITS`；`sys_prlimit64` 在入口检查 `resource >= RLIM_NLIMITS`。
- 越界会 panic（数组索引）——这是"调用方责任"的契约。

## 4. 不变量与约束

- **C24**：新增读取 `rlim[resource]` 的代码必须自己校验 `resource` 范围，或复用 `sys_prlimit64` 的检查。
- `AX_FILE_LIMIT` 必须与 `file/mod.rs` 的 `FlattenObjects<FileDescriptor, AX_FILE_LIMIT>` 常量一致；两处改一处会导致"可以打开超过表容量的 fd"或反之。
- `RLIMIT_STACK` 初值必须与实际栈映射一致（`mm/loader.rs` 用 `config::USER_STACK_SIZE`）。
- `Rlimit.max` 的语义是硬上限；`sys_prlimit64` 目前**不**阻止提高硬上限（注释 TODO），只阻止降低失败时静默返回 0——注意这是有意的宽松实现。

## 5. 与其他模块的关系

| 模块 | 关系 |
|---|---|
| `task/mod.rs` | `ProcessData.rlim: RwLock<Rlimits>`，`Default` 初始化 |
| `syscall/resources.rs` | `sys_prlimit64` 读写；校验 `rlim_cur <= rlim_max` |
| `file/mod.rs` | `add_file_like` 用 `rlim[RLIMIT_NOFILE].current` 判断 `TooManyOpenFiles` |
| `syscall/task/clone.rs` | 新进程继承默认 `Rlimits`（未复制父进程限制 —— **已知缺陷**） |

## 6. 已知局限 / TODO

- `clone` 时子进程使用 `Rlimits::default()` 而非父进程当前值（`ProcessData::new` 里 `rlim: RwLock::default()`）。
- 未实现 `RLIMIT_CPU/AS/DATA/CORE/...` 的实际约束。
- `sys_prlimit64` 对提高硬上限返回 `Ok(0)` 而不生效（`// TODO: patch resources`）。

## 7. 变更影响面

改本文件 → `task/mod.rs.md`、`syscall/resources.rs.md`、`file/mod.rs.md`、`CONSTRAINTS.md` C24。
