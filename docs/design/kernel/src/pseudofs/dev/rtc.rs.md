# `kernel/src/pseudofs/dev/rtc.rs`

> 对应源码：`kernel/src/pseudofs/dev/rtc.rs`
> 所属子系统：RTC（`/dev/rtc0`）
> 依赖：`chrono`（`Datelike/Timelike`）、`axhal::time::wall_time_nanos`、`starry_vm::VmMutPtr`、`linux_raw_sys::ioctl::RTC_RD_TIME`
> 被依赖：`pseudofs/dev/mod.rs`

## 1. 文件职责

把平台 RTC（`axhal` 的 wall clock）暴露为字符设备，支持 `RTC_RD_TIME`。

## 2. 内容

```rust
pub const RTC0_DEVICE_ID: DeviceId = DeviceId::new(250, 0);
#[repr(C)] struct rtc_time { tm_sec, tm_min, tm_hour, tm_mday, tm_mon, tm_year, tm_wday, tm_yday, tm_isdst }
pub struct Rtc;
```

`ioctl`：

```rust
RTC_RD_TIME => {
    let wall = DateTime::from_timestamp_nanos(wall_time_nanos() as _);
    vm_write(rtc_time {
        tm_sec: wall.second(), tm_min: minute(), tm_hour: hour(),
        tm_mday: day(), tm_mon: month0(), tm_year: year() - 1900,
        tm_wday: 0, tm_yday: 0, tm_isdst: 0,
    })?;
}
_ => NotATty
```

`read_at`/`write_at` 都返回 `Ok(0)`（RTC 不用 read/write）。

## 3. 不变量与约束

- **`rtc_time` 字段与 Linux 一致**：`tm_year` 是 `year - 1900`、`tm_mon` 是 `0..11`（`month0()`）；写错会让 `hwclock`/`date` 混乱。
- `DeviceId 250:0` 是 Linux 的 `/dev/rtc0` 惯例。
- `wall_time_nanos` 来自 `axhal`（goldfish RTC）；精度到纳秒但 RTC 实际秒级。
- `RTC_RD_TIME` 的 `arg` 是用户指针，必须 `vm_write`（C6）。
- 只支持读时间；设置时间（`RTC_SET_TIME`）返回 `NotATty`。
- `flags = NON_CACHEABLE | STREAM`。

## 4. 与其他模块的关系

| 模块 | 关系 |
|---|---|
| `pseudofs/dev/mod.rs` | 注册 `rtc0`（`RTC0_DEVICE_ID`） |
| `pseudofs/device.rs` | `DeviceOps` |
| `axhal::time` | wall clock（goldfish RTC 驱动） |
| `syscall/time.rs` | `clock_gettime(CLOCK_REALTIME)` 也读 wall time（不经过本设备） |

## 5. 已知局限 / TODO

- 只实现 `RTC_RD_TIME`；无 `RTC_SET_TIME`/`RTC_ALM_*`/`RTC_IRQP_*`。
- `tm_wday/tm_yday` 恒 0。
- 无中断/闹钟支持。
- 时区/夏令时（`tm_isdst`）恒 0。

## 6. 变更影响面

改本文件 → `pseudofs/dev/mod.rs.md`、`pseudofs/device.rs.md`。
