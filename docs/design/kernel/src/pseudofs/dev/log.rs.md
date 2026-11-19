# `kernel/src/pseudofs/dev/log.rs`

> 对应源码：`kernel/src/pseudofs/dev/log.rs`
> 所属子系统：`/dev/log` syslog 服务器
> feature：`dev-log`（`#[cfg(feature = "dev-log")]`）
> 依赖：`axnet::unix::{DgramTransport, UnixSocket, UnixSocketAddr}`、`core::bstr::ByteStr`

## 1. 文件职责

启动一个 Unix datagram 服务器绑定在 `/dev/log`，接收用户态（glibc/musl 的 `syslog(3)`）发来的日志，并转发到内核日志。

```rust
pub fn bind_dev_log() -> LinuxResult<()> {
    let server = UnixSocket::new(DgramTransport::new(1));
    server.bind(SocketAddrEx::Unix(UnixSocketAddr::Path("/dev/log".into())))?;
    axtask::spawn_with_name(move || {
        let mut buf = [0u8; 65536];
        loop {
            match server.recv(&mut buf[..], RecvOptions::default()) {
                Ok(read) => { let msg = ByteStr::new(buf[..read].trim_ascii_end()); info!("{msg}") }
                Err(err) => { warn!("Failed to receive logs from client: {err:?}"); break }
            }
        }
    }, "dev-log-server".into());
    Ok(())
}
```

## 2. 设计要点

- 用 `axnet` 的 Unix datagram socket 直接实现，**不经过** `FileLike`/`FD_TABLE`——它是内核内部的"服务端"。
- `DgramTransport::new(1)`：`pid` 参数为 1（日志服务端固定）。
- 缓冲区 64KiB；每次收到即 `info!`，去掉尾部空白（`trim_ascii_end`）。
- `bind_dev_log` 在 `pseudofs::mount_all` 的 `drop(fs)` 之后调用（需要 `/dev` 已挂载）。

## 3. 不变量与约束

- **`/dev/log` 必须是 socket 类型节点**：`pseudofs/dev/mod.rs` 在 `dev-log` 下注册 `SimpleFile::new(fs, NodeType::Socket, || Ok(b""))`；`bind` 时用路径地址，二者配合让 `syslog(3)` 能找到。
- 服务端任务用 `spawn_with_name`（不是 `spawn_raw`），失败不应阻止启动（但当前 `mount_all` 里是 `expect`）。
- `recv` 错误即退出循环（不重试）；如果客户端发送导致解析错误，服务端会停止——这是现状。
- 日志内容直接 `info!`，可能包含用户输入；不要改成 `info!("{msg:?}")`（会改变格式）。
- 依赖 `axnet::unix` 的 dgram 实现；`unix` 传输层的可靠性由 `axnet` 保证。

## 4. 与其他模块的关系

| 模块 | 关系 |
|---|---|
| `pseudofs/mod.rs` | `mount_all` 中 `#[cfg(feature = "dev-log")] bind_dev_log()` |
| `pseudofs/dev/mod.rs` | 注册 `/dev/log` socket 节点 |
| `axnet::unix` | Unix socket 实现 |
| `kernel/Cargo.toml` | `dev-log` feature |
| 顶层 `Cargo.toml` | `qemu` feature 启用 `starry-kernel/dev-log` |

## 5. 已知局限 / TODO

- 服务端启动失败只 panic（`expect`），没有降级。
- 收到错误即退出，不重启。
- 不解析 syslog 优先级/时间戳，原样转发。
- 无 `/dev/log` 的权限控制（任何进程可发）。

## 6. 变更影响面

改本文件 → `pseudofs/mod.rs.md`、`pseudofs/dev/mod.rs.md`、`kernel/Cargo.toml`（feature 文档在根 README）。
