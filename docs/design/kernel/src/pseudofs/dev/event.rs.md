# `kernel/src/pseudofs/dev/event.rs`

> 对应源码：`kernel/src/pseudofs/dev/event.rs`
> 所属子系统：evdev 输入设备（`/dev/input/*`）
> feature：`input`（`#[cfg(feature = "input")]`）
> 依赖：`axinput`/`axdriver::prelude::{AxInputDevice, Event, EventType}`、`bitmaps::Bitmap`、`zerocopy`
> 被依赖：`pseudofs/dev/mod.rs`（`input_devices`）

## 1. 文件职责

把 `axinput` 的输入设备包装成 Linux evdev 字符设备：

- 读取 `input_event` 结构（`{time, type, code, value}`）流；
- 实现 `EVIOCG*` 系列 ioctl（版本/ID/名称/物理位置/唯一 ID/能力位图/按键状态）；
- 维护按键按下状态位图（`EVIOCGKEY`）；
- `input_devices(fs)` 枚举所有输入设备：含 `BTN_MOUSE` 的命名为 `mice`，其余为 `eventN`。

## 2. 关键类型

```rust
struct Inner { device: AxInputDevice, read_ahead: Option<(Duration, Event)>, key_state: Bitmap<KEY_CNT> }
pub struct EventDev { inner: Mutex<Inner>, ev_bits: Bitmap<{EventType::COUNT}> }
#[repr(C)] struct KernelTimeval { tv_sec: __kernel_old_time_t, tv_usec: __kernel_suseconds_t }
#[repr(C)] struct InputEvent { time: KernelTimeval, event_type: u16, code: u16, value: i32 }
```

`KEY_CNT = EventType::Key.bits_count()`。

## 3. 关键流程

### 3.1 读取

`read_at(buf, _)`：

- `buf` 为空 → `Ok(0)`；`buf.len() < size_of::<InputEvent>()` → `InvalidInput`；
- 按 `chunks_exact_mut(size_of::<InputEvent>())` 逐事件填；
- `inner.has_event()`：若无 `read_ahead` 就 `device.read_event()`；`Again` 忽略；按键事件更新 `key_state`，记录 `(wall_time, event)`；
- 填 `InputEvent { time: wall_time, event_type, code, value }`；
- 一个都没读到 → `WouldBlock`。

### 3.2 ioctl

| cmd | 行为 |
|---|---|
| `EVIOCGVERSION` | 写 `0x10001` |
| `EVIOCGID` | 写 `device.device_id()` |
| `EVIOCGRAB` | 直接 `Ok(0)`（不真正独占） |
| 其他 | 解出 `nr/ty/size/dir` 四段（Linux ioctl 编码） |

可变长度命令（`ty == b'E'`）：

| nr | 行为 |
|---|---|
| `0x06` EVIOCGNAME | `return_str(device_name())` |
| `0x07` EVIOCGPHYS | `return_str(physical_location())` |
| `0x08` EVIOCGUNIQ | `return_str(unique_id())` |
| `0x09` EVIOCGPROP | `Ok(0)`（virtio 不提供） |
| `0x18` EVIOCGKEY | 拷贝 `key_state.as_bytes()` |
| `0x19/0x1a/0x1b` | 全 0 位图（LED/SND/SW） |
| 其他 `nr & !EventType::MAX == EventType::COUNT` | `get_event_bits`（能力位图） |
| `nr & !(ABS_CNT-1) == ABS_CNT` | `Ok(0)`（abs info 未实现） |

### 3.3 `input_devices`

```text
for (i, device) in axinput::take_inputs():
    断言 device 支持 Key 位图
    dev = Device::new(fs, CharacterDevice, DeviceId::new(13, i+1), Arc::new(EventDev::new(device)))
    若 keys[BTN_MOUSE/8] 位为 1 → inputs.add("mice", dev)
    否则 → inputs.add(format!("event{input_id}"), dev); input_id += 1
```

## 4. 不变量与约束

- **C28**：`inner` 用 `Mutex` 保护；`has_event` 会修改 `read_ahead/key_state`，必须持锁。
- `InputEvent` 布局必须与 Linux `struct input_event` 一致（timeval + 3 个 u16/i32），否则用户态解析错位。
- `flags = NON_CACHEABLE | STREAM`：输入流不能缓存。
- `Pollable::register` 直接 `wake_by_ref()`（忙轮询），因为 `axinput` 没有注册中断的接口；这是**已知低效**点。
- `ongkey()` 函数是给链接器/调试用的符号（`#[unsafe(no_mangle)] #[inline(never)]`），不要删。
- `get_event_bits` 的 `ty == 0` 返回 `ev_bits`；非 0 时转发给设备并返回 `min(len, bits.div_ceil(8))`。
- `ioctl` 的 `dir` 编码：1=写、2=读；写方向返回 `InvalidInput`（evdev 没有用户写命令）。

## 5. 与其他模块的关系

| 模块 | 关系 |
|---|---|
| `pseudofs/dev/mod.rs` | 注册 `input/` 目录 |
| `pseudofs/device.rs` | `DeviceOps` |
| `mm/access.rs` | `UserPtr` 读写用户缓冲区 |
| `axinput`/`axdriver` | 底层设备 |
| `pseudofs/dev/fb.rs` | 同为图形/输入子系统 |

## 6. 已知局限 / TODO

- `register` 忙轮询（CPU 占用）。
- `EVIOCGRAB` 不独占。
- `EVIOCGPROP` 返回 0（virtio 限制）。
- abs info（`EVIOCGABS`）未实现。
- 键位图大小 `0x300` 硬编码在 `input_devices`。
- 没有多指/触摸协议（`EV_ABS` 只透传）。

## 7. 变更影响面

改本文件 → `pseudofs/dev/mod.rs.md`、`pseudofs/device.rs.md`、`kernel/Cargo.toml`（feature）、`CONSTRAINTS.md` C28。
