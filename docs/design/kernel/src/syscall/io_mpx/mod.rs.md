# `kernel/src/syscall/io_mpx/mod.rs`

> 对应源码：`kernel/src/syscall/io_mpx/mod.rs`
> 所属子系统：I/O 多路复用（模块门面）
> 依赖：`axpoll::{IoEvents, Pollable}`、`file::FileLike`

## 1. 文件职责

1. 声明并重导出 `epoll/poll/select`；
2. 定义 `FdPollSet`——把"一组 (fd, 关注事件)"组合成一个 `Pollable`，供 `poll_io` 等待。

```rust
struct FdPollSet(pub Vec<(Arc<dyn FileLike>, IoEvents)>);
impl Pollable for FdPollSet {
    fn poll(&self) -> IoEvents { unreachable!() }          // 由调用方手工检查每个 fd
    fn register(&self, context, _events) {
        for (file, events) in &self.0 { file.register(context, *events); }
    }
}
```

## 2. 设计要点

- `poll` 是 `unreachable!()`：`poll_io(&fds, IoEvents::empty(), ...)` 传入空的 interested 事件，`poll_io` 不会调用 `poll`（只调 `register`），实际就绪检查在闭包里逐 fd 完成。
- `register` 把同一个 waker 分发给所有被关注的 fd；任一 fd 就绪都会唤醒等待任务，然后闭包重新检查全部 fd。
- `FdPollSet` 由 `poll.rs` 与 `select.rs` 共用。

## 3. 不变量与约束

- **不能在 `poll` 被调用时 panic**：新增使用者必须保证 `poll_io` 的第一个参数是 `IoEvents::empty()`（`poll_io` 会短路，不调 `poll`）。
- `FdPollSet` 内的 `Arc<dyn FileLike>` 必须与调用方持有的 fd 一致（fd 可能在等待期间被关闭 → `register` 仍会注册到旧对象，唤醒逻辑由 `PollSet` 的 `Arc` 保证不 UAF）。
- `register` 对同一个 fd 可能注册多次（不同事件位），由各 `FileLike` 的 `register` 处理幂等。
- 本文件不持有 fd 表锁；fd 有效性在构造 `FdPollSet` 前已校验。

## 4. 与其他模块的关系

| 模块 | 关系 |
|---|---|
| `syscall/io_mpx/poll.rs` | `do_poll` 用 `FdPollSet` |
| `syscall/io_mpx/select.rs` | `do_select` 用 `FdPollSet` |
| `syscall/io_mpx/epoll.rs` | 不用 `FdPollSet`（用 `Epoll` 实例） |
| `file/mod.rs` | `FileLike` |
| `axpoll` | `Pollable`/`IoEvents` |

## 5. 已知局限 / TODO

- `poll` 的 `unreachable!()` 是脆弱契约。
- 每次等待都重新注册所有 fd（O(n)）。
- 无 `ppoll` 的 `sigmask` 之外的高级语义。

## 6. 变更影响面

改本文件 → `syscall/io_mpx/{poll,select}.rs.md`、`file/mod.rs.md`。
