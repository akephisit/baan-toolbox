use crate::bridge::{error, get, set, Bridge};
use gloo_timers::future::TimeoutFuture;
use js_sys::{Array, ArrayBuffer, Function, Object, Uint8Array};
use std::{
    cell::{Cell, RefCell},
    collections::{HashMap, HashSet, VecDeque},
    rc::Rc,
};
use toolbox_core::{decode_frame, encode_frame, safe_filename};
use toolbox_protocol::*;
use wasm_bindgen::{prelude::*, JsCast};
use wasm_bindgen_futures::{spawn_local, JsFuture};
use web_sys::*;

type Result<T> = std::result::Result<T, String>;
#[derive(Clone)]
pub struct Source {
    pub meta: FileMeta,
    pub file: File,
}
#[derive(Clone, Default)]
pub struct IncomingView {
    pub batch: String,
    pub sender: String,
    pub files: Vec<FileMeta>,
}
#[derive(Clone, Default)]
pub struct TransferView {
    pub batch: String,
    pub peer: String,
    pub incoming: bool,
    pub total: u64,
    pub done: u64,
    pub status: String,
    pub finished: bool,
    pub failed: bool,
}
#[derive(Clone, Default)]
pub struct Download {
    pub job: String,
    pub name: String,
    pub size: u64,
    pub url: String,
    pub direct: bool,
}
#[derive(Clone, Default)]
pub struct Model {
    pub name: String,
    pub online: bool,
    pub connecting: bool,
    pub worker_ready: bool,
    pub peers: Vec<Peer>,
    pub selected: HashSet<String>,
    pub files: Vec<FileMeta>,
    pub room: String,
    pub room_link: String,
    pub qr: String,
    pub offers: Vec<IncomingView>,
    pub transfers: Vec<TransferView>,
    pub downloads: Vec<Download>,
    pub notice: String,
    pub busy: bool,
}
struct Link {
    pc: RtcPeerConnection,
    dc: RefCell<Option<RtcDataChannel>>,
    handlers: RefCell<Vec<Closure<dyn FnMut(JsValue)>>>,
}
impl Drop for Link {
    fn drop(&mut self) {
        self.pc.set_ondatachannel(None);
        self.pc.set_onconnectionstatechange(None);
        self.pc.close();
        if let Some(dc) = self.dc.borrow().as_ref() {
            dc.set_onmessage(None);
            dc.set_onclose(None);
            dc.close();
        }
    }
}
struct Socket {
    ws: WebSocket,
    _handlers: Vec<Closure<dyn FnMut(JsValue)>>,
}
impl Drop for Socket {
    fn drop(&mut self) {
        self.ws.set_onopen(None);
        self.ws.set_onmessage(None);
        self.ws.set_onclose(None);
        self.ws.set_onerror(None);
        let _ = self.ws.close();
    }
}
#[derive(Default)]
struct SendState {
    accepted: bool,
    error: Option<String>,
    ready: HashSet<String>,
    ack: HashMap<String, u64>,
    sent: HashMap<String, u64>,
    verified: HashSet<String>,
}
struct Outgoing {
    peer: String,
    files: Vec<Source>,
    state: RefCell<SendState>,
}
struct Incoming {
    peer: String,
    sender: String,
    files: Vec<FileMeta>,
    accepted: bool,
    handle: JsValue,
    active: HashMap<String, FileMeta>,
    completed: usize,
}
pub struct Engine {
    model: RefCell<Model>,
    notify: Box<dyn Fn(Model)>,
    last_emit: Cell<f64>,
    pub worker: Rc<Bridge>,
    sources: RefCell<Vec<Source>>,
    socket: RefCell<Option<Socket>>,
    links: RefCell<HashMap<String, Rc<Link>>>,
    outgoing: RefCell<HashMap<String, Rc<Outgoing>>>,
    incoming: RefCell<HashMap<String, Incoming>>,
    queue: RefCell<VecDeque<(String, Vec<Source>, String)>>,
    active: Cell<usize>,
    endpoint: RefCell<String>,
    device_id: String,
    own_peer: RefCell<String>,
}
thread_local! {static ENGINE:RefCell<Option<Rc<Engine>>>=const{RefCell::new(None)};}
pub fn engine() -> Rc<Engine> {
    ENGINE.with_borrow(|e| e.as_ref().unwrap().clone())
}
pub fn now() -> f64 {
    js_sys::Date::now()
}
pub fn id() -> String {
    let mut bytes = [0u8; 16];
    web_sys::window()
        .unwrap()
        .crypto()
        .unwrap()
        .get_random_values_with_u8_array(&mut bytes)
        .unwrap();
    toolbox_core::hex(&bytes)
}
pub fn format_size(n: u64) -> String {
    let units = ["B", "KB", "MB", "GB", "TB", "PB"];
    let mut x = n as f64;
    let mut u = 0;
    while x >= 1024.0 && u < units.len() - 1 {
        x /= 1024.0;
        u += 1;
    }
    if u == 0 {
        format!("{n} B")
    } else {
        format!("{x:.1} {}", units[u])
    }
}
async fn sleep(ms: u32) {
    TimeoutFuture::new(ms).await;
}
fn storage() -> Option<Storage> {
    window()?.local_storage().ok().flatten()
}
fn save(k: &str, v: &str) {
    if let Some(s) = storage() {
        let _ = s.set_item(k, v);
    }
}
fn load(k: &str) -> Option<String> {
    storage()?.get_item(k).ok().flatten()
}
fn invoke(target: &JsValue, name: &str, args: &[JsValue]) -> Result<JsValue> {
    let f: Function = get(target, name).dyn_into().map_err(error)?;
    let a = Array::new();
    for v in args {
        a.push(v);
    }
    f.apply(target, &a).map_err(error)
}
async fn resolve(v: JsValue) -> Result<JsValue> {
    JsFuture::from(js_sys::Promise::resolve(&v))
        .await
        .map_err(error)
}
impl Engine {
    pub fn init(notify: impl Fn(Model) + 'static) -> Result<Rc<Self>> {
        let worker = Bridge::new()?;
        let device_id = load("toolbox.device").unwrap_or_else(id);
        save("toolbox.device", &device_id);
        let name = load("toolbox.name").unwrap_or_else(|| format!("อุปกรณ์ {}", &device_id[..4]));
        save("toolbox.name", &name);
        let e = Rc::new(Self {
            model: RefCell::new(Model {
                name,
                ..Model::default()
            }),
            notify: Box::new(notify),
            last_emit: Cell::new(0.0),
            worker,
            sources: RefCell::default(),
            socket: RefCell::default(),
            links: RefCell::default(),
            outgoing: RefCell::default(),
            incoming: RefCell::default(),
            queue: RefCell::default(),
            active: Cell::new(0),
            endpoint: RefCell::default(),
            device_id,
            own_peer: RefCell::default(),
        });
        ENGINE.with_borrow_mut(|g| *g = Some(e.clone()));
        e.emit(true);
        let me = e.clone();
        spawn_local(async move {
            if let Err(s) = me.start().await {
                me.notice(&s);
            }
        });
        let weak = Rc::downgrade(&e);
        spawn_local(async move {
            loop {
                sleep(20_000).await;
                let Some(me) = weak.upgrade() else {
                    break;
                };
                me.signal(&ClientMessage::Ping);
            }
        });
        Ok(e)
    }
    fn emit(&self, force: bool) {
        if force || now() - self.last_emit.get() > 100.0 {
            self.last_emit.set(now());
            (self.notify)(self.model.borrow().clone());
        }
    }
    pub fn notice(&self, s: &str) {
        self.model.borrow_mut().notice = s.into();
        self.emit(true);
    }
    pub fn dismiss(&self) {
        self.model.borrow_mut().notice.clear();
        self.emit(true);
    }
    async fn start(self: &Rc<Self>) -> Result<()> {
        self.worker.call("status", &[], None).await?;
        self.model.borrow_mut().worker_ready = true;
        self.emit(true);
        let response = JsFuture::from(window().unwrap().fetch_with_str("/config.json"))
            .await
            .map_err(error)?
            .dyn_into::<Response>()
            .map_err(error)?;
        if !response.ok() {
            return Err("โหลดการตั้งค่าเซิร์ฟเวอร์ไม่สำเร็จ".into());
        }
        let config = JsFuture::from(response.json().map_err(error)?)
            .await
            .map_err(error)?;
        let endpoint = get(&config, "signalingUrl")
            .as_string()
            .filter(|s| !s.is_empty())
            .ok_or("ยังไม่ได้ตั้งค่า signalingUrl สำหรับ deployment นี้")?;
        *self.endpoint.borrow_mut() = endpoint;
        let params =
            UrlSearchParams::new_with_str(&window().unwrap().location().search().map_err(error)?)
                .map_err(error)?;
        self.connect(params.get("room").unwrap_or_default())?;
        Ok(())
    }
    pub fn rename(&self, name: String) {
        let name: String = name.chars().filter(|c| !c.is_control()).take(40).collect();
        self.model.borrow_mut().name = name.clone();
        save("toolbox.name", &name);
        self.signal(&ClientMessage::Rename { name });
        self.emit(true);
    }
    pub fn toggle(&self, peer: &str) {
        let mut m = self.model.borrow_mut();
        if !m.selected.remove(peer) {
            m.selected.insert(peer.into());
        }
        drop(m);
        self.emit(true);
    }
    pub fn select_all(&self) {
        let ids: HashSet<_> = self
            .model
            .borrow()
            .peers
            .iter()
            .map(|p| p.id.clone())
            .collect();
        let mut m = self.model.borrow_mut();
        if m.selected.len() == ids.len() {
            m.selected.clear();
        } else {
            m.selected = ids;
        }
        drop(m);
        self.emit(true);
    }
    pub fn pick_input(self: &Rc<Self>) {
        if let Some(input) = window()
            .and_then(|w| w.document())
            .and_then(|d| d.get_element_by_id("file-input"))
            .and_then(|e| e.dyn_into::<HtmlInputElement>().ok())
        {
            if let Some(files) = input.files() {
                self.add_files(files);
            }
            input.set_value("");
        }
    }
    pub fn add_files(&self, files: FileList) {
        if self.model.borrow().busy {
            return;
        }
        let mut sources = self.sources.borrow_mut();
        for i in 0..files.length() {
            if let Some(f) = files.get(i) {
                if f.size() > 9_007_199_254_740_991.0 {
                    self.notice("ไฟล์นี้มีขนาดเกินช่วงตัวเลขที่ browser รองรับ");
                    continue;
                }
                let meta = FileMeta {
                    id: id(),
                    name: f.name(),
                    size: f.size() as u64,
                    mime: f.type_(),
                };
                sources.push(Source { meta, file: f });
            }
        }
        self.model.borrow_mut().files = sources.iter().map(|s| s.meta.clone()).collect();
        drop(sources);
        self.emit(true);
    }
    pub fn remove_file(&self, file: &str) {
        if self.model.borrow().busy {
            return;
        }
        let worker = self.worker.clone();
        let source_id = file.to_owned();
        spawn_local(async move {
            let _ = worker
                .call("unregister", &[("id", source_id.into())], None)
                .await;
        });
        self.sources.borrow_mut().retain(|s| s.meta.id != file);
        self.model.borrow_mut().files.retain(|s| s.id != file);
        self.emit(true);
    }
    pub fn clear_files(&self) {
        if self.model.borrow().busy {
            return;
        }
        let ids: Vec<_> = self
            .sources
            .borrow()
            .iter()
            .map(|s| s.meta.id.clone())
            .collect();
        let worker = self.worker.clone();
        spawn_local(async move {
            for id in ids {
                let _ = worker.call("unregister", &[("id", id.into())], None).await;
            }
        });
        self.sources.borrow_mut().clear();
        self.model.borrow_mut().files.clear();
        self.emit(true);
    }
    pub fn bind_drop(self: &Rc<Self>) {
        let Some(el) = window()
            .and_then(|w| w.document())
            .and_then(|d| d.get_element_by_id("dropzone"))
        else {
            return;
        };
        let weak = Rc::downgrade(self);
        let c = Closure::<dyn FnMut(web_sys::DragEvent)>::new(move |ev: web_sys::DragEvent| {
            ev.prevent_default();
            if let (Some(e), Some(files)) =
                (weak.upgrade(), ev.data_transfer().and_then(|d| d.files()))
            {
                e.add_files(files);
            }
        });
        let _ = el.add_event_listener_with_callback("drop", c.as_ref().unchecked_ref());
        c.forget();
    }
    pub fn new_room(self: &Rc<Self>) {
        let stamp = format!("{:08x}", (now() / 1000.0) as u32);
        let code = format!("{}{}", stamp, &id()[..24]);
        if let Err(e) = self.connect(code) {
            self.notice(&e);
        }
    }
    pub fn join_room(self: &Rc<Self>, value: &str) {
        let value = value.trim();
        let code = if value.contains("?") {
            Url::new(value)
                .ok()
                .and_then(|u| UrlSearchParams::new_with_str(&u.search()).ok())
                .and_then(|p| p.get("room"))
                .unwrap_or_default()
        } else {
            value.replace([' ', '-'], "").to_lowercase()
        };
        if code.len() != 32 || !code.bytes().all(|b| b.is_ascii_hexdigit()) {
            self.notice("รหัสกลุ่มไม่ถูกต้อง กรุณาวางลิงก์หรือรหัสที่ได้รับ");
            return;
        }
        if let Err(e) = self.connect(code) {
            self.notice(&e);
        }
    }
    pub fn auto_group(self: &Rc<Self>) {
        if let Err(e) = self.connect(String::new()) {
            self.notice(&e);
        }
    }
    pub fn reconnect(self: &Rc<Self>) {
        let room = self.model.borrow().room.clone();
        if let Err(e) = self.connect(room) {
            self.notice(&e);
        }
    }
    fn connect(self: &Rc<Self>, room: String) -> Result<()> {
        if self.model.borrow().busy || self.incoming.borrow().values().any(|i| i.accepted) {
            return Err("กรุณารอให้ส่งหรือรับไฟล์เสร็จก่อนเปลี่ยนกลุ่ม".into());
        }
        if self.endpoint.borrow().is_empty() {
            return Err("ยังไม่ได้เชื่อมต่อบริการจับคู่".into());
        }
        self.socket.borrow_mut().take();
        self.links.borrow_mut().clear();
        self.incoming.borrow_mut().clear();
        let base = self
            .endpoint
            .borrow()
            .replace("https://", "wss://")
            .replace("http://", "ws://");
        let url = format!(
            "{}/signal{}",
            base.trim_end_matches('/'),
            if room.is_empty() {
                String::new()
            } else {
                format!("?room={room}")
            }
        );
        let ws = WebSocket::new(&url).map_err(error)?;
        {
            let mut m = self.model.borrow_mut();
            m.online = false;
            m.connecting = true;
            m.peers.clear();
            m.selected.clear();
            m.offers.clear();
            m.room = room.clone();
            m.notice.clear();
            m.room_link = if room.is_empty() {
                String::new()
            } else {
                format!(
                    "{}{}?room={room}",
                    window().unwrap().location().origin().map_err(error)?,
                    window().unwrap().location().pathname().map_err(error)?
                )
            };
            m.qr = if m.room_link.is_empty() {
                String::new()
            } else {
                qrcode::QrCode::new(m.room_link.as_bytes())
                    .map(|q| {
                        q.render::<qrcode::render::svg::Color>()
                            .min_dimensions(180, 180)
                            .build()
                    })
                    .unwrap_or_default()
            };
        }
        let mut handlers = vec![];
        let weak = Rc::downgrade(self);
        let h = Closure::new(move |_: JsValue| {
            if let Some(e) = weak.upgrade() {
                e.signal(&ClientMessage::Hello {
                    version: VERSION,
                    device_id: e.device_id.clone(),
                    name: e.model.borrow().name.clone(),
                });
            }
        });
        ws.set_onopen(Some(h.as_ref().unchecked_ref()));
        handlers.push(h);
        let weak = Rc::downgrade(self);
        let h = Closure::new(move |v: JsValue| {
            if let Some(e) = weak.upgrade() {
                let ev: MessageEvent = v.unchecked_into();
                if let Some(t) = ev.data().as_string() {
                    match serde_json::from_str::<ServerMessage>(&t) {
                        Ok(m) => e.server_message(m),
                        Err(_) => e.notice("รูปแบบข้อความจากเซิร์ฟเวอร์ไม่ถูกต้อง"),
                    }
                }
            }
        });
        ws.set_onmessage(Some(h.as_ref().unchecked_ref()));
        handlers.push(h);
        let weak = Rc::downgrade(self);
        let h = Closure::new(move |_: JsValue| {
            if let Some(e) = weak.upgrade() {
                let mut m = e.model.borrow_mut();
                m.online = false;
                m.connecting = false;
                m.peers.clear();
                m.selected.clear();
                drop(m);
                e.emit(true);
            }
        });
        ws.set_onclose(Some(h.as_ref().unchecked_ref()));
        handlers.push(h);
        let weak = Rc::downgrade(self);
        let h = Closure::new(move |_: JsValue| {
            if let Some(e) = weak.upgrade() {
                e.model.borrow_mut().connecting = false;
                e.notice(
                    "เชื่อมต่อรายชื่อผู้รับไม่ได้ กลุ่มอาจหมดอายุ หรืออินเทอร์เน็ตขัดข้อง ลองเชื่อมต่อใหม่หรือสร้างกลุ่มใหม่",
                );
            }
        });
        ws.set_onerror(Some(h.as_ref().unchecked_ref()));
        handlers.push(h);
        *self.socket.borrow_mut() = Some(Socket {
            ws,
            _handlers: handlers,
        });
        self.emit(true);
        Ok(())
    }
    fn signal(&self, m: &ClientMessage) {
        if let Some(s) = self.socket.borrow().as_ref() {
            if s.ws.ready_state() == WebSocket::OPEN {
                if let Ok(text) = serde_json::to_string(m) {
                    let _ = s.ws.send_with_str(&text);
                }
            }
        }
    }
    fn server_message(self: &Rc<Self>, msg: ServerMessage) {
        match msg {
            ServerMessage::Welcome { peer_id, peers } => {
                *self.own_peer.borrow_mut() = peer_id;
                self.server_message(ServerMessage::Peers { peers });
            }
            ServerMessage::Peers { peers } => {
                let mut m = self.model.borrow_mut();
                m.online = true;
                m.connecting = false;
                let ids: HashSet<_> = peers.iter().map(|p| p.id.clone()).collect();
                m.selected.retain(|id| ids.contains(id));
                m.peers = peers;
                drop(m);
                self.emit(true);
            }
            ServerMessage::Signal { from, kind, sdp } => {
                let me = self.clone();
                spawn_local(async move {
                    if let Err(e) = me.receive_signal(&from, &kind, &sdp).await {
                        me.fail_peer(&from, &e);
                    }
                });
            }
            ServerMessage::Error { message } => self.notice(&message),
            ServerMessage::Pong => {}
        }
    }
    fn make_link(self: &Rc<Self>, peer: &str) -> Result<Rc<Link>> {
        let config = RtcConfiguration::new();
        let servers = Array::new();
        let stun = RtcIceServer::new();
        stun.set_urls_str("stun:stun.cloudflare.com:3478");
        servers.push(&stun);
        config.set_ice_servers(&servers);
        let pc = RtcPeerConnection::new_with_configuration(&config).map_err(error)?;
        let link = Rc::new(Link {
            pc,
            dc: RefCell::default(),
            handlers: RefCell::default(),
        });
        let weak = Rc::downgrade(self);
        let p = peer.to_owned();
        let h = Closure::new(move |ev: JsValue| {
            if let Some(e) = weak.upgrade() {
                let ev: RtcDataChannelEvent = ev.unchecked_into();
                e.attach_channel(&p, ev.channel());
            }
        });
        link.pc.set_ondatachannel(Some(h.as_ref().unchecked_ref()));
        link.handlers.borrow_mut().push(h);
        let weak = Rc::downgrade(self);
        let p = peer.to_owned();
        let pc = link.pc.clone();
        let h = Closure::new(move |_: JsValue| {
            if matches!(
                pc.connection_state(),
                RtcPeerConnectionState::Failed | RtcPeerConnectionState::Closed
            ) {
                if let Some(e) = weak.upgrade() {
                    e.fail_peer(
                        &p,
                        "เชื่อมต่อโดยตรงไม่ได้ เครือข่ายอาจแยกอุปกรณ์ ลองใช้ Wi-Fi อื่นหรือปิด VPN",
                    );
                }
            }
        });
        link.pc
            .set_onconnectionstatechange(Some(h.as_ref().unchecked_ref()));
        link.handlers.borrow_mut().push(h);
        self.links.borrow_mut().insert(peer.into(), link.clone());
        Ok(link)
    }
    fn attach_channel(self: &Rc<Self>, peer: &str, dc: RtcDataChannel) {
        let Some(link) = self.links.borrow().get(peer).cloned() else {
            return;
        };
        dc.set_binary_type(RtcDataChannelType::Arraybuffer);
        let weak = Rc::downgrade(self);
        let p = peer.to_owned();
        let h = Closure::new(move |v: JsValue| {
            if let Some(e) = weak.upgrade() {
                let ev: MessageEvent = v.unchecked_into();
                let data = ev.data();
                if let Some(text) = data.as_string() {
                    match serde_json::from_str::<Control>(&text) {
                        Ok(c) => e.control(&p, c),
                        Err(_) => e.fail_peer(&p, "ข้อความรับส่งไฟล์ไม่ถูกต้อง"),
                    }
                } else if data.is_instance_of::<ArrayBuffer>() {
                    let bytes = Uint8Array::new(&data).to_vec();
                    let e = e.clone();
                    let p = p.clone();
                    spawn_local(async move {
                        e.binary(&p, bytes).await;
                    });
                }
            }
        });
        dc.set_onmessage(Some(h.as_ref().unchecked_ref()));
        link.handlers.borrow_mut().push(h);
        let weak = Rc::downgrade(self);
        let p = peer.to_owned();
        let h = Closure::new(move |_: JsValue| {
            if let Some(e) = weak.upgrade() {
                e.fail_peer(&p, "การเชื่อมต่อกับผู้รับสิ้นสุด กรุณาส่งใหม่");
            }
        });
        dc.set_onclose(Some(h.as_ref().unchecked_ref()));
        link.handlers.borrow_mut().push(h);
        *link.dc.borrow_mut() = Some(dc);
    }
    async fn gather(&self, pc: &RtcPeerConnection) -> Result<String> {
        let start = now();
        while pc.ice_gathering_state() != RtcIceGatheringState::Complete {
            if now() - start > 10_000.0 {
                break;
            }
            sleep(50).await;
        }
        pc.local_description()
            .map(|s| s.sdp())
            .ok_or("สร้างข้อมูลเชื่อมต่อไม่ได้".into())
    }
    async fn receive_signal(self: &Rc<Self>, peer: &str, kind: &str, sdp: &str) -> Result<()> {
        if kind == "connect" {
            let me = self.clone();
            let peer = peer.to_owned();
            spawn_local(async move {
                if let Err(e) = me.channel(&peer).await {
                    me.fail_peer(&peer, &e);
                }
            });
            return Ok(());
        }
        if kind == "offer" {
            // Only the lexicographically smaller session initiates SDP, preventing glare.
            if self.links.borrow().get(peer).is_some_and(|l| {
                l.dc.borrow()
                    .as_ref()
                    .is_some_and(|d| d.ready_state() == RtcDataChannelState::Open)
            }) {
                return Ok(());
            }
            let link = self.make_link(peer)?;
            let desc = RtcSessionDescriptionInit::new(RtcSdpType::Offer);
            desc.set_sdp(sdp);
            JsFuture::from(link.pc.set_remote_description(&desc))
                .await
                .map_err(error)?;
            let answer = JsFuture::from(link.pc.create_answer())
                .await
                .map_err(error)?;
            let desc: RtcSessionDescriptionInit = answer.unchecked_into();
            JsFuture::from(link.pc.set_local_description(&desc))
                .await
                .map_err(error)?;
            let sdp = self.gather(&link.pc).await?;
            self.signal(&ClientMessage::Signal {
                to: peer.into(),
                kind: "answer".into(),
                sdp,
            });
        } else if kind == "answer" {
            let link = self
                .links
                .borrow()
                .get(peer)
                .cloned()
                .ok_or("ไม่พบการเชื่อมต่อ")?;
            let desc = RtcSessionDescriptionInit::new(RtcSdpType::Answer);
            desc.set_sdp(sdp);
            JsFuture::from(link.pc.set_remote_description(&desc))
                .await
                .map_err(error)?;
        }
        Ok(())
    }
    async fn channel(self: &Rc<Self>, peer: &str) -> Result<RtcDataChannel> {
        let existing = self.links.borrow().get(peer).cloned();
        let link = if let Some(l) = existing {
            if matches!(
                l.pc.connection_state(),
                RtcPeerConnectionState::Failed | RtcPeerConnectionState::Closed
            ) {
                self.links.borrow_mut().remove(peer);
                self.make_link(peer)?
            } else {
                l
            }
        } else {
            self.make_link(peer)?
        };
        if link.dc.borrow().is_none() && self.own_peer.borrow().as_str() < peer {
            let dc = link.pc.create_data_channel("toolbox-v1");
            self.attach_channel(peer, dc);
            let offer = JsFuture::from(link.pc.create_offer())
                .await
                .map_err(error)?;
            let desc: RtcSessionDescriptionInit = offer.unchecked_into();
            JsFuture::from(link.pc.set_local_description(&desc))
                .await
                .map_err(error)?;
            let sdp = self.gather(&link.pc).await?;
            self.signal(&ClientMessage::Signal {
                to: peer.into(),
                kind: "offer".into(),
                sdp,
            });
        } else if link.dc.borrow().is_none() {
            self.signal(&ClientMessage::Signal {
                to: peer.into(),
                kind: "connect".into(),
                sdp: String::new(),
            });
        }
        let start = now();
        loop {
            let current = self
                .links
                .borrow()
                .get(peer)
                .cloned()
                .ok_or("การเชื่อมต่อสิ้นสุดแล้ว")?;
            if let Some(dc) = current.dc.borrow().as_ref() {
                if dc.ready_state() == RtcDataChannelState::Open {
                    return Ok(dc.clone());
                }
            }
            if now() - start > 30_000.0 {
                return Err(
                    "เชื่อมต่อโดยตรงไม่สำเร็จ ลองเปลี่ยน Wi-Fi หรือปิด VPN (ไม่มีการส่งผ่าน relay)".into(),
                );
            }
            sleep(50).await;
        }
    }
    fn send_control(&self, peer: &str, c: &Control) -> Result<()> {
        let link = self
            .links
            .borrow()
            .get(peer)
            .cloned()
            .ok_or("ไม่พบการเชื่อมต่อ")?;
        let dc = link.dc.borrow().clone().ok_or("ยังไม่เชื่อมต่อ")?;
        let text = serde_json::to_string(c).map_err(|e| e.to_string())?;
        if text.len() > FRAME_BYTES {
            return Err("รายละเอียดไฟล์ยาวเกินขนาดข้อความที่รองรับ".into());
        }
        dc.send_with_str(&text).map_err(error)
    }
    pub fn send(self: &Rc<Self>) {
        let m = self.model.borrow();
        if !m.online || !m.worker_ready || m.busy || m.selected.is_empty() || m.files.is_empty() {
            return;
        }
        let sources = self.sources.borrow().clone();
        if sources.len() > 4096 {
            drop(m);
            self.notice("กรุณาแบ่งส่งครั้งละไม่เกิน 4,096 ไฟล์");
            return;
        }
        let peers: Vec<_> = m
            .peers
            .iter()
            .filter(|p| m.selected.contains(&p.id))
            .cloned()
            .collect();
        drop(m);
        let total = sources
            .iter()
            .try_fold(0u64, |a, s| a.checked_add(s.meta.size));
        let Some(total) = total else {
            self.notice("ขนาดไฟล์รวมเกินช่วงที่รองรับ");
            return;
        };
        for p in peers {
            let batch = id();
            self.model.borrow_mut().transfers.push(TransferView {
                batch: batch.clone(),
                peer: p.name,
                incoming: false,
                total,
                status: "รอคิว".into(),
                ..Default::default()
            });
            self.queue
                .borrow_mut()
                .push_back((p.id, sources.clone(), batch));
        }
        self.model.borrow_mut().busy = true;
        self.emit(true);
        self.pump();
    }
    fn pump(self: &Rc<Self>) {
        while self.active.get() < 3 {
            let next = self.queue.borrow_mut().pop_front();
            let Some((peer, files, batch)) = next else {
                break;
            };
            self.active.set(self.active.get() + 1);
            let outgoing = Rc::new(Outgoing {
                peer: peer.clone(),
                files,
                state: RefCell::default(),
            });
            self.outgoing
                .borrow_mut()
                .insert(batch.clone(), outgoing.clone());
            let me = self.clone();
            spawn_local(async move {
                let result = me.transmit(&batch, outgoing.clone()).await;
                if let Err(reason) = result {
                    me.cancel(&batch, &reason);
                } else {
                    me.row_status(&batch, "ส่งสำเร็จ · ตรวจสอบ SHA-256 แล้ว", true, false);
                }
                me.outgoing.borrow_mut().remove(&batch);
                me.active.set(me.active.get() - 1);
                if me.active.get() == 0 && me.queue.borrow().is_empty() {
                    me.model.borrow_mut().busy = false;
                }
                me.emit(true);
                me.pump();
            });
        }
    }
    fn row_status(&self, batch: &str, status: &str, finished: bool, failed: bool) {
        if let Some(t) = self
            .model
            .borrow_mut()
            .transfers
            .iter_mut()
            .find(|t| t.batch == batch)
        {
            t.status = status.into();
            t.finished = finished;
            t.failed = failed;
        }
        self.emit(true);
    }
    fn progress(&self, batch: &str, done: u64) {
        if let Some(t) = self
            .model
            .borrow_mut()
            .transfers
            .iter_mut()
            .find(|t| t.batch == batch)
        {
            t.done = done.min(t.total);
        }
        self.emit(false);
    }
    async fn wait_send(
        &self,
        out: &Rc<Outgoing>,
        limit_ms: f64,
        test: impl Fn(&SendState) -> bool,
    ) -> Result<()> {
        let start = now();
        loop {
            let s = out.state.borrow();
            if let Some(e) = &s.error {
                return Err(e.clone());
            }
            if test(&s) {
                return Ok(());
            }
            drop(s);
            if now() - start > limit_ms {
                return Err("รอนานเกินไป กรุณาเปิดหน้าเว็บทั้งสองเครื่องไว้แล้วลองส่งใหม่".into());
            }
            sleep(10).await;
        }
    }
    async fn transmit(self: &Rc<Self>, batch: &str, out: Rc<Outgoing>) -> Result<()> {
        self.row_status(batch, "กำลังเชื่อมต่อโดยตรง…", false, false);
        let dc = self.channel(&out.peer).await?;
        self.wait_send(&out, 1.0, |_| true).await?;
        self.send_control(
            &out.peer,
            &Control::OfferStart {
                version: VERSION,
                batch: batch.into(),
                sender: self.model.borrow().name.clone(),
            },
        )?;
        for file in &out.files {
            self.send_control(
                &out.peer,
                &Control::OfferFile {
                    batch: batch.into(),
                    file: file.meta.clone(),
                },
            )?;
        }
        self.send_control(
            &out.peer,
            &Control::OfferEnd {
                batch: batch.into(),
            },
        )?;
        self.row_status(batch, "รอผู้รับยอมรับ", false, false);
        self.wait_send(&out, 300_000.0, |s| s.accepted).await?;
        let mut completed = 0u64;
        for source in &out.files {
            let job = id();
            out.state.borrow_mut().ack.insert(job.clone(), 0);
            out.state.borrow_mut().sent.insert(job.clone(), 0);
            self.worker
                .call(
                    "register",
                    &[
                        ("id", source.meta.id.clone().into()),
                        ("file", source.file.clone().into()),
                    ],
                    None,
                )
                .await?;
            self.send_control(
                &out.peer,
                &Control::Start {
                    batch: batch.into(),
                    file: source.meta.id.clone(),
                    job: job.clone(),
                },
            )?;
            self.row_status(batch, &format!("กำลังส่ง {}", source.meta.name), false, false);
            self.wait_send(&out, 120_000.0, |s| s.ready.contains(&job))
                .await?;
            let mut offset = 0u64;
            let mut last_progress = now();
            let mut last_ack = 0;
            while offset < source.meta.size {
                let failure = out.state.borrow().error.clone();
                if let Some(e) = failure {
                    let _ = self
                        .worker
                        .call("abort", &[("job", job.clone().into())], None)
                        .await;
                    return Err(e);
                }
                let ack = *out.state.borrow().ack.get(&job).unwrap_or(&0);
                if ack > last_ack {
                    last_ack = ack;
                    last_progress = now();
                    self.progress(batch, completed + ack);
                }
                if offset.saturating_sub(ack) >= WINDOW_BYTES
                    || dc.buffered_amount() > WINDOW_BYTES as u32
                {
                    if now() - last_progress > 60_000.0 {
                        return Err("ผู้รับไม่ตอบสนอง กรุณาส่งใหม่".into());
                    }
                    sleep(5).await;
                    continue;
                }
                let data = self
                    .worker
                    .call(
                        "read",
                        &[
                            ("id", source.meta.id.clone().into()),
                            ("job", job.clone().into()),
                            ("offset", offset.to_string().into()),
                            (
                                "length",
                                ((source.meta.size - offset).min(PAYLOAD_BYTES as u64) as u32)
                                    .into(),
                            ),
                        ],
                        None,
                    )
                    .await?;
                let bytes = Uint8Array::new(&get(&data, "bytes")).to_vec();
                if bytes.is_empty() {
                    return Err("อ่านไฟล์ต้นฉบับไม่ครบ".into());
                }
                let frame = encode_frame(&job, offset, &bytes)?;
                dc.send_with_u8_array(&frame).map_err(error)?;
                offset += bytes.len() as u64;
                out.state.borrow_mut().sent.insert(job.clone(), offset);
            }
            self.wait_send(&out, 120_000.0, |s| {
                s.ack.get(&job) == Some(&source.meta.size)
            })
            .await?;
            let digest = self
                .worker
                .call("digest", &[("job", job.clone().into())], None)
                .await?;
            let digest = get(&digest, "digest")
                .as_string()
                .ok_or("คำนวณ SHA-256 ไม่สำเร็จ")?;
            self.send_control(
                &out.peer,
                &Control::End {
                    job: job.clone(),
                    digest,
                },
            )?;
            self.row_status(batch, "ผู้รับกำลังตรวจสอบไฟล์…", false, false);
            self.wait_send(&out, 120_000.0, |s| s.verified.contains(&job))
                .await?;
            completed += source.meta.size;
            self.progress(batch, completed);
        }
        self.send_control(
            &out.peer,
            &Control::BatchDone {
                batch: batch.into(),
            },
        )?;
        Ok(())
    }
    fn control(self: &Rc<Self>, peer: &str, c: Control) {
        match c {
            Control::OfferStart {
                version,
                batch,
                sender,
            } => {
                if version != VERSION
                    || toolbox_core::id_bytes(&batch).is_err()
                    || self.incoming.borrow().contains_key(&batch)
                {
                    return;
                }
                if self
                    .incoming
                    .borrow()
                    .values()
                    .filter(|i| i.peer == peer)
                    .count()
                    >= 8
                {
                    let _ = self.send_control(peer, &Control::Reject { batch });
                    return;
                }
                self.incoming.borrow_mut().insert(
                    batch,
                    Incoming {
                        peer: peer.into(),
                        sender: sender.chars().take(40).collect(),
                        files: vec![],
                        accepted: false,
                        handle: JsValue::UNDEFINED,
                        active: HashMap::new(),
                        completed: 0,
                    },
                );
            }
            Control::OfferFile { batch, file } => {
                let valid = toolbox_core::id_bytes(&file.id).is_ok()
                    && file.size <= 9_007_199_254_740_991
                    && file.name.len() < 4096;
                let mut incoming = self.incoming.borrow_mut();
                if let Some(i) = incoming.get_mut(&batch) {
                    if i.peer == peer
                        && !i.accepted
                        && valid
                        && i.files.len() < 4096
                        && !i.files.iter().any(|f| f.id == file.id)
                    {
                        i.files.push(file);
                    }
                }
            }
            Control::OfferEnd { batch } => {
                let incoming = self.incoming.borrow();
                if let Some(i) = incoming.get(&batch) {
                    if i.peer == peer
                        && !i.files.is_empty()
                        && !i.accepted
                        && i.files
                            .iter()
                            .try_fold(0u64, |n, f| n.checked_add(f.size))
                            .is_some()
                    {
                        let mut m = self.model.borrow_mut();
                        if !m.offers.iter().any(|o| o.batch == batch) {
                            m.offers.push(IncomingView {
                                batch,
                                sender: i.sender.clone(),
                                files: i.files.clone(),
                            });
                        }
                    }
                }
                drop(incoming);
                self.emit(true);
            }
            Control::Accept { batch } => {
                if let Some(o) = self.outgoing.borrow().get(&batch) {
                    if o.peer == peer {
                        o.state.borrow_mut().accepted = true;
                    }
                }
            }
            Control::Reject { batch } => {
                if self
                    .outgoing
                    .borrow()
                    .get(&batch)
                    .is_some_and(|o| o.peer == peer)
                {
                    self.cancel(&batch, "ผู้รับปฏิเสธไฟล์");
                }
            }
            Control::Start { batch, file, job } => {
                if toolbox_core::id_bytes(&job).is_err() {
                    return;
                }
                let details = {
                    let mut incoming = self.incoming.borrow_mut();
                    incoming.get_mut(&batch).and_then(|i| {
                        if i.peer != peer || !i.accepted || !i.active.is_empty() {
                            return None;
                        }
                        i.files.iter().find(|f| f.id == file).cloned().map(|f| {
                            i.active.insert(job.clone(), f.clone());
                            (f, i.handle.clone())
                        })
                    })
                };
                if let Some((file, handle)) = details {
                    self.row_status(&batch, &format!("กำลังรับ {}", file.name), false, false);
                    let me = self.clone();
                    let peer = peer.to_owned();
                    spawn_local(async move {
                        let result = me
                            .worker
                            .call(
                                "prepare",
                                &[
                                    ("job", job.clone().into()),
                                    ("size", file.size.to_string().into()),
                                    ("name", file.name.into()),
                                    ("handle", handle),
                                ],
                                None,
                            )
                            .await;
                        if let Err(e) = result {
                            me.cancel(&batch, &e);
                        } else if me.incoming.borrow().contains_key(&batch) {
                            let _ = me.send_control(&peer, &Control::Ready { job });
                        } else {
                            let _ = me.worker.call("abort", &[("job", job.into())], None).await;
                        }
                    });
                }
            }
            Control::Ready { job } => {
                for o in self.outgoing.borrow().values() {
                    if o.peer == peer && o.state.borrow().sent.contains_key(&job) {
                        o.state.borrow_mut().ready.insert(job.clone());
                    }
                }
            }
            Control::Ack { job, offset } => {
                for o in self.outgoing.borrow().values() {
                    if o.peer == peer {
                        let mut s = o.state.borrow_mut();
                        if let Some(sent) = s.sent.get(&job).copied() {
                            if offset <= sent && offset >= *s.ack.get(&job).unwrap_or(&0) {
                                s.ack.insert(job.clone(), offset);
                            }
                        }
                    }
                }
            }
            Control::End { job, digest } => {
                let detail = self.incoming.borrow().iter().find_map(|(batch, i)| {
                    if i.peer == peer {
                        i.active.get(&job).map(|f| (batch.clone(), f.clone()))
                    } else {
                        None
                    }
                });
                if let Some((batch, file)) = detail {
                    let me = self.clone();
                    let peer = peer.to_owned();
                    spawn_local(async move {
                        match me
                            .worker
                            .call(
                                "finish",
                                &[("job", job.clone().into()), ("digest", digest.into())],
                                None,
                            )
                            .await
                        {
                            Ok(data) => {
                                if !me.incoming.borrow().contains_key(&batch) {
                                    let _ = me
                                        .worker
                                        .call("delete", &[("job", job.into())], None)
                                        .await;
                                    return;
                                }
                                let direct = get(&data, "savedDirectly").as_bool().unwrap_or(false);
                                let url = if direct {
                                    String::new()
                                } else {
                                    match get(&data, "file")
                                        .dyn_into::<Blob>()
                                        .map_err(error)
                                        .and_then(|b| {
                                            Url::create_object_url_with_blob(&b).map_err(error)
                                        }) {
                                        Ok(url) => url,
                                        Err(e) => {
                                            me.cancel(&batch, &e);
                                            return;
                                        }
                                    }
                                };
                                me.model.borrow_mut().downloads.push(Download {
                                    job: job.clone(),
                                    name: file.name,
                                    size: file.size,
                                    url,
                                    direct,
                                });
                                if let Some(i) = me.incoming.borrow_mut().get_mut(&batch) {
                                    i.active.remove(&job);
                                    i.completed += 1;
                                }
                                let _ = me.send_control(&peer, &Control::Verified { job });
                                me.emit(true);
                            }
                            Err(e) => me.cancel(&batch, &e),
                        }
                    });
                }
            }
            Control::Verified { job } => {
                for o in self.outgoing.borrow().values() {
                    if o.peer == peer && o.state.borrow().sent.contains_key(&job) {
                        o.state.borrow_mut().verified.insert(job.clone());
                    }
                }
            }
            Control::Cancel { batch, reason } => {
                let matches = self
                    .outgoing
                    .borrow()
                    .get(&batch)
                    .is_some_and(|o| o.peer == peer)
                    || self
                        .incoming
                        .borrow()
                        .get(&batch)
                        .is_some_and(|i| i.peer == peer);
                if matches {
                    self.cancel(&batch, &reason.chars().take(200).collect::<String>());
                }
            }
            Control::BatchDone { batch } => {
                let finished = self.incoming.borrow().get(&batch).is_some_and(|i| {
                    i.peer == peer
                        && i.accepted
                        && i.completed == i.files.len()
                        && i.active.is_empty()
                });
                if finished {
                    self.incoming.borrow_mut().remove(&batch);
                    self.row_status(&batch, "รับครบแล้ว · ตรวจสอบ SHA-256 ผ่าน", true, false);
                }
            }
        }
    }
    async fn binary(self: &Rc<Self>, peer: &str, frame: Vec<u8>) {
        let (job, offset, bytes) = match decode_frame(&frame) {
            Ok(v) => v,
            Err(_) => {
                self.fail_peer(peer, "รูปแบบข้อมูลไฟล์ไม่ถูกต้อง");
                return;
            }
        };
        let batch = self.incoming.borrow().iter().find_map(|(b, i)| {
            if i.peer == peer && i.accepted && i.active.contains_key(&job) {
                Some(b.clone())
            } else {
                None
            }
        });
        let Some(batch) = batch else {
            return;
        };
        let array = Uint8Array::from(bytes);
        let buffer: JsValue = array.buffer().into();
        match self
            .worker
            .call(
                "write",
                &[
                    ("job", job.clone().into()),
                    ("offset", offset.to_string().into()),
                    ("bytes", buffer.clone()),
                ],
                Some(&buffer),
            )
            .await
        {
            Ok(data) => {
                if !self.incoming.borrow().contains_key(&batch) {
                    return;
                }
                let next = get(&data, "offset")
                    .as_string()
                    .and_then(|s| s.parse::<u64>().ok())
                    .unwrap_or(offset);
                if let Some(t) = self
                    .model
                    .borrow_mut()
                    .transfers
                    .iter_mut()
                    .find(|t| t.batch == batch)
                {
                    t.done = t.done.saturating_add(bytes.len() as u64).min(t.total);
                }
                let _ = self.send_control(peer, &Control::Ack { job, offset: next });
                self.emit(false);
            }
            Err(e) => self.cancel(&batch, &e),
        }
    }
    pub fn accept(self: &Rc<Self>, batch: &str) {
        let details = self
            .incoming
            .borrow()
            .get(batch)
            .filter(|i| !i.accepted)
            .map(|i| (i.peer.clone(), i.sender.clone(), i.files.clone()));
        let Some((peer, sender, files)) = details else {
            return;
        };
        let window: JsValue = window().unwrap().into();
        let opts: JsValue = Object::new().into();
        // Invoke the picker synchronously inside the user's click gesture, before any await.
        let picker = if files.len() == 1 && get(&window, "showSaveFilePicker").is_function() {
            set(
                &opts,
                "suggestedName",
                &safe_filename(&files[0].name).into(),
            );
            invoke(&window, "showSaveFilePicker", &[opts])
        } else if files.len() > 1 && get(&window, "showDirectoryPicker").is_function() {
            set(&opts, "mode", &"readwrite".into());
            invoke(&window, "showDirectoryPicker", &[opts])
        } else {
            Ok(JsValue::UNDEFINED)
        };
        let batch = batch.to_owned();
        let me = self.clone();
        spawn_local(async move {
            let result = match picker {
                Ok(v) if v.is_undefined() => Ok(v),
                Ok(v) => resolve(v).await,
                Err(e) => Err(e),
            };
            match result {
                Ok(handle) => {
                    let accepted = {
                        let mut incoming = me.incoming.borrow_mut();
                        if let Some(i) = incoming.get_mut(&batch) {
                            if i.accepted {
                                false
                            } else {
                                i.accepted = true;
                                i.handle = handle;
                                true
                            }
                        } else {
                            false
                        }
                    };
                    if !accepted {
                        return;
                    }
                    let total = files.iter().map(|f| f.size).sum();
                    let mut m = me.model.borrow_mut();
                    m.offers.retain(|o| o.batch != batch);
                    m.transfers.push(TransferView {
                        batch: batch.clone(),
                        peer: sender,
                        incoming: true,
                        total,
                        status: "เตรียมรับไฟล์…".into(),
                        ..Default::default()
                    });
                    drop(m);
                    if let Err(e) = me.send_control(
                        &peer,
                        &Control::Accept {
                            batch: batch.clone(),
                        },
                    ) {
                        me.cancel(&batch, &e);
                    }
                    me.emit(true);
                }
                Err(e) => {
                    if !e.contains("AbortError") && !e.to_lowercase().contains("abort") {
                        me.notice(&format!("เปิดที่บันทึกไม่ได้: {e}"));
                    }
                }
            }
        });
    }
    pub fn reject(&self, batch: &str) {
        if let Some(i) = self.incoming.borrow_mut().remove(batch) {
            let _ = self.send_control(
                &i.peer,
                &Control::Reject {
                    batch: batch.into(),
                },
            );
        }
        self.model.borrow_mut().offers.retain(|o| o.batch != batch);
        self.emit(true);
    }
    pub fn cancel(self: &Rc<Self>, batch: &str, reason: &str) {
        let queued = self.queue.borrow().iter().any(|(_, _, b)| b == batch);
        self.queue.borrow_mut().retain(|(_, _, b)| b != batch);
        let out = self.outgoing.borrow().get(batch).cloned();
        if let Some(o) = out {
            let first = o.state.borrow().error.is_none();
            o.state.borrow_mut().error = Some(reason.into());
            if first {
                let _ = self.send_control(
                    &o.peer,
                    &Control::Cancel {
                        batch: batch.into(),
                        reason: reason.into(),
                    },
                );
            }
            let jobs: Vec<_> = o.state.borrow().sent.keys().cloned().collect();
            let worker = self.worker.clone();
            spawn_local(async move {
                for job in jobs {
                    let _ = worker.call("abort", &[("job", job.into())], None).await;
                }
            });
        }
        let incoming = self.incoming.borrow_mut().remove(batch);
        if let Some(i) = incoming {
            let _ = self.send_control(
                &i.peer,
                &Control::Cancel {
                    batch: batch.into(),
                    reason: reason.into(),
                },
            );
            let worker = self.worker.clone();
            spawn_local(async move {
                for job in i.active.keys() {
                    let _ = worker
                        .call("abort", &[("job", job.clone().into())], None)
                        .await;
                }
            });
        }
        self.model.borrow_mut().offers.retain(|o| o.batch != batch);
        self.row_status(batch, reason, true, true);
        if queued && self.active.get() == 0 && self.queue.borrow().is_empty() {
            self.model.borrow_mut().busy = false;
            self.emit(true);
        }
    }
    fn fail_peer(self: &Rc<Self>, peer: &str, reason: &str) {
        let mut batches: Vec<_> = self
            .outgoing
            .borrow()
            .iter()
            .filter(|(_, o)| o.peer == peer)
            .map(|(b, _)| b.clone())
            .collect();
        batches.extend(
            self.incoming
                .borrow()
                .iter()
                .filter(|(_, i)| i.peer == peer)
                .map(|(b, _)| b.clone()),
        );
        for batch in batches {
            self.cancel(&batch, reason);
        }
    }
    pub fn delete_download(self: &Rc<Self>, job: &str) {
        let item = self
            .model
            .borrow()
            .downloads
            .iter()
            .find(|d| d.job == job)
            .cloned();
        if let Some(item) = item {
            if !item.url.is_empty() {
                let _ = Url::revoke_object_url(&item.url);
            }
            self.model.borrow_mut().downloads.retain(|d| d.job != job);
            let worker = self.worker.clone();
            let job = job.to_owned();
            spawn_local(async move {
                let _ = worker.call("delete", &[("job", job.into())], None).await;
            });
            self.emit(true);
        }
    }
}
