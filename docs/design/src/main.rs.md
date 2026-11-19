# `src/main.rs`

> 对应源码：`src/main.rs`（crate `starryos`，二进制入口）
> 所属：顶层 crate / 启动入口
> 依赖：`starry_kernel::entry::init`、`axfs::FS_CONTEXT`、`src/init.sh`
> 被依赖：无（程序入口）

## 1. 文件职责

这是整个操作系统的**唯一二进制入口**（`[[bin]] name = "starryos"`，`required-features = ["qemu"]`）：

1. 在 rootfs 上挑选一个可用的 shell 作为 init 程序；
2. 组装 init 进程的 `argv`；
3. 调用 `starry_kernel::entry::init(&args, &envs)`，此后不再返回。

## 2. 关键常量

```rust
pub const CMDLINE: &[&str] = &["/bin/sh", "-c", include_str!("init.sh")];
const SHELL_CANDIDATES: &[&str] = &["/bin/sh", "/musl/busybox", "/glibc/busybox", "/busybox"];
```

| 常量 | 作用 |
|---|---|
| `CMDLINE` | 标准形态：`/bin/sh -c <init.sh 全文>`；`init.sh` 在编译期内联 |
| `SHELL_CANDIDATES` | 竞赛镜像只有 busybox，因此按优先级回退 |

`init.sh` 被内联意味着：改 `src/init.sh` 必须重新编译内核；它不是 rootfs 上的文件。

## 3. 关键流程

```rust
fn main() {
    let shell = SHELL_CANDIDATES.iter().copied()
        .find(|path| rootfs_contains(path))
        .unwrap_or(CMDLINE[0]);            // 默认 /bin/sh
    let mut args = vec![shell.to_owned()];
    if shell != "/bin/sh" { args.push("sh".to_owned()); }   // busybox 需要 applet 名
    args.extend(CMDLINE[1..].iter().copied().map(str::to_owned)); // "-c", init.sh
    let envs = [];
    starry_kernel::entry::init(&args, &envs);
}
```

`rootfs_contains(path)` 通过 `axfs::FS_CONTEXT.lock().resolve(path).is_ok()` 判断文件是否存在——此时 rootfs 已由 ArceOS 挂载（ext4 on virtio-blk）。

## 4. 不变量与约束

- **args[0] 必须是可执行文件路径**：`entry::init` 会 `FS_CONTEXT.resolve(&args[0])` 并 `load_user_app`，失败即 panic。
- **busybox 形态**：当 shell 不是 `/bin/sh` 时，`argv = [busybox, "sh", "-c", init.sh]`，使 busybox 以 `sh` applet 运行。
- `CMDLINE[1..]` 固定是 `["-c", init.sh]`；若改 init 启动方式，需同时满足 `entry::init` 的假设（args[0] 为程序、后续为参数）。
- `main()` 不能返回：`entry::init` 内部 `task.join()` 后直接关机路径。若改成返回，`axruntime` 行为未定义。
- **C31**：本文件不含任何 git 相关逻辑。

## 5. 与其他模块的关系

- `kernel/src/entry.rs`：真正的初始化逻辑。
- `src/init.sh`：被 `include_str!` 内联，决定 init 进程行为（普通 shell / 竞赛测试 runner）。
- `axfs::FS_CONTEXT`：rootfs 解析。
- `Cargo.toml`：`qemu` feature 决定是否构建本 binary；`vf2` feature 引入 VisionFive2 平台 crate。

## 6. 已知局限 / TODO

- 只探测 4 个固定路径；没有解析 `/etc/passwd`/`/sbin/init`。
- 不传环境变量（`envs = []`），`HOME/PATH/TERM` 全由 `init.sh` 自己 export。
- 若 rootfs 无 shell，会 fallback 到 `/bin/sh` 并在 `entry::init` 里 panic（没有更友好的错误）。
- `init.sh` 内联导致无法在运行期替换 init 脚本（除 `execve` 自己启动的脚本外）。

## 7. 变更影响面

改本文件 → `docs/design/src/init.sh.md`、`docs/design/kernel/src/entry.rs.md`、`README.md`（启动说明）。
