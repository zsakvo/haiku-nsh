//! 原生回调的进程内出口，由 app 订阅后转发给引擎。

use std::sync::{
    mpsc::{self, Receiver, Sender},
    Mutex,
};

/// 保留原生回调的事件名和载荷。
pub struct NativeEvent {
    pub name: &'static str,
    pub data: serde_json::Value,
}

static SINK: Mutex<Option<Sender<NativeEvent>>> = Mutex::new(None);

/// 替换当前订阅；本任务暂不从 app 调用。
pub fn subscribe() -> Receiver<NativeEvent> {
    let (sender, receiver) = mpsc::channel();
    *SINK.lock().unwrap_or_else(|e| e.into_inner()) = Some(sender);
    receiver
}

pub(crate) fn emit(name: &'static str, data: serde_json::Value) -> Result<(), String> {
    let sink = SINK.lock().unwrap_or_else(|e| e.into_inner());
    let sender = sink
        .as_ref()
        .ok_or("native event receiver is not installed")?;
    sender
        .send(NativeEvent { name, data })
        .map_err(|e| e.to_string())
}
