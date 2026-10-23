use js_sys::{Array, Function, Object, Promise, Reflect, Uint8Array};
use std::{cell::RefCell, collections::HashMap};
use toolbox_core::{safe_filename, IncrementalHash};
use wasm_bindgen::prelude::*;
use wasm_bindgen::JsCast;
use wasm_bindgen_futures::JsFuture;

struct Sink {
    handle: JsValue,
    writer: JsValue,
    sync: bool,
    opfs: bool,
    offset: u64,
    size: u64,
    hash: IncrementalHash,
}
thread_local! {
    static SOURCES: RefCell<HashMap<String,web_sys::File>> = RefCell::default();
    static HASHES: RefCell<HashMap<String,IncrementalHash>> = RefCell::default();
    static SINKS: RefCell<HashMap<String,Sink>> = RefCell::default();
    static FINISHED: RefCell<HashMap<String,JsValue>> = RefCell::default();
}
fn err(s: &str) -> JsValue {
    js_sys::Error::new(s).into()
}
fn field(v: &JsValue, k: &str) -> Result<JsValue, JsValue> {
    Reflect::get(v, &k.into())
}
fn text(v: &JsValue, k: &str) -> Result<String, JsValue> {
    field(v, k)?
        .as_string()
        .ok_or_else(|| err("missing text field"))
}
fn num(v: &JsValue, k: &str) -> Result<u64, JsValue> {
    text(v, k)?
        .parse()
        .map_err(|_| err("invalid 64-bit offset"))
}
fn obj() -> JsValue {
    Object::new().into()
}
fn set(o: &JsValue, k: &str, v: &JsValue) -> Result<(), JsValue> {
    Reflect::set(o, &k.into(), v).map(|_| ())
}
fn call(o: &JsValue, k: &str, args: &[JsValue]) -> Result<JsValue, JsValue> {
    let f: Function = field(o, k)?.dyn_into()?;
    let a = Array::new();
    for arg in args {
        a.push(arg);
    }
    f.apply(o, &a)
}
async fn wait(v: JsValue) -> Result<JsValue, JsValue> {
    JsFuture::from(Promise::resolve(&v)).await
}
async fn root() -> Result<JsValue, JsValue> {
    let nav = field(&js_sys::global(), "navigator")?;
    let storage = field(&nav, "storage")?;
    let root = wait(call(&storage, "getDirectory", &[])?)
        .await
        .map_err(|_| err("เปิดพื้นที่รับไฟล์ไม่ได้ ลองใช้แท็บปกติแทนโหมดส่วนตัว หรือเปิดด้วย browser อื่น"))?;
    let opts = obj();
    set(&opts, "create", &true.into())?;
    wait(call(
        &root,
        "getDirectoryHandle",
        &["baan-toolbox-transfers".into(), opts],
    )?)
    .await
}
async fn remove_temp(id: &str) -> Result<(), JsValue> {
    let r = root().await?;
    wait(call(&r, "removeEntry", &[id.into()])?)
        .await
        .map(|_| ())
}
#[wasm_bindgen]
pub async fn dispatch(v: JsValue) -> Result<JsValue, JsValue> {
    let command = text(&v, "command")?;
    let out = obj();
    match command.as_str() {
        "status" => {}
        "register" => {
            let id = text(&v, "id")?;
            let file = field(&v, "file")?.dyn_into::<web_sys::File>()?;
            SOURCES.with_borrow_mut(|m| m.insert(id, file));
        }
        "unregister" => {
            let id = text(&v, "id")?;
            SOURCES.with_borrow_mut(|m| m.remove(&id));
        }
        "read" => {
            let id = text(&v, "id")?;
            let job = text(&v, "job")?;
            let offset = num(&v, "offset")?;
            let length = field(&v, "length")?
                .as_f64()
                .ok_or_else(|| err("invalid length"))? as u64;
            if length > toolbox_core_payload_limit() {
                return Err(err("read exceeds bounded buffer"));
            }
            let file = SOURCES
                .with_borrow(|m| m.get(&id).cloned())
                .ok_or_else(|| err("source file unavailable"))?;
            let end = offset
                .checked_add(length)
                .ok_or_else(|| err("offset overflow"))?
                .min(file.size() as u64);
            if offset > end || end > 9_007_199_254_740_991 {
                return Err(err("file offset unsupported by browser"));
            }
            let slice = file.slice_with_f64_and_f64(offset as f64, end as f64)?;
            let buf = JsFuture::from(slice.array_buffer()).await?;
            let bytes = Uint8Array::new(&buf).to_vec();
            HASHES.with_borrow_mut(|m| m.entry(job).or_default().update(&bytes));
            set(&out, "bytes", &buf)?;
        }
        "digest" => {
            let job = text(&v, "job")?;
            let hash = HASHES
                .with_borrow_mut(|m| m.remove(&job))
                .unwrap_or_default();
            set(&out, "digest", &hash.digest().into())?;
        }
        "prepare" => {
            let id = text(&v, "job")?;
            let size = num(&v, "size")?;
            let name = safe_filename(&text(&v, "name")?);
            if SINKS.with_borrow(|m| m.contains_key(&id))
                || FINISHED.with_borrow(|m| m.contains_key(&id))
            {
                return Err(err("duplicate receive job"));
            }
            let mut handle = field(&v, "handle")?;
            let opfs = handle.is_undefined() || handle.is_null();
            if opfs {
                let storage = field(&field(&js_sys::global(), "navigator")?, "storage")?;
                let estimate = wait(call(&storage, "estimate", &[])?).await?;
                let quota = field(&estimate, "quota")?.as_f64().unwrap_or(0.0);
                let usage = field(&estimate, "usage")?.as_f64().unwrap_or(0.0);
                if quota > 0.0 && size as f64 > quota - usage {
                    return Err(err("พื้นที่เก็บข้อมูลของ browser ไม่พอสำหรับไฟล์นี้"));
                }
                let options = obj();
                set(&options, "create", &true.into())?;
                handle = wait(call(
                    &root().await?,
                    "getFileHandle",
                    &[id.clone().into(), options],
                )?)
                .await?;
            } else if field(&handle, "kind")?.as_string().as_deref() == Some("directory") {
                let options = obj();
                set(&options, "create", &true.into())?;
                // Prefix prevents silently replacing an existing file with the same name.
                handle = wait(call(
                    &handle,
                    "getFileHandle",
                    &[format!("{}-{}", &id[..8], name).into(), options],
                )?)
                .await?;
            }
            let sync = opfs;
            let writer = if sync {
                wait(call(&handle, "createSyncAccessHandle", &[])?).await?
            } else {
                wait(call(&handle, "createWritable", &[])?).await?
            };
            if sync {
                call(&writer, "truncate", &[0.into()])?;
            }
            SINKS.with_borrow_mut(|m| {
                m.insert(
                    id,
                    Sink {
                        handle,
                        writer,
                        sync,
                        opfs,
                        offset: 0,
                        size,
                        hash: IncrementalHash::default(),
                    },
                )
            });
        }
        "write" => {
            let id = text(&v, "job")?;
            let offset = num(&v, "offset")?;
            let bytes = Uint8Array::new(&field(&v, "bytes")?).to_vec();
            let (writer, sync) = SINKS
                .with_borrow(|m| m.get(&id).map(|s| (s.writer.clone(), s.sync)))
                .ok_or_else(|| err("receive job unavailable"))?;
            SINKS.with_borrow(|m| {
                let s = &m[&id];
                if offset != s.offset
                    || offset
                        .checked_add(bytes.len() as u64)
                        .is_none_or(|n| n > s.size)
                {
                    Err(err("invalid chunk offset or size"))
                } else {
                    Ok(())
                }
            })?;
            let array = Uint8Array::from(bytes.as_slice());
            if sync {
                let options = obj();
                set(&options, "at", &(offset as f64).into())?;
                let written = call(&writer, "write", &[array.into(), options])?
                    .as_f64()
                    .unwrap_or(0.0) as usize;
                if written != bytes.len() {
                    return Err(err("เขียนไฟล์ได้ไม่ครบ พื้นที่อาจไม่พอ"));
                }
            } else {
                wait(call(&writer, "write", &[array.into()])?).await?;
            }
            let next = offset + bytes.len() as u64;
            SINKS.with_borrow_mut(|m| {
                let s = m.get_mut(&id).unwrap();
                s.offset = next;
                s.hash.update(&bytes);
            });
            set(&out, "offset", &next.to_string().into())?;
        }
        "finish" => {
            let id = text(&v, "job")?;
            let expected = text(&v, "digest")?;
            let sink = SINKS
                .with_borrow_mut(|m| m.remove(&id))
                .ok_or_else(|| err("receive job unavailable"))?;
            if sink.offset != sink.size || sink.hash.digest() != expected {
                if sink.sync {
                    let _ = call(&sink.writer, "close", &[]);
                } else {
                    let _ = wait(call(&sink.writer, "abort", &[])?).await;
                }
                if sink.opfs {
                    let _ = remove_temp(&id).await;
                }
                return Err(err("ตรวจสอบ SHA-256 ไม่ผ่าน กรุณาส่งไฟล์ใหม่"));
            }
            if sink.sync {
                call(&sink.writer, "flush", &[])?;
                call(&sink.writer, "close", &[])?;
            } else {
                wait(call(&sink.writer, "close", &[])?).await?;
            }
            if sink.opfs {
                let file = wait(call(&sink.handle, "getFile", &[])?).await?;
                set(&out, "file", &file)?;
                FINISHED.with_borrow_mut(|m| m.insert(id, sink.handle));
            }
            set(&out, "savedDirectly", &(!sink.opfs).into())?;
        }
        "abort" => {
            let id = text(&v, "job")?;
            HASHES.with_borrow_mut(|m| m.remove(&id));
            if let Some(s) = SINKS.with_borrow_mut(|m| m.remove(&id)) {
                if s.sync {
                    let _ = call(&s.writer, "close", &[]);
                } else if let Ok(p) = call(&s.writer, "abort", &[]) {
                    let _ = wait(p).await;
                }
                if s.opfs {
                    let _ = remove_temp(&id).await;
                }
            }
        }
        "delete" => {
            let id = text(&v, "job")?;
            if FINISHED.with_borrow_mut(|m| m.remove(&id)).is_some() {
                remove_temp(&id).await?;
            }
        }
        "cleanup" => {
            let r = root().await?;
            let iterator = call(&r, "keys", &[])?;
            loop {
                let n = wait(call(&iterator, "next", &[])?).await?;
                if field(&n, "done")?.as_bool() == Some(true) {
                    break;
                }
                if let Some(id) = field(&n, "value")?.as_string() {
                    if id.len() == 32 && id.bytes().all(|b| b.is_ascii_hexdigit()) {
                        let _ = remove_temp(&id).await;
                    }
                }
            }
        }
        _ => return Err(err("unknown worker command")),
    }
    Ok(out)
}
fn toolbox_core_payload_limit() -> u64 {
    16 * 1024 - 24
}
