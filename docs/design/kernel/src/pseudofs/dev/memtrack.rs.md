# `kernel/src/pseudofs/dev/memtrack.rs`

> 对应源码：`kernel/src/pseudofs/dev/memtrack.rs`
> 所属子系统：内存追踪设备（`/dev/memtrack`）
> feature：`memtrack`（`#[cfg(feature = "memtrack")]`）
> 依赖：`axbacktrace::Backtrace`、`axalloc`（tracking）、`gimli`、`pseudofs::DeviceOps`

## 1. 文件职责

提供内存泄漏/占用分析入口：用户向 `/dev/memtrack` 写 `start\n` 开始记录分配，写 `end\n` 触发按调用栈分类的统计报告。

## 2. 关键流程

### 2.1 `write_at`

```text
只处理 offset == 0 且非空：
  b"start\n" → stamped = axalloc::current_generation(); STAMPED_GENERATION.store(...); enable_tracking()
  b"end\n"   → run_memory_analysis(); disable_tracking()
  其他       → 忽略
返回 buf.len()
```

### 2.2 `run_memory_analysis`

```text
yield_now()                      // 等 GC/任务清理
cleanup_task_tables()            // 清理 weak map 失效项
clear_elf_cache()                // 清理 ELF 缓存噪声
打印 Alive tasks
from = STAMPED_GENERATION; to = axalloc::current_generation()
allocations_in(from..to, |info| 分类 by backtrace)
  MemoryCategory::new(backtrace) → Known(name) 或 Unknown(backtrace)
按总字节降序打印：大小/次数/首个 Layout/分类
```

### 2.3 `MemoryCategory`

- `category(backtrace)`：遍历栈帧，找 `DW_LANG_Rust` 函数，按 demangle 名匹配已知分配点：
  `starry_core::mm::ElfLoader::load`→"elf cache"、`ProcessData::new`→"process data"、`Process::new`、`ProcessGroup::new`、`ext4 inode`、`CachedFile::get_or_create/page_or_insert`→"cached file"、`axtask::timers::set_alarm_wakeup`→"timer"、`DirNode::lookup_locked/create_locked`→"dentry"、`ext4_user_malloc`→"lwext4"。
- 未匹配 → `Unknown(Backtrace)`（`Display` 打印完整栈）。
- 注意：匹配名里的 `starry_core` 是**旧 crate 名**（当前是 `starry_kernel`），因此这些分类实际不会命中——见"已知局限"。

## 3. 数据

```rust
static STAMPED_GENERATION: AtomicU64;
enum MemoryCategory { Known(&'static str), Unknown(Backtrace) }
pub(crate) struct MemTrack;    // 零大小
```

## 4. 不变量与约束

- **C28/C33**：`write_at` 会调用 `enable_tracking`/`disable_tracking` 并做重分析，属于重操作；不要在其他路径调用。
- `STAMPED_GENERATION` 是"起始代"；必须 `start` → 操作 → `end` 的配对使用。
- `allocations_in(from..to)` 的区间是 `[from, to)`；`to` 在 `end` 时取当前代。
- `run_memory_analysis` 会修改全局状态（清理表/ELF 缓存），**只能在调试场景使用**；不要在正常运行时调用。
- `MemoryCategory::Known(&'static str)` 要求名字是 `'static`；新增分类用字符串字面量。
- `Backtrace` 的 `Ord` 由 `axbacktrace` 提供（`Unknown` 用其排序）。
- feature 未启用时，`/dev/memtrack` 不存在。

## 5. 与其他模块的关系

| 模块 | 关系 |
|---|---|
| `pseudofs/dev/mod.rs` | `memtrack` feature 下注册 |
| `task/ops.rs` | `cleanup_task_tables()` |
| `mm/loader.rs` | `clear_elf_cache()` |
| `axalloc` | `current_generation/allocations_in/enable_tracking` |
| `kernel/Cargo.toml` | `memtrack = ["axfeat/dwarf", "axalloc/tracking", "dep:gimli"]` |

## 6. 已知局限 / TODO

- 分类名使用旧 crate 名 `starry_core`，当前 crate 是 `starry_kernel` → **分类全部落到 `Unknown`**（应更新字符串）。
- 报告输出到串口（`ax_println!`），大数据量时很慢。
- 只支持 `start`/`end` 两个命令。
- 依赖 `dwarf` 特性才能解析栈帧；未启用时 `frames()` 返回 None。
- 无按大小/地址过滤，全量分析。

## 7. 变更影响面

改本文件 → `pseudofs/dev/mod.rs.md`、`task/ops.rs.md`、`mm/loader.rs.md`、`kernel/Cargo.toml`（feature）。
