# 源码 ↔ 设计文档 映射表

> 规则：**核心源文件 `P`（`src/` 或 `kernel/src/` 下）→ `docs/design/P.md`**。
> 本文列出全部已覆盖文件，供人工核对与 Agent 定位。新增/删除源文件时同步本表。

## 覆盖统计

| 范围 | 源文件数 | 文档数 |
|---|---|---|
| `src/` | 1 个 `.rs` + 1 个 `.sh` | 2 |
| `kernel/src/` | 104 | 104 |
| 合计（文件级） | 106 | 106 |
| 体系/元文档 | — | 6（`README/ARCHITECTURE/PRINCIPLES/CONSTRAINTS/AGENT-GUIDE/GLOSSARY`）+ 本表 |

## 验证命令

```bash
# 1) 缺失检查（应无输出）
find src kernel/src -name '*.rs' | while read -r f; do
  [ -f "docs/design/$f.md" ] || echo "MISSING docs/design/$f.md"
done

# 2) 多余检查（应无输出）
find docs/design/kernel docs/design/src -name '*.md' | while read -r d; do
  s="${d#docs/design/}"; s="${s%.md}"
  [ -f "$s" ] || echo "EXTRA $d"
done

# 3) 数量核对
echo "src=$(find src kernel/src -name '*.rs' | wc -l) docs=$(find docs/design/kernel docs/design/src -name '*.md' | wc -l)"
```

## 顶层 `src/`（2）

- [x] `src/main.rs` → `docs/design/src/main.rs.md`
- [x] `src/init.sh` → `docs/design/src/init.sh.md`（非 `.rs`，但属于核心启动逻辑，纳入文档）

## `kernel/src/` 根与配置（9）

- [x] `kernel/src/lib.rs`
- [x] `kernel/src/entry.rs`
- [x] `kernel/src/time.rs`
- [x] `kernel/src/config/mod.rs`
- [x] `kernel/src/config/aarch64.rs`
- [x] `kernel/src/config/loongarch64.rs`
- [x] `kernel/src/config/riscv64.rs`
- [x] `kernel/src/config/x86_64.rs`

## `kernel/src/task/`（8）

- [x] `task/mod.rs`、`task/ops.rs`、`task/futex.rs`、`task/resources.rs`
- [x] `task/signal.rs`、`task/stat.rs`、`task/timer.rs`、`task/user.rs`

## `kernel/src/file/`（8）

- [x] `file/mod.rs`、`file/fs.rs`、`file/pipe.rs`、`file/epoll.rs`
- [x] `file/event.rs`、`file/net.rs`、`file/pidfd.rs`、`file/signalfd.rs`

## `kernel/src/mm/`（10）

- [x] `mm/mod.rs`、`mm/access.rs`、`mm/io.rs`、`mm/loader.rs`
- [x] `mm/aspace/mod.rs`
- [x] `mm/aspace/backend/mod.rs`、`linear.rs`、`shared.rs`、`cow.rs`、`file.rs`

## `kernel/src/pseudofs/`（23）

- [x] `pseudofs/mod.rs`、`fs.rs`、`dir.rs`、`file.rs`、`device.rs`、`tmp.rs`、`proc.rs`
- [x] `pseudofs/dev/mod.rs`、`event.rs`、`fb.rs`、`log.rs`、`loop.rs`、`memtrack.rs`、`rtc.rs`
- [x] `pseudofs/dev/tty/mod.rs`、`ntty.rs`、`ptm.rs`、`pts.rs`、`pty.rs`
- [x] `pseudofs/dev/tty/terminal/mod.rs`、`job.rs`、`ldisc.rs`、`termios.rs`

## `kernel/src/syscall/`（47）

- [x] `syscall/mod.rs`、`resources.rs`、`signal.rs`、`sys.rs`、`time.rs`
- [x] `syscall/fs/mod.rs`、`ctl.rs`、`event.rs`、`fd_ops.rs`、`io.rs`、`memfd.rs`、`mount.rs`、`pidfd.rs`、`pipe.rs`、`signalfd.rs`、`stat.rs`
- [x] `syscall/io_mpx/mod.rs`、`epoll.rs`、`poll.rs`、`select.rs`
- [x] `syscall/ipc/mod.rs`、`msg.rs`、`shm.rs`
- [x] `syscall/mm/mod.rs`、`brk.rs`、`mincore.rs`、`mmap.rs`
- [x] `syscall/net/mod.rs`、`addr.rs`、`cmsg.rs`、`io.rs`、`name.rs`、`opt.rs`、`socket.rs`
- [x] `syscall/sync/mod.rs`、`futex.rs`、`membarrier.rs`
- [x] `syscall/task/mod.rs`、`clone.rs`、`clone3.rs`、`ctl.rs`、`execve.rs`、`exit.rs`、`job.rs`、`schedule.rs`、`thread.rs`、`wait.rs`

## 明确不建文档的路径（有意为之）

| 路径 | 原因 |
|---|---|
| `Makefile`、`Cargo.toml`、`kernel/Cargo.toml`、`rust-toolchain.toml`、`config.toml.temp`、`.axconfig.toml`、`configs/` | 构建/平台配置，不是核心源码；以 `README.md` 与注释为准 |
| `make/`、`scripts/`、`.github/` | 构建框架/工具/CI，不是内核语义代码 |
| `vendor/`、`target/`、`kernel-rv`、`kernel-la`、`*.img` | 第三方/产物，不可维护 |
| `README.md`、`CONTRIBUTING.md`、`docs/team-docker.md`、`docs/x11.md` | 项目说明类，非设计文档 |

## 维护流程（与 AGENT-GUIDE 第 4 节一致）

1. 新增核心源文件 → 新建 `docs/design/<同路径>.md`，并在本文对应分组追加条目。
2. 删除核心源文件 → 删除文档，并移除本文条目。
3. 重命名/移动 → 同步移动文档（保持 `docs/design/<同路径>.md` 关系）并更新本文。
4. 仅文档结构变化（如拆分小节）不需要更新本文。

## 变更记录

- 2026-11-01（基线 `0.2.0-preview.2`）：建立初始覆盖，106 个文件级文档 + 7 个体系文档（含本表）。
