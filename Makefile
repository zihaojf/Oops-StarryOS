# StarryOS build entry point for the OS kernel competition.
#
# The evaluation system executes `make all` in the project root and expects the
# ELF kernels `kernel-rv` and `kernel-la` to be produced.  Everything is built
# from vendored sources, so no network access is required.
#
# Useful targets:
#   make all      build both kernels (default)
#   make rv       build only kernel-rv
#   make la       build only kernel-la
#   make check    clippy for both archs (same env as the build)
#   make clean    remove build artifacts
#   make run-rv   boot kernel-rv with the riscv64 test image (for local testing)
#   make run-la   boot kernel-la with the loongarch64 test image (for local testing)

.DEFAULT_GOAL := all

# Rust nightly toolchain used by this repository.  Override with
# `make TOOLCHAIN=...` if the evaluation image provides a different one.
TOOLCHAIN ?= nightly-2025-05-20
export RUSTUP_TOOLCHAIN := $(TOOLCHAIN)

# Cross compilers used to build the lwext4 C sources.
export PATH := /opt/riscv64-linux-musl-cross/bin:/opt/loongarch64-linux-musl-cross/bin:$(PATH)

CARGO ?= cargo
TARGET_DIR ?= target

RV_TARGET := riscv64gc-unknown-none-elf
LA_TARGET := loongarch64-unknown-none-softfloat

RV_CONFIG := configs/riscv64-qemu-virt.toml
LA_CONFIG := configs/loongarch64-qemu-virt.toml

RV_PLATFORM := riscv64-qemu-virt
LA_PLATFORM := loongarch64-qemu-virt

RV_ELF := $(TARGET_DIR)/$(RV_TARGET)/release/starryos
LA_ELF := $(TARGET_DIR)/$(LA_TARGET)/release/starryos

MKPHYSELF := $(TARGET_DIR)/mkphyself
CARGO_CONFIG := .cargo/config.toml

COMMON_ENV := \
	AX_LOG=error \
	AX_IP=10.0.2.15 \
	AX_GW=10.0.2.2 \
	AX_MODE=release

.PHONY: all rv la prepare clean run-rv run-la kernel-rv kernel-la check check-rv check-la

all: rv la

kernel-rv: rv
kernel-la: la

# The submission process drops hidden files/directories, so `.cargo/config.toml`
# and the `.cargo-checksum.json` files of the vendored crates are recreated here
# from their visible copies.
prepare:
	@mkdir -p .cargo
	@cp -f config.toml.temp .cargo/config.toml
	@find vendor -type f -name cargo-checksum.json -exec sh -c \
		 'for f; do cp -f "$$f" "$$(dirname "$$f")/.cargo-checksum.json"; done' sh {} +

# Convert the high-half linked kernel image into an ELF that QEMU can load at
# its physical address.
$(MKPHYSELF): scripts/mkphyself.rs
	@mkdir -p $(TARGET_DIR)
	@rustc --edition 2021 -O $< -o $@

rv: prepare $(MKPHYSELF)
	@echo "==> Building kernel-rv ($(RV_TARGET))"
	@AX_CONFIG_PATH=$(CURDIR)/$(RV_CONFIG) \
	 AX_ARCH=riscv64 \
	 AX_PLATFORM=$(RV_PLATFORM) \
	 AX_TARGET=$(RV_TARGET) \
	 $(COMMON_ENV) \
	 RUSTFLAGS="-C link-arg=-T$(CURDIR)/$(TARGET_DIR)/$(RV_TARGET)/release/linker_$(RV_PLATFORM).lds -C link-arg=-no-pie -C link-arg=-znostart-stop-gc" \
	 $(CARGO) build --release --offline --target $(RV_TARGET) --target-dir $(TARGET_DIR) \
		--features "qemu,axfeat/bus-mmio"
	@$(MKPHYSELF) $(RV_ELF) kernel-rv 0xffff_ffc0_0000_0000 0x8020_0000
	@echo "==> kernel-rv is ready"

la: prepare $(MKPHYSELF)
	@echo "==> Building kernel-la ($(LA_TARGET))"
	@AX_CONFIG_PATH=$(CURDIR)/$(LA_CONFIG) \
	 AX_ARCH=loongarch64 \
	 AX_PLATFORM=$(LA_PLATFORM) \
	 AX_TARGET=$(LA_TARGET) \
	 $(COMMON_ENV) \
	 RUSTFLAGS="-C link-arg=-T$(CURDIR)/$(TARGET_DIR)/$(LA_TARGET)/release/linker_$(LA_PLATFORM).lds -C link-arg=-no-pie -C link-arg=-znostart-stop-gc" \
	 $(CARGO) build --release --offline --target $(LA_TARGET) --target-dir $(TARGET_DIR) \
		--features "qemu"
	@$(MKPHYSELF) $(LA_ELF) kernel-la 0xffff_8000_0000_0000 0x0020_0040
	@echo "==> kernel-la is ready"

# Lint with the same environment as the real build.  axconfig reads
# AX_CONFIG_PATH at compile time (vendored axconfig_macros falls back to a
# non-existent axconfig.toml), so a bare `cargo clippy` aborts -- even though
# `make rv` succeeds.
CLIPPY_ARGS ?= -- -D warnings -A unused

check-rv: prepare
	@AX_CONFIG_PATH=$(CURDIR)/$(RV_CONFIG) \
	 AX_ARCH=riscv64 \
	 AX_PLATFORM=$(RV_PLATFORM) \
	 AX_TARGET=$(RV_TARGET) \
	 $(COMMON_ENV) \
	 $(CARGO) clippy --offline --target $(RV_TARGET) --target-dir $(TARGET_DIR) \
		--features "qemu,axfeat/bus-mmio" $(CLIPPY_ARGS)

check-la: prepare
	@AX_CONFIG_PATH=$(CURDIR)/$(LA_CONFIG) \
	 AX_ARCH=loongarch64 \
	 AX_PLATFORM=$(LA_PLATFORM) \
	 AX_TARGET=$(LA_TARGET) \
	 $(COMMON_ENV) \
	 $(CARGO) clippy --offline --target $(LA_TARGET) --target-dir $(TARGET_DIR) \
		--features "qemu" $(CLIPPY_ARGS)

check: check-rv check-la

# Local testing helpers.  The test images are not part of the submission.
run-rv: kernel-rv
	@qemu-system-riscv64 -machine virt -kernel kernel-rv -m 1G -nographic -smp 1 \
		-bios default -drive file=sdcard-rv.img,if=none,format=raw,id=x0 \
		-device virtio-blk-device,drive=x0,bus=virtio-mmio-bus.0 -no-reboot \
		-device virtio-net-device,netdev=net -netdev user,id=net -rtc base=utc

run-la: kernel-la
	@qemu-system-loongarch64 -kernel kernel-la -m 1G -nographic -smp 1 \
		-drive file=sdcard-la.img,if=none,format=raw,id=x0 \
		-device virtio-blk-pci,drive=x0 -no-reboot \
		-device virtio-net-pci,netdev=net0 \
		-netdev user,id=net0,hostfwd=tcp::5555-:5555,hostfwd=udp::5555-:5555 \
		-rtc base=utc

clean:
	@rm -rf kernel-rv kernel-la $(TARGET_DIR) .cargo
