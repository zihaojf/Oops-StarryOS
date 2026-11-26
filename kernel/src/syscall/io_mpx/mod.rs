mod epoll;
mod poll;
mod select;

use alloc::{sync::Arc, vec::Vec};
use core::task::Context;

use axpoll::{IoEvents, Pollable};

pub use self::{epoll::*, poll::*, select::*};
use crate::file::FileLike;

struct FdPollSet(pub Vec<(Arc<dyn FileLike>, IoEvents)>);
impl Pollable for FdPollSet {
    fn poll(&self) -> IoEvents {
        unreachable!()
    }

    fn register(&self, context: &mut Context<'_>, _events: IoEvents) {
        // HUP/ERR 必须始终能被唤醒：注册掩码 ≠ 关注位，否则“先睡后关闭”收不到唤醒。
        for (file, events) in &self.0 {
            file.register(context, *events | IoEvents::ALWAYS_POLL);
        }
    }
}
