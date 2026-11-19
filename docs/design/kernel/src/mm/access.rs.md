# `kernel/src/mm/access.rs`

> 对应源码：`kernel/src/mm/access.rs`
> 所属子系统：用户内存访问 / 缺页处理
> 依赖：`axhal::{asm::user_copy, paging::MappingFlags, trap::{PAGE_FAULT, register_trap_handler}}`、`starry_vm`、`bytemuck`、`kernel_guard`
> 被依赖：`mm/mod.rs`（重导出）、`syscall/**`、`file/**`、`pseudofs/**`

## 1. 文件职责

1. **受控的用户内存访问**：`UserPtr/UserConstPtr` 校验并生成引用；`VmBytes/VmBytesMut` 作为 `axio` 的 `Read/Write` 适配器；`VmIo` 实现 `starry_vm` 的读写后端；
2. **`access_user_memory`**：开启"正在访问用户内存"作用域，允许内核态缺页；
3. **`PAGE_FAULT` trap handler**：仅在该作用域内处理缺页；
4. `vm_load_string`、`check_access`、`nullable!` 宏。

## 2. 关键类型

```rust
pub fn access_user_memory<R>(f: impl FnOnce() -> R) -> R;   // 设置/清除 Thread.accessing_user_memory
fn check_region(start, layout, access_flags) -> AxResult<()>; // 校验权限 + populate
pub struct UserPtr<T>(*mut T);        // 可写用户指针
pub struct UserConstPtr<T>(*const T); // 只读用户指针
pub struct VmBytes { ptr: *const u8, len: usize }        // Read + IoBuf
pub struct VmBytesMut { ptr: *mut u8, len: usize }       // Write + IoBufMut
struct Vm(IrqSave);                   // VmIo 实现（关中断保护指针检查）
```

`UserPtr` API：`get_as_mut()/get_as_mut_slice(len)/get_as_mut_null_terminated()`；`UserConstPtr`：`get_as_ref()/get_as_slice(len)/get_as_null_terminated()/get_as_str()`。`nullable!` 宏把空指针转成 `Ok(None)`。

## 3. 关键流程

### 3.1 `check_region`

```text
1. 对齐检查：start & (align-1) != 0 → BadAddress
2. aspace.can_access_range(start, size, access_flags)     // 权限 + 覆盖
3. aspace.populate_area(页对齐范围, access_flags)          // 缺页预填充（惰性后端在此分配）
```

`populate_area` 失败 → `NoMemory`；`can_access_range` 失败 → `BadAddress`。

### 3.2 `check_null_terminated`

逐页推进：每跨一页就用 `can_access_range(page, 4K, flags)` 重新校验（**不能**持 aspace 锁循环，因为 `ptr.read_volatile()` 可能触发缺页，缺页路径还要拿同一把锁）。读到 `T::default()` 结束，返回长度。

### 3.3 `PAGE_FAULT` handler

```rust
register_trap_handler(PAGE_FAULT) fn handle_page_fault(vaddr, access_flags) -> bool {
    let curr = current();
    let Some(thr) = curr.try_as_thread() else { return false };
    if !thr.is_accessing_user_memory() { return false }      // 不是受控访问 → 交给上层
    thr.proc_data.aspace.lock().handle_page_fault(vaddr, access_flags)
}
```

### 3.4 `VmIo for Vm`

```text
read:  check_access(start, len) → access_user_memory(|| user_copy(dst, start, len)) → failed_at != 0 ? AccessDenied : Ok
write: 同上反向
```

`check_access` 只做**粗略**边界检查（是否落在 `USER_SPACE_BASE..BASE+SIZE`），真正权限由 `user_copy` 触发的缺页处理。

## 4. 不变量与约束

- **C6**：用户指针必须经本文件的 API；`UserPtr::get_as_mut` 返回 `&'static mut T` 是**契约性 unsafe**：调用方必须保证在返回引用期间不发生并发修改（单核 + 当前任务独占）。
- **C7**：所有会触发用户缺页的内核访问必须在 `access_user_memory` 内；否则 trap handler 返回 false → 内核 panic。
- `check_null_terminated` 的逐页校验不能提取到循环外（注释明确说明锁重入问题）。
- `Vm` 用 `IrqSave` 保护（`VmIo::new`），避免在关中断期间被抢占（用户内存访问是原子的）。
- `check_access` 是"粗略"的：通过它不代表内存已映射；不要把它当安全边界。
- `UserPtr::ACCESS_FLAGS = READ|WRITE`，`UserConstPtr = READ`；新增方法必须选择正确的 flags。
- `nullable!` 只用于"0/NULL 表示未提供"的参数；不要用它掩盖非法指针（NULL 与非 NULL 走不同校验）。

## 5. 与其他模块的关系

| 模块 | 关系 |
|---|---|
| `mm/aspace/mod.rs` | `can_access_range`/`populate_area`/`handle_page_fault` |
| `mm/io.rs` | `IoVectorBuf` 用 `vm_read_slice/vm_write_slice`（最终走 `VmIo`） |
| `file/mod.rs` | `IoDst/IoSrc` 由 `VmBytes/VmBytesMut` 实现 |
| `task/user.rs` | 用户态缺页走 `ReturnReason::PageFault`（另一条路径） |
| `task/mod.rs` | `Thread.accessing_user_memory` 标志 |
| `syscall/**` | 所有参数/结构体读写 |

## 6. 已知局限 / TODO

- `check_null_terminated` 效率低（每页重复校验，TODO 注释承认）。
- `get_as_mut` 返回 `'static` 引用，依赖单核/当前任务独占假设；SMP 下不安全。
- `check_access` 不做映射检查，只做范围检查。
- `user_copy` 的失败位置（failed_at）目前只转成 `AccessDenied`，没有细分 EFAULT 位置。
- `Vm` 的 `IrqSave` 只保护检查+复制，不保护后续对返回引用的使用。

## 7. 变更影响面

改本文件 → `mm/io.rs.md`、`mm/aspace/mod.rs.md`、`file/mod.rs.md`、`task/user.rs.md`、`task/mod.rs.md`、`CONSTRAINTS.md` C6/C7、`ARCHITECTURE.md#5.2`。
