# StarryOS 文档入口

本目录是 StarryOS 仓库的**文档入口**，分为两类：

| 目录/文件 | 作用 | 读者 |
|---|---|---|
| [`design/`](./design/README.md) | 设计文档体系：总体架构、设计原则、硬约束、逐文件设计说明 | **AI Agent（必读）**、人类开发者 |
| [`design/ARCHITECTURE.md`](./design/ARCHITECTURE.md) | 系统总体架构、启动流程、关键数据流 | 新人、Agent、评审人 |
| [`design/PRINCIPLES.md`](./design/PRINCIPLES.md) | 设计思路与不变量（为什么这样写） | 评审人、Agent |
| [`design/CONSTRAINTS.md`](./design/CONSTRAINTS.md) | 硬性约束（不能违反的规则） | **Agent（每次任务前必查）** |
| [`design/AGENT-GUIDE.md`](./design/AGENT-GUIDE.md) | Agent 工作流：如何读文档、改代码、回写文档 | **Agent（每次任务前必读）** |
| [`design/GLOSSARY.md`](./design/GLOSSARY.md) | 术语表 | 所有人 |
| [`design/TRACEABILITY.md`](./design/TRACEABILITY.md) | 源码文件 ↔ 设计文档一一映射表 | 所有人、检查脚本 |
| [`design/kernel/src/`](./design/kernel/src/) | 与 `kernel/src/**` 一一对应的逐文件设计文档 | 改内核的人 |
| [`design/src/`](./design/src/) | 与顶层 `src/**` 一一对应的设计文档 | 改入口/init 的人 |
| [`team-docker.md`](./team-docker.md) | Docker 开发环境配置（团队约定） | 所有人 |
| [`x11.md`](./x11.md) | X11 图形环境使用说明 | 需要 GUI 的人 |

## 文档范围（重要）

设计文档**只覆盖核心源代码**，即：

- `src/**`（顶层 crate：`main.rs`、`init.sh`）
- `kernel/src/**`（内核全部 Rust 源码）

**不覆盖**：`Makefile`、`Cargo.toml`、`configs/`、`scripts/`、`make/`、`.github/`、`vendor/`、镜像与产物。
这些属于构建/工具链/协作基础设施，改动时以 `README.md`、`docs/team-docker.md` 与文件内注释为准；不要求逐文件设计文档。

## 30 秒上手

1. **人类**：先读 [`design/ARCHITECTURE.md`](./design/ARCHITECTURE.md)，再按需查 `design/kernel/src/` 下对应文件的文档。
2. **Agent**：任何任务开始前，必须按顺序读
   - `docs/design/AGENT-GUIDE.md`
   - `docs/design/CONSTRAINTS.md`
   - 与本次改动相关的 `docs/design/kernel/src/...` 或 `docs/design/src/...` 文件级文档
3. 改完代码后，**必须**同步更新对应文件的设计文档（规则见 AGENT-GUIDE 第 4 节）。

## 文档映射规则（唯一规则）

> **对任意核心源文件 `P`（位于 `src/` 或 `kernel/src/` 下），其设计文档固定为 `docs/design/P.md`。**

| 源码路径 | 设计文档 |
|---|---|
| `src/main.rs` | `docs/design/src/main.rs.md` |
| `src/init.sh` | `docs/design/src/init.sh.md` |
| `kernel/src/lib.rs` | `docs/design/kernel/src/lib.rs.md` |
| `kernel/src/task/mod.rs` | `docs/design/kernel/src/task/mod.rs.md` |
| `kernel/src/syscall/ipc/shm.rs` | `docs/design/kernel/src/syscall/ipc/shm.rs.md` |

规则简单到可以脚本化检查，完整清单见 [`design/TRACEABILITY.md`](./design/TRACEABILITY.md)。

## 维护约定

- 文档与源码**同一次改动**内一起提交（不允许"下次补文档"）。
- 文档里出现的路径、类型名、函数名必须与当前源码一致。
- 文档只描述**当前已实现的行为**；未实现的写在"已知局限/TODO"小节，不要写成已实现。
- 新增核心源文件时：同时新增 `docs/design/<同样相对路径>.md` 并更新 `TRACEABILITY.md`。
- 删除/重命名核心源文件时：同步删除/重命名对应文档并更新 `TRACEABILITY.md`。
- 所有文档使用 Markdown + 代码块，中文叙述 + 英文标识符。
