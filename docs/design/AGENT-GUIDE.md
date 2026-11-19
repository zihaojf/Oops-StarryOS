# AI Agent 工作指南（StarryOS）

> 本文件是 Agent 在 StarryOS 仓库做任何改动前的**强制阅读项**。
> 配套：`CONSTRAINTS.md`（硬约束编号 C1–C34）、`ARCHITECTURE.md`（结构）、`TRACEABILITY.md`（映射表）。

## 0. 一页速查

```text
读: docs/design/AGENT-GUIDE.md → CONSTRAINTS.md → 目标文件文档 → 上下游文件文档
改: 源码 + 对应 docs/design/<同路径>.md（同一次改动）
验: make all（至少） / cargo fmt --check / 一致性脚本
禁: git 操作、改 vendor、新增在线依赖、裸解引用用户指针、panic 表示未实现
报: 改了什么 / 为什么 / 影响面 / 文档更新点
```

## 1. 任务开始前（必做）

1. 读 `docs/design/AGENT-GUIDE.md`（本文件）。
2. 读 `docs/design/CONSTRAINTS.md`，记住与本次改动相关的编号。
3. 读 `docs/design/ARCHITECTURE.md` 中相关章节（不知道从哪读就看第 2 节分层图）。
4. 找到目标文件，读 `docs/design/<目标文件路径>.md`：
   - "文件职责" — 确认它确实是该改的地方；
   - "不变量与约束" — 确认改动不破坏契约；
   - "变更影响面" — 确认还要改哪些文件/文档。
5. 读上游/下游文件的文档（"与其他模块的关系"小节会点名）。
6. 如果目标文件没有文档：**先补文档**（按 `docs/design/README.md` 模板），再改代码。

## 2. 任务规划

- 把需求拆成"最小可验证改动集"，每个改动集对应 **一个源码文件 + 一份文档**。
- 涉及跨模块（如新增 syscall）时，按 `ARCHITECTURE.md` 第 6 节列出的链路逐层确认：
  `syscall/mod.rs` → `syscall/<子系统>` → `file`/`mm`/`task`。
- 涉及 ABI（结构体、errno、flags）时，先查 `linux_raw_sys` 与 Linux man page，**不要凭记忆**。

## 3. 写代码时的自检清单

| 检查项 | 对应约束 |
|---|---|
| 有没有裸解引用用户指针？ | C6 |
| 内核访问用户内存是否在 `access_user_memory` 内？ | C7 |
| 持有 `aspace` 锁时有没有调用可能阻塞的接口？ | C8/C32 |
| 新 fd 是否实现 `FileLike + Pollable`、`path()`？ | C12 |
| 阻塞语义是否与 `nonblocking()` 一致？ | C13 |
| 是否有等待者需要在关闭/退出时唤醒？ | C14 |
| 任务退出是否走 `do_exit`？ | C16 |
| 新 syscall 是否返回 `AxResult<isize>`、错误转负 errno？ | C21 |
| 未实现分支是否 `warn!` + `Unsupported`，而不是 panic/静默成功？ | C22 |
| 长度/大小参数是否校验？ | C24 |
| 新增架构相关代码是否 `#[cfg(target_arch)]` 隔离？ | C1/C4 |
| 是否引入 `std`/新在线依赖/更新工具链特性？ | C1/C3/C5 |

## 4. 改完代码后（强制）

### 4.1 必须更新的文档

**规则：核心源码文件 `P`（`src/` 或 `kernel/src/` 下）的文档是 `docs/design/P.md`。**

| 改动类型 | 必须更新 |
|---|---|
| 修改行为/函数/类型/常量 | `docs/design/<该文件>.md` 对应小节 |
| 新增核心源文件 | 新建 `docs/design/<同路径>.md` + 更新 `TRACEABILITY.md` |
| 删除/重命名核心源文件 | 删除/重命名文档 + 更新 `TRACEABILITY.md` |
| 改变模块边界/数据流 | `ARCHITECTURE.md` 对应章节 + 相关子系统 `mod.rs.md` |
| 新增/修改硬约束或例外 | `CONSTRAINTS.md`（并在文件文档"不变量"同步） |
| 新增 syscall 支持列表变化 | `syscall/mod.rs.md` + 具体子系统文档的"支持列表" |
| 仅构建/配置/脚本（`Makefile`、`Cargo.toml`、`configs/`、`scripts/`、`make/`、`.github/`） | **不要求**设计文档；同步更新 `README.md`/文件内注释即可 |

### 4.2 文件文档的更新方式

- 只改"事实"：函数名、类型名、常量值、调用链、约束。
- 保留结构与小节标题；不要整篇重写（除非结构已变）。
- 在文末 `## 变更记录` 追加一行：
  ```text
  - <日期或提交号>: <一句话说明>
  ```
- 若某功能从"未实现"变"已实现"，从"已知局限"移到正文，并在"变更记录"注明。

### 4.3 验证

```bash
# 1) 编译（至少一个架构；两个都要尽量）
make rv            # 或 make all
# 2) 格式
cargo fmt --all -- --check
# 3) 文档完整性（应无输出）
find src kernel/src -name '*.rs' | while read -r f; do
  [ -f "docs/design/$f.md" ] || echo "MISSING docs/design/$f.md"
done
# 4) 文档中的路径是否仍存在（抽查本次改动涉及的路径）
```

> 注意：**不要**在 Agent 会话里执行 `git`（C31）。需要提交时，把改动清单交给人类。

## 5. 报告格式（交付给人类）

```text
## 改动摘要
- <文件>: <一句话>

## 设计决策
- 为什么这样改（引用 PRINCIPLES/CONSTRAINTS 编号）

## 影响面
- 受影响的模块 / syscall / 用户可见行为

## 文档更新
- docs/design/... （逐条列出）

## 验证
- make all: 结果
- cargo fmt: 结果
- 未验证项及原因

## 风险与遗留 TODO
- ...
```

## 6. 禁止事项（硬性）

1. 禁止在 StarryOS 仓库执行 `git`（add/commit/push/checkout/stash…），除非用户在当前会话明确要求并确认。
2. 禁止修改 `vendor/`、`.git/`、`target/`、`kernel-rv`、`kernel-la`、`*.img`。
3. 禁止用 `panic!`/`unwrap()` 表示"功能未实现"。
4. 禁止绕过 `do_exit` 自行清理任务。
5. 禁止把进程私有资源放进跨进程全局静态（C11）。
6. 禁止为了让代码编译而删除/放宽 `#[cfg]`、`assert!`、错误检查（除非有明确理由并写入文档）。
7. 禁止提交只有代码没有文档的改动（除非改动是纯注释/格式）。

## 7. 常见任务路径（Cheat Sheet）

| 任务 | 需要读的文档 | 通常要改的文件 |
|---|---|---|
| 新增一个 syscall | `ARCHITECTURE.md#6`、`syscall/mod.rs.md`、`syscall/<子系>/*.md` | `syscall/mod.rs`、`syscall/<子系>/*.rs`、两侧文档 |
| 修 mmap/page fault | `mm/aspace/mod.rs.md`、`backend/*.md`、`mm/access.rs.md` | `syscall/mm/mmap.rs`、`mm/aspace/**`、`mm/access.rs` |
| 加一种 fd | `file/mod.rs.md`、目标 fd 文档、`syscall/fs/fd_ops.rs.md` | `file/<新>.rs`、`file/mod.rs`、`syscall/fs/*` |
| 改信号语义 | `task/signal.rs.md`、`task/user.rs.md`、`syscall/signal.rs.md` | 对应文件 |
| 改调度/wait | `task/ops.rs.md`、`syscall/task/wait.rs.md`、`task/timer.rs.md` | 对应文件 |
| 改 /proc 输出 | `pseudofs/proc.rs.md`、`task/stat.rs.md` | `pseudofs/proc.rs` |
| 加设备节点 | `pseudofs/dev/mod.rs.md`、`pseudofs/device.rs.md` | `pseudofs/dev/mod.rs`、新设备文件 |
| 改构建/镜像 | 不属于本设计文档范围；看仓库 `README.md` 与 `Makefile`/`scripts/` 注释 | `Makefile`、`scripts/*` |
| 改 init 行为 | `src/main.rs.md`、`src/init.sh.md`、`kernel/src/entry.rs.md` | 对应文件 |

## 8. 升级为"长期任务"时

若任务跨多个子系统或需要多轮迭代：

1. 先产出**变更计划**（文件清单 + 文档清单 + 验证方式）给人类确认；
2. 每完成一个子系统就更新文档并自检，不要攒到最后；
3. 在最终报告中附"文档一致性检查"的结果；
4. 遇到与 `CONSTRAINTS.md` 冲突的设计，停止并输出冲突分析（不要擅自例外）。
