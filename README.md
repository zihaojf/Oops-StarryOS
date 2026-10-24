# Starry OS

*An experimental monolithic OS based on ArceOS*

[![GitHub Stars](https://img.shields.io/github/stars/Starry-OS/StarryOS?style=for-the-badge)](https://github.com/Starry-OS/StarryOS/stargazers)
[![GitHub Forks](https://img.shields.io/github/forks/Starry-OS/StarryOS?style=for-the-badge)](https://github.com/Starry-OS/StarryOS/network)
[![GitHub License](https://img.shields.io/github/license/Starry-OS/StarryOS?style=for-the-badge)](https://github.com/Starry-OS/StarryOS/blob/main/LICENSE)
[![Build status](https://img.shields.io/github/check-runs/Starry-OS/StarryOS/main?style=for-the-badge)](https://github.com/Starry-OS/StarryOS/actions)

## Supported Architectures

- [x] RISC-V 64
- [x] LoongArch64
- [x] AArch64
- [ ] x86_64 (work in progress)

## Features

TODO

## Quick Start

### 1. Clone repo

```bash
git clone --recursive https://github.com/Starry-OS/StarryOS.git
cd StarryOS
```

Or if you have already cloned it without `--recursive` option:

```bash
cd StarryOS
git submodule update --init --recursive
```

### 2. Install Prerequisites

#### A. Using Docker

We provide a prebuilt Docker image with all dependencies installed.

For users in mainland China, you can use the following image which includes optimizations like Debian packages mirrors and crates.io mirrors:

```bash
docker pull docker.cnb.cool/starry-os/arceos-build
docker run -it --rm -v $(pwd):/workspace -w /workspace docker.cnb.cool/starry-os/arceos-build
```

For other users, you can use the image hosted on GitHub Container Registry:

```bash
docker pull ghcr.io/arceos-org/arceos-build
docker run -it --rm -v $(pwd):/workspace -w /workspace ghcr.io/arceos-org/arceos-build
```

**Note:** The `--rm` flag will destroy the container instance upon exit. Any changes made inside the container (outside of the mounted `/workspace` volume) will be lost. Please refer to the [Docker documentation](https://docs.docker.com/) for more advanced usage.

#### B. Manual Setup

##### i. Install System Dependencies

This step may vary depending on your operating system. Here is an example based on Debian:

```bash
sudo apt update
sudo apt install -y build-essential cmake clang qemu-system
```

**Note:** Running on LoongArch64 requires QEMU 10. If the QEMU version in your Linux distribution is too old (e.g. Ubuntu), consider building QEMU from [source](https://www.qemu.org/download/).

##### ii. Install Musl Toolchain

1. Download files from [setup-musl releases](https://github.com/arceos-org/setup-musl/releases/tag/prebuilt)
2. Extract to some path, for example `/opt/riscv64-linux-musl-cross`
3. Add bin folder to `PATH`, for example:

   ```bash
   export PATH=/opt/riscv64-linux-musl-cross/bin:$PATH
   ```

##### iii. Setup Rust toolchain

```bash
# Install rustup from https://rustup.rs or using your system package manager

# Automatically download components via rustup
cd StarryOS
cargo -V
```

### 3. Build the competition kernels

The top-level `Makefile` is set up for the OS kernel competition.  It uses the
vendored crates in `vendor/` and builds completely offline:

```bash
make all        # build ./kernel-rv and ./kernel-la (ELF)
make rv         # build only ./kernel-rv
make la         # build only ./kernel-la
```

`make all` performs the following steps:

1. recreates `.cargo/config.toml` from `config.toml.temp` and restores the
   vendored crate checksums (the submission system drops hidden files);
2. builds the kernel for `riscv64gc-unknown-none-elf` with virtio-mmio support
   and for `loongarch64-unknown-none-softfloat` with virtio-pci support;
3. rewrites the load addresses/entry point of the linked ELFs with
   `scripts/mkphyself.rs` so that QEMU's `-kernel` loader can start them.

The Rust toolchain is pinned to `nightly-2025-05-20`, which is shipped in the
evaluation image together with the required `riscv64gc-unknown-none-elf` and
`loongarch64-unknown-none-softfloat` targets.  Use `make TOOLCHAIN=<toolchain>`
if the evaluation image provides a different one.  A few vendored crates are
lightly patched so that they build with this toolchain (the upstream versions
use unstable library features that were only stabilized later).

The `riscv64-linux-musl` and `loongarch64-linux-musl` cross compilers are
expected under `/opt` (as in the competition image) or somewhere in `PATH`.

### 4. Run with the competition test image

At boot the kernel scans the root directory, `/glibc` and `/musl` for
`*_testcode.sh` scripts.  It prepares a small runtime environment (a static
`/bin/sh`, the glibc/musl dynamic loaders and `LD_LIBRARY_PATH`), runs every test point
serially and then shuts the machine down.  `lmbench` and `ltp` are skipped
because they are too slow/unsupported for now.  On LoongArch64 `iozone` and
`unixbench` are skipped as well (the timer implementation is incomplete and
`iozone` hangs), while the network test points are still run.

For local testing with the official disk images:

```bash
make run-rv     # uses ./sdcard-rv.img
make run-la     # uses ./sdcard-la.img
```

`disk.img` is not generated: the evaluation system mounts the test image itself.

To inspect which test points and test cases passed, capture the serial output and
summarize it:

```bash
make run-rv 2>&1 | tee rv.log
python3 scripts/summarize-tests.py rv.log
```

## What next?

You can check out the [GUI guide](./docs/x11.md) to set up a graphical environment, or explore other documentation in this folder.

If you're interested in contributing to the project, please see our [Contributing Guide](./CONTRIBUTING.md).

See more build options in the [Makefile](./Makefile).

## License

This project is now released under the Apache License 2.0. All modifications and new contributions in our project are distributed under the same license. See the [LICENSE](./LICENSE) and [NOTICE](./NOTICE) files for details.
