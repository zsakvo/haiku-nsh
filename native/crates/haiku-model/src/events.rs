//! 领域事件的进程内出口；播放事件保留来源 generation。

use std::sync::mpsc::{SendError, Sender};

/// 事件及其可选的播放请求代次。
pub struct Event<T> {
    pub generation: Option<u64>,
    pub data: T,
}

/// 调用方提供接收端，关闭接收端即通知生产者停止转发。
pub struct EventSink<T> {
    sender: Sender<Event<T>>,
    generation: Option<u64>,
}

impl<T> Clone for EventSink<T> {
    fn clone(&self) -> Self {
        Self {
            sender: self.sender.clone(),
            generation: self.generation,
        }
    }
}

impl<T> EventSink<T> {
    /// 使用调用方创建的 Rust channel。
    pub fn new(sender: Sender<Event<T>>) -> Self {
        Self {
            sender,
            generation: None,
        }
    }

    /// 为当前批事件携带原引擎的代次。
    pub fn with_generation(&self, generation: u64) -> Self {
        Self {
            sender: self.sender.clone(),
            generation: Some(generation),
        }
    }

    /// 转交事件，保留接收端已关闭的错误。
    pub fn add(&self, data: T) -> Result<(), SendError<Event<T>>> {
        self.sender.send(Event {
            generation: self.generation,
            data,
        })
    }
}
