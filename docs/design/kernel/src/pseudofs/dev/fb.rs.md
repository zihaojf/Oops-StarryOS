# `kernel/src/pseudofs/dev/fb.rs`

> 对应源码：`kernel/src/pseudofs/dev/fb.rs`
> 所属子系统：framebuffer（`/dev/fb0`）
> 依赖：`axdisplay`、`axhal::mem::virt_to_phys`、`starry_vm::VmMutPtr`
> 被依赖：`pseudofs/dev/mod.rs`（`axdisplay::has_display()` 时注册）

## 1. 文件职责

把 `axdisplay` 的帧缓冲包装成 `/dev/fb0`：

- `read_at/write_at`：直接读写帧缓冲 VA；
- `ioctl`：`FBIOGET_VSCREENINFO`/`FBIOGET_FSCREENINFO` 等；
- `mmap`：返回 `DeviceMmap::Physical`（用户可直接映射显存）；
- 后台 60Hz 刷新任务。

## 2. 关键类型

```rust
#[repr(C)] struct FrameBufferBitfield { offset, length, msb_right }
#[repr(C)] struct VarScreenInfo { ... }    // fb_var_screeninfo 子集
#[repr(C)] struct FixScreenInfo { ... }    // fb_fix_screeninfo 子集
pub struct FrameBuffer { base: VirtAddr, size: usize }
```

## 3. 关键流程

### 3.1 `FrameBuffer::new()`

- `axtask::spawn_with_name(|| block_on(refresh_task()), "fb-refresh")`；
- `info = axdisplay::framebuffer_info()`；`base = VirtAddr(info.fb_base_vaddr)`、`size = info.fb_size`。

### 3.2 `refresh_task`

```text
loop { axdisplay::framebuffer_flush(); sleep(1/60s).await }
```

### 3.3 `read_at/write_at`

- `as_mut_slice()`：`from_raw_parts_mut(base, size)`（`#[allow(clippy::mut_from_ref)]`）；
- 读：`len = min(buf.len(), size - offset)`；
- 写：`offset >= size` → `StorageFull`；否则 `len = min(buf.len(), size - offset)`。

**注意**：写实现是 `slice[..len].copy_from_slice(&buf[..len])`——**忽略了 offset**（bug：总是写帧缓冲开头）。

### 3.4 ioctl

| cmd | 行为 |
|---|---|
| `FBIOGET_VSCREENINFO (0x4600)` | 填 `VarScreenInfo`（宽高、bpp、RGB 位域、pixclock 等） |
| `FBIOPUT_VSCREENINFO (0x4601)` | `Ok(0)`（不生效） |
| `FBIOGET_FSCREENINFO (0x4602)` | 填 `FixScreenInfo`（`smem_start = fb_base_vaddr`、line_length） |
| `FBIOGETCMAP (0x4604)` / `FBIOPUTCMAP (0x4605)` | `Ok(0)` |
| `FBIOPAN_DISPLAY (0x4606)` / `FBIOBLANK (0x4611)` | `InvalidInput` |
| 其他 | `NotATty` |

### 3.5 mmap

`DeviceMmap::Physical(PhysAddrRange::from_start_size(virt_to_phys(base), size))` → `syscall/mm/mmap.rs` 转成 `Backend::Linear`。

## 4. 不变量与约束

- **写偏移 bug（已知）**：`write_at` 未使用 `offset`；修它时注意 `as_mut_slice()` 返回 `&mut [u8]` 的别名安全（单核 + 设备锁不严格，当前靠"帧缓冲只有一个写者"假设）。
- `bpp` 计算：`line_length / width`（`line_length = fb_size / height`），假定像素格式为 32 位。
- `flags = NON_CACHEABLE`：显存不能走缓存。
- `fb0` 只在 `has_display()` 时存在；Xorg 探测依赖 `/sys/class/graphics/fb0/device/subsystem` 软链接（`pseudofs/mod.rs`）。
- `VarScreenInfo`/`FixScreenInfo` 是**子集**，字段顺序必须与 Linux 结构一致（用户态按固定偏移读取）。
- 刷新任务每 1/60s 调 `framebuffer_flush`；对无 flush 需求的平台是空操作。

## 5. 与其他模块的关系

| 模块 | 关系 |
|---|---|
| `pseudofs/dev/mod.rs` | 注册 `fb0` |
| `pseudofs/device.rs` | `DeviceMmap::Physical` |
| `syscall/mm/mmap.rs` | `DeviceMmap::Physical` → `Linear` |
| `pseudofs/mod.rs` | `/sys/.../subsystem` 软链接 |
| `axdisplay` | 帧缓冲信息与 flush |

## 6. 已知局限 / TODO

- `write_at` 忽略 offset（bug）。
- `FBIOPUT_VSCREENINFO`/`FBIOBLANK`/`FBIOPAN_DISPLAY` 未实现。
- 像素格式/位域部分硬编码（red=16,green=8,blue=0，32bpp）。
- 无双缓冲/垂直同步。
- `--` 无 input 与显示联动（鼠标坐标由 X 自行处理）。

## 7. 变更影响面

改本文件 → `pseudofs/dev/mod.rs.md`、`pseudofs/device.rs.md`、`syscall/mm/mmap.rs.md`、`pseudofs/mod.rs.md`、`docs/x11.md`（GUI 说明）。
