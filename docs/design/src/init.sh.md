# `src/init.sh`

> 对应源码：`src/init.sh`（被 `src/main.rs` 用 `include_str!` 内联）
> 所属：init 进程脚本（busybox sh）
> 运行环境：`/bin/sh -c <本文件内容>`（或 busybox sh）

## 1. 文件职责

init 进程的行为脚本，分两种模式：

1. **普通 rootfs**（无 `*_testcode.sh`）：打印欢迎信息，`cd /root`，`exec /bin/sh --login`。
2. **竞赛测试镜像**：搭最小运行环境（静态 sh、glibc/musl 动态加载器、`LD_LIBRARY_PATH`），按顺序跑所有 `*_testcode.sh`，最后关机。

## 2. 环境变量

```sh
export HOME=/root
export PATH=/bin:/usr/bin:/sbin:/usr/sbin:/glibc:/musl
export TERM=linux
```

## 3. 关键逻辑

### 3.1 探测

- `BB`：依次找 `/musl/busybox`、`/glibc/busybox`、`/busybox`，取第一个可执行。
- `TESTDIRS`：对 `/`、`/glibc`、`/musl` 检查是否存在 `*_testcode.sh`，收集目录。
- 若 `TESTDIRS` 为空 → 普通 init 模式。

### 3.2 `prepare_environment`

- `$BB mkdir -p /bin /lib /lib64`；
- 把 busybox 复制成 `/bin/sh`、`/bin/busybox`（满足脚本 `#!/bin/sh`）；
- glibc：把 `/glibc/lib/ld-linux-*.so.1` 复制到 `/lib` 与 `/lib64`；
- musl：把 `/musl/lib/libc.so` 复制成 `/lib/ld-musl-riscv64-sf.so.1` 或 `/lib64/ld-musl-loongarch-lp64d.so.1`（按 `uname -m`）。

### 3.3 `run_testcode dir script limit`

- `cd $dir`（脚本用相对路径引用工具）；
- 按目录设置 `LD_LIBRARY_PATH=/musl/lib` 或 `/glibc/lib`；
- 用 `$BB timeout -s KILL <limit> /bin/sh ./<script>` 限时执行；
- 打印开始/结束与状态码（失败不中断后续）。

### 3.4 执行顺序（重要）

```text
FAST_ORDER   basic lua libcbench libctest busybox cyclictest           (limit 240s)
MEDIUM_ORDER iperf netperf [iozone]                                    (limit 240s)
LONG_ORDER   [unixbench]                                               (limit 600s)
```

- LoongArch：跳过 `iozone`（会 hang）与 `unixbench`；`LONG_ORDER` 为空。
- watchdog：非 LoongArch 且有 busybox 时，后台 `sleep 1800; poweroff`，正常结束时 kill。
- 结尾 `$BB sync; exit 0` → 触发 `entry::init` 的 `join` 返回 → 卸载 + flush + 关机。

### 3.5 测试点选择（`TEST_ONLY` / `TEST_SKIP`）

两个变量，留空即"全部"，`TEST_SKIP` 优先：

```sh
TEST_ONLY="ltp"                                            # 白名单：只跑这些
TEST_SKIP="ltp/clone* ltp/close_range*"                    # 黑名单：从里面剔除
```

token 用空格分隔、支持 shell 通配，几种写法：

```text
ltp                          整轮 LTP
basic / libctest / unixbench 套件（也可写 basic_testcode.sh）
musl / glibc                 libc 变体（= 测试目录）
basic-musl                   套件 + 变体
ltp/pipe02                   单个 LTP 用例；ltp/clone* 表示一批
```

常见组合：

```sh
TEST_ONLY="ltp" TEST_SKIP="ltp/clone*"        # LTP 里去掉所有 clone 用例
TEST_ONLY="musl basic libctest"               # 只跑 musl 的两个套件
TEST_SKIP="unixbench iozone ltp"              # 除了这三样全跑
```

当前仓库里的默认值是**调试配置**：`TEST_ONLY="ltp"`，
`TEST_SKIP="ltp/clone* ltp/close_range* ltp/cpuctl_def_task* ltp/cpuacct_task ltp/cpuacct.sh"`
——前两项会 panic / 自旋死锁，后三项是 LTP 的 cgroup CPU helper（裸跑会对整个进程组发
`SIGUSR1`，曾把 init 打死导致整机静默关机）。正式评分前把两行都改成空字符串。

生效的选择会打到串口（`#### StarryOS: tests only="..." skip="..." ####`），日志里能看到这次跑了什么。
因为 `init.sh` 是 `include_str!` 内联进内核的，改这两个值必须重新 `make all`。

## 4. 不变量与约束

- 脚本必须以 `exit 0` 结束（正常路径），否则 init 退出码非 0（仅日志可见）。
- 所有测试点必须"失败不中断"（`set -e` 未开启，刻意为之）。
- 不得假设 `/bin/sh` 已存在（竞赛镜像没有，必须先 `prepare_environment`）。
- 不得依赖 `sleep/timeout` 在 LoongArch 上可靠（定时器不完整，故跳过 watchdog 与部分测试）。
- **`include_str!` 内联**：修改本文件后必须重新 `make all` 才生效。
- 选择只做"筛"：不改顺序、不改超时、不动内建 hang 跳过表（`LTP_SKIP`）；未选中的用例不会打印 `RUN/FAIL LTP CASE`。
- `init.sh` 没有信号保护（内核还没实现 PID 1 语义）：LTP 的 cgroup helper 一句 `kill(-pgid, SIGUSR1)`
  就能打死 init，而 init 一退出内核就 `system_off()`（`axtask::run_queue::exit_current` 的 `is_init()` 分支），
  表现为"串口突然没了、QEMU 退出码 0、无 END/panic"。当前靠 `TEST_SKIP` 把这类 helper 剔掉，内核侧待修。

## 5. 与其他模块的关系

- `src/main.rs` 内联本文件；
- `kernel/src/entry.rs` 在其退出后做 unmount/flush；
- `kernel/src/syscall/task/execve.rs`：`$BB timeout ...` 依赖 `fork/execve/wait4`；`$BB sync` 依赖 `sync`（当前是 dummy）；
- `scripts/ci-test.py` 是另一条"进 shell 再 exit"的冒烟路径，与竞赛 mode 不同。

## 6. 已知局限 / TODO

- 只支持 busybox；没有 `/bin/sh` 的真实 rootfs 走普通模式。
- 测试点集合由 `TEST_ONLY` / `TEST_SKIP` 选择（见 3.5），但可选的名字仍来自 `FAST/MEDIUM/LONG_ORDER` 与镜像里的脚本，新增测试仍需改脚本并重编。
- `poweroff`/`reboot` 依赖 `sys_reboot`（当前所有 cmd 都 `system_off`）。
- LoongArch 的跳过列表是为了规避已知 hang，不是架构能力差异。

## 7. 变更影响面

改本文件 → 重新构建；同步 `docs/design/src/main.rs.md`、`README.md` 的测试说明、`docs/design/kernel/src/syscall/sys.rs.md`（reboot/sync 语义）。
