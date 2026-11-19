# StarryOS 设计文档体系

本目录用于回答三个问题：

1. **这个系统是怎么设计的？**（架构、分层、关键数据流）
2. **为什么这么写？有哪些不能违反的约束？**（设计原则、不变量、硬约束）
3. **某个具体源文件负责什么？改它会影响谁？**（逐文件设计文档）

## 范围

设计文档只覆盖**核心源代码**：

- `src/**` —— 顶层 crate（`main.rs`、`init.sh`）
- `kernel/src/**` —— 内核全部源码

不覆盖 `Makefile` / `Cargo.toml` / `configs/` / `scripts/` / `make/` / `.github/` / `vendor/`。
这些属于构建与协作基础设施，改动时以仓库 `README.md`、`docs/team-docker.md` 与文件内注释为准。

## 目录结构

```text
docs/design/
├── README.md              # 本文件：体系说明与阅读顺序
├── ARCHITECTURE.md        # 总体架构、启动流程、跨模块数据流
├── PRINCIPLES.md          # 设计思路与不变量
├── CONSTRAINTS.md         # 硬约束清单（违反即视为错误改动）
├── AGENT-GUIDE.md         # AI Agent 工作流与文档回写规则
├── GLOSSARY.md            # 术语表
├── TRACEABILITY.md        # 源码 ↔ 文档 一一映射表
├── kernel/                # 与 kernel/src/** 一一对应
│   └── src/
│       ├── lib.rs.md
│       ├── entry.rs.md
│       ├── time.rs.md
│       ├── config/
│       ├── task/
│       ├── file/
│       ├── mm/
│       ├── pseudofs/
│       └── syscall/
└── src/                   # 与顶层 src/** 一一对应
    ├── main.rs.md
    └── init.sh.md
```

## 阅读顺序（按角色）

### 新人 / 评审人

1. `ARCHITECTURE.md` —— 先建立全局图景
2. `PRINCIPLES.md` —— 理解取舍
3. `kernel/src/lib.rs.md` → `kernel/src/entry.rs.md` —— 看启动主线
4. 按任务查 `kernel/src/<子系统>/mod.rs.md`，再下钻到具体文件

### AI Agent（每次任务）

1. `AGENT-GUIDE.md`（工作流）
2. `CONSTRAINTS.md`（硬约束）
3. `ARCHITECTURE.md` 的相关小节
4. 目标文件的 `<文件>.md`，以及其"上游/下游"文件文档
5. 动手改代码
6. **回写文档**（见 AGENT-GUIDE 第 4 节）

## 文档模板

每个文件级文档统一使用下面结构（小文件可裁剪合并小节）：

```markdown
# `kernel/src/xxx/yyy.rs`

> 对应源码：`kernel/src/xxx/yyy.rs`
> 所属子系统：子系统名
> 依赖：上游文件；被依赖：下游文件

## 1. 文件职责
## 2. 关键类型与数据
## 3. 关键流程（函数级）
## 4. 不变量与约束
## 5. 与其他模块的关系
## 6. 已知局限 / TODO
## 7. 变更影响面（改这里要一起改什么）
```

## 文档质量要求

- **可执行性**：Agent 只读文档就能知道"能不能改、怎么改、改哪里"。
- **可验证性**：所有断言要么来自源码，要么来自 `CONSTRAINTS.md`。
- **不过期**：与源码同一次改动更新；文档里的行为必须与代码一致。
- **不臆造**：未实现的功能只出现在"已知局限"里，并注明 `TODO`。

## 与源码一致性检查

在仓库根目录执行（只读，不修改任何文件）：

```bash
find src kernel/src -name '*.rs' | while read -r f; do
  [ -f "docs/design/$f.md" ] || echo "MISSING docs/design/$f.md"
done
```

若输出为空，说明所有核心 `.rs` 文件都有对应设计文档。完整清单见 `TRACEABILITY.md`。
