use futures_channel::oneshot;
use js_sys::{Object, Reflect};
use std::{
    cell::{Cell, RefCell},
    collections::HashMap,
    rc::Rc,
};
use wasm_bindgen::{prelude::*, JsCast};
use web_sys::{MessageEvent, Worker, WorkerOptions, WorkerType};

pub fn error(v: JsValue) -> String {
    v.as_string()
        .or_else(|| {
            Reflect::get(&v, &"message".into())
                .ok()
                .and_then(|x| x.as_string())
        })
        .unwrap_or_else(|| format!("{v:?}"))
}
pub fn get(v: &JsValue, k: &str) -> JsValue {
    Reflect::get(v, &k.into()).unwrap_or(JsValue::UNDEFINED)
}
pub fn set(v: &JsValue, k: &str, value: &JsValue) {
    let _ = Reflect::set(v, &k.into(), value);
}
pub struct Bridge {
    worker: Worker,
    next: Cell<u32>,
    pending: Rc<RefCell<HashMap<u32, oneshot::Sender<Result<JsValue, String>>>>>,
    _message: Closure<dyn FnMut(MessageEvent)>,
    _error: Closure<dyn FnMut(web_sys::ErrorEvent)>,
}
impl Bridge {
    pub fn new() -> Result<Rc<Self>, String> {
        let options = WorkerOptions::new();
        options.set_type(WorkerType::Module);
        let worker = Worker::new_with_options("/processor-worker.js", &options).map_err(error)?;
        let pending: Rc<RefCell<HashMap<u32, oneshot::Sender<Result<JsValue, String>>>>> =
            Rc::default();
        let p = pending.clone();
        let message = Closure::new(move |event: MessageEvent| {
            let v = event.data();
            let id = get(&v, "requestId").as_f64().unwrap_or(0.0) as u32;
            if let Some(tx) = p.borrow_mut().remove(&id) {
                let result = if get(&v, "ok").as_bool() == Some(true) {
                    Ok(get(&v, "data"))
                } else {
                    let message = get(&v, "error")
                        .as_string()
                        .unwrap_or_else(|| "Web Worker failed".into());
                    let code = get(&v, "code").as_string().unwrap_or_default();
                    if code == "QuotaExceededError"
                        || message.contains("No space")
                        || message.contains("quota")
                    {
                        Err(
                            "พื้นที่รับไฟล์ไม่พอ หรือ browser จำกัดพื้นที่ระหว่างรับ กรุณาเพิ่มพื้นที่ว่างหรือเลือกที่บันทึกอื่น"
                                .into(),
                        )
                    } else {
                        Err(message)
                    }
                };
                let _ = tx.send(result);
            }
        });
        worker.set_onmessage(Some(message.as_ref().unchecked_ref()));
        let p = pending.clone();
        let onerror = Closure::new(move |event: web_sys::ErrorEvent| {
            for (_, tx) in p.borrow_mut().drain() {
                let _ = tx.send(Err(format!("Web Worker: {}", event.message())));
            }
        });
        worker.set_onerror(Some(onerror.as_ref().unchecked_ref()));
        Ok(Rc::new(Self {
            worker,
            next: Cell::new(1),
            pending,
            _message: message,
            _error: onerror,
        }))
    }
    pub async fn call(
        &self,
        command: &str,
        fields: &[(&str, JsValue)],
        transfer: Option<&JsValue>,
    ) -> Result<JsValue, String> {
        let id = self.next.get();
        self.next.set(id.wrapping_add(1));
        let msg: JsValue = Object::new().into();
        set(&msg, "requestId", &id.into());
        set(&msg, "command", &command.into());
        for (k, v) in fields {
            set(&msg, k, v);
        }
        let (tx, rx) = oneshot::channel();
        self.pending.borrow_mut().insert(id, tx);
        let result = if let Some(buffer) = transfer {
            let a = js_sys::Array::new();
            a.push(buffer);
            self.worker.post_message_with_transfer(&msg, &a)
        } else {
            self.worker.post_message(&msg)
        };
        if let Err(e) = result {
            self.pending.borrow_mut().remove(&id);
            return Err(error(e));
        }
        rx.await
            .map_err(|_| "Web Worker disconnected".to_string())?
    }
}
