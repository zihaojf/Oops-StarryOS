# StarryOS 团队开发环境配置（Docker）

> 本文只说明 **Docker 环境**。仓库结构、代码、构建目标不在本文重复；队友 `git pull` 后按本文配置即可。
>
> 目标：任何人拉下仓库后，能在同一个 Docker 环境里完成 `make all`、`make run-rv`、`make run-la`，并且不会因为工具链/镜像/换行符问题浪费半天。

---

## 0. 环境要求

| 项目 | 建议 |
|---|---|
| 宿主机 | Linux / WSL2（Windows）/ macOS |
| Docker | 24+，能正常 `docker run` |
| 磁盘 | 至少 40GB 空闲（镜像约 27GB + 测试镜像 8GB + target） |
| 内存 | 16GB 以上 |
| CPU | 8 核以上（`make all` 首次约 1–3 分钟） |
| 仓库位置 | **WSL/Linux 原生文件系统**，不要放在 `/mnt/c/...` |

镜像：

```text
zhouzhouyi/os-contest:20260510
```

镜像内已包含：

- QEMU 10.0.2（`qemu-system-riscv64` / `qemu-system-loongarch64`）
- `riscv64-linux-musl` 交叉工具链
- `loongarch64-linux-musl` 交叉工具链
- Rustup（默认工具链不是仓库需要的那个）

**重要**：镜像里 **没有** 仓库固定的 `nightly-2025-05-20`，需要按第 2 节一次性安装。

---

## 1. 拉取镜像和仓库

```bash
docker pull zhouzhouyi/os-contest:20260510

# 在 WSL/Linux 家目录下 clone，不要放 /mnt/c
git clone https://gitlab.eduxiji.net/T2026104869912739/oskernel2026-oops.git
cd oskernel2026-oops
```

如果是已有仓库：

```bash
git pull
```

---

## 2. 一次性安装 Rust 工具链

仓库的 `rust-toolchain.toml` 和 Makefile 固定使用：

```text
nightly-2025-05-20
targets: riscv64gc-unknown-none-elf, loongarch64-unknown-none-softfloat
```

有下面两种做法，**选一种即可**。

### 方案 A（推荐）：命名 volume，不重建镜像

优点：只装一次，之后所有容器复用；镜像本身不动。

```bash
# 1. 创建 volume
docker volume create starry-rustup

# 2. 把镜像自带的 rustup 目录复制进 volume，并在 volume 里装新工具链
docker run --rm -v starry-rustup:/rustup zhouzhouyi/os-contest:20260510 bash -lc '
  cp -a /root/.rustup/. /rustup/ &&
  RUSTUP_HOME=/rustup rustup toolchain install nightly-2025-05-20 --profile minimal &&
  RUSTUP_HOME=/rustup rustup target add --toolchain nightly-2025-05-20 \
      riscv64gc-unknown-none-elf loongarch64-unknown-none-softfloat &&
  RUSTUP_HOME=/rustup rustup toolchain list
'
```

如果下载慢，可以在命令前加国内镜像（二选一，失败就换回默认）：

```bash
RUSTUP_DIST_SERVER=https://mirrors.ustc.edu.cn/rust-static \
RUSTUP_UPDATE_ROOT=https://mirrors.ustc.edu.cn/rust-static/rustup \
RUSTUP_HOME=/rustup rustup toolchain install nightly-2025-05-20 --profile minimal
```

验证：

```bash
docker run --rm -v starry-rustup:/root/.rustup zhouzhouyi/os-contest:20260510 bash -lc '
  rustup toolchain list
  rustup run nightly-2025-05-20 rustc --version
'
```

以后启动容器时挂载：

```text
-v starry-rustup:/root/.rustup
```

### 方案 B：自建一个带工具链的镜像

优点：镜像自带工具链，队友不用管理 volume。

在仓库下建 `docker/Dockerfile`：

```dockerfile
FROM zhouzhouyi/os-contest:20260510

RUN rustup toolchain install nightly-2025-05-20 --profile minimal && \
    rustup target add --toolchain nightly-2025-05-20 \
        riscv64gc-unknown-none-elf loongarch64-unknown-none-softfloat
```

构建（注意 build context 用 `docker/`，避免把整个仓库和 8GB 镜像发进构建上下文）：

```bash
mkdir -p docker
# 把上面的 Dockerfile 内容保存到 docker/Dockerfile
docker build -t starry-dev:latest -f docker/Dockerfile docker
```

以后用 `starry-dev:latest` 启动即可，不需要 volume。

> 如果使用方案 B，后续命令中的镜像名替换成 `starry-dev:latest`。

---

## 3. 启动开发容器

在仓库根目录执行：

```bash
docker run --rm -it \
  --name starry-dev \
  --privileged \
  -v "$PWD":/work \
  -v starry-rustup:/root/.rustup \
  -w /work \
  zhouzhouyi/os-contest:20260510 bash
```

说明：

- `--privileged`：与官方 devcontainer 一致，方便 QEMU/设备访问；只做编译和 QEMU 用户态模拟时通常也可以去掉。
- `-v "$PWD":/work`：仓库挂载进容器，改代码在宿主机/VS Code 里改，容器里编译。
- `-v starry-rustup:/root/.rustup`：方案 A 的工具链；方案 B 不需要。
- 容器里默认是 root，`target/`、`kernel-rv`、`kernel-la` 会变成 root 属主；退出后可在宿主机执行：

  ```bash
  sudo chown -R "$USER:$USER" target kernel-rv kernel-la
  ```

  这是 Docker 下最省事的做法；**不要在 root 容器里执行 `git add/commit`**，否则 `.git` 属主会变。

### 容器内自检

```bash
rustc --version
rustup run nightly-2025-05-20 rustc --version

qemu-system-riscv64 --version
qemu-system-loongarch64 --version

which riscv64-linux-musl-cc
which loongarch64-linux-musl-cc
```

四个工具（QEMU ×2、musl cc ×2）都应该能找到。

---

## 4. 构建和运行

在容器内、仓库根目录：

```bash
make all

file kernel-rv kernel-la
# 期望：ELF 64-bit ... RISC-V / LoongArch

make run-rv
make run-la
```

说明：

- `make all` 完全离线使用 `vendor/`，不会临时下载 Rust crate。
- 第一次会生成 `.cargo/config.toml` 并恢复 vendor 的 checksum；不要手动删 `vendor/` 或 `.cargo`。
- `make run-rv` / `make run-la` 需要仓库根目录有测试镜像，见第 6 节。
- QEMU 的 `-nographic` 界面退出：`Ctrl-A` 然后 `X`；评测 init 跑完会主动关机。

---

## 5. Windows + WSL2 注意事项

1. 安装 Docker Desktop，开启 **WSL Integration**；或直接在 WSL 里装 Docker Engine。
2. 仓库放在 WSL 的 `~` 下，例如：

   ```text
   /home/<user>/starry-work/oskernel2026-oops
   ```

   **不要放 `/mnt/c/...`**：编译速度会慢很多，还可能遇到权限/换行符问题。

3. 在 WSL 里设置：

   ```bash
   git config --global core.autocrlf false
   ```

   仓库的 `.gitattributes` 已经对 `vendor/**` 做了 `-text`，但源码仍建议保持 LF。
   **不要对 vendor 执行 `dos2unix`**，否则 cargo 的 checksum 会失效。

4. VS Code 远程连接 WSL 后，再在 WSL 终端里执行 Docker 命令，避免路径混用。

5. Windows 防火墙/代理可能影响 `docker pull`；如果拉取慢，给 Docker 配镜像加速。

---

## 6. 测试镜像

`sdcard-rv.img` 和 `sdcard-la.img` 各约 4GB，**不在 git 仓库里**（被 `.gitignore` 的 `*.img` 忽略），需要从队伍共享盘/官方渠道获取，放到仓库根目录：

```text
oskernel2026-oops/
  sdcard-rv.img
  sdcard-la.img
```

检查：

```bash
ls -lh sdcard-*.img
```

没有镜像时：

- `make all` 仍可正常编译；
- `make run-rv` / `make run-la` 无法启动。

**不要提交这两个镜像**。

> 官方指南里 LA 的 QEMU 参数有笔误：`-device virtio-blk-pci,...,bus=virtio-mmio-bus.0` 在 QEMU 9/10 上会报 `Bus 'virtio-mmio-bus.0' not found`。仓库的 `make run-la` 已经使用正确写法：`-device virtio-blk-pci,drive=x0`。

---

## 7. VS Code Dev Container（可选）

如果更习惯 VS Code：

1. 安装 **Dev Containers** 扩展；
2. 打开仓库目录；
3. `Dev Containers: Reopen in Container`。

现有 `.devcontainer/devcontainer.json` 使用基础镜像，**不带** `nightly-2025-05-20`。两种处理：

- 在 `.devcontainer/devcontainer.json` 里加 volume：

  ```json
  "mounts": [
    "source=starry-rustup,target=/root/.rustup,type=volume"
  ]
  ```

  前提是已经按方案 A 创建并安装好 volume。

- 或者使用方案 B 的 `starry-dev:latest`，把 `"image"` 改成该镜像。

如果不确定，建议先用第 3 节的终端 `docker run` 流程，稳定后再接 VS Code。

---

## 8. 常见问题

| 现象 | 原因/解决 |
|---|---|
| `toolchain 'nightly-2025-05-20' is not installed` | 没做第 2 节，或启动时没挂载 `starry-rustup:/root/.rustup` |
| `riscv64-linux-musl-cc: not found` | 不在官方镜像里，或 PATH 丢了；用本文镜像，确认 `which` 能找到 |
| `loongarch64-linux-musl-cc: not found` | 同上 |
| `error: failed to load checksum ...` | 对 vendor 做了换行转换/手工改文件；重新 `git checkout -- vendor`，以后不要 `dos2unix` vendor |
| `.git/objects` 权限错误 | 在 root 容器里跑过 git；宿主机 `sudo chown -R "$USER:$USER" .git`，以后在宿主机执行 git |
| `No block device found` | 没放 `sdcard-*.img`，或镜像为空/损坏 |
| `Bus 'virtio-mmio-bus.0' not found` | LA 用了官方指南里的错误 bus 参数；用 `make run-la` 或去掉 `bus=` |
| QEMU 很慢 | WSL/无 KVM 时 rv/la 走 TCG，正常；不要在 `/mnt/c` 下编译 |
| `target/` 删不掉 | 容器 root 写入；宿主机 `sudo chown -R "$USER:$USER" target` |
| Docker 磁盘占用大 | 镜像约 27GB + target + 8GB 测试镜像；定期 `docker system df` |

---

## 9. 团队约定

1. 每天开始前 `git pull`，构建前先 `make all`。
2. **不要提交**：
   - `target/`
   - `kernel-rv` / `kernel-la`
   - `*.img`
   - `.cargo/`
3. 不要在 root 容器里执行 `git add/commit`；git 操作在宿主机做。
4. 不要随意改 `vendor/`；要打补丁必须同步更新 `cargo-checksum.json` 并在 PR 说明。
5. 不要随意换工具链；仓库固定 `nightly-2025-05-20`，换之前先确认评测镜像。
6. 构建/运行有问题，先在群里贴：
   - `docker images`
   - `rustup toolchain list`
   - `make all` 的完整报错
   - `ls -lh sdcard-*.img`

---

## 10. 首次配置检查清单

```text
[ ] docker pull zhouzhouyi/os-contest:20260510
[ ] docker volume create starry-rustup
[ ] 在 volume 里安装 nightly-2025-05-20 + rv/la targets
[ ] 在 WSL/Linux 原生目录 clone 仓库
[ ] git config --global core.autocrlf false
[ ] 放入 sdcard-rv.img / sdcard-la.img
[ ] docker run 挂载仓库 + volume
[ ] 容器内四件套自检：rustc、rv qemu、la qemu、rv/la musl cc
[ ] make all 成功，kernel-rv/kernel-la 是 ELF
[ ] make run-rv 能启动
[ ] make run-la 能启动
```

全部通过后，就可以按仓库里的开发/评测流程开始改代码了。
