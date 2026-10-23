use hmac::{Hmac, Mac};
use serde::{Deserialize, Serialize};
use sha2::Sha256;
use toolbox_protocol::{ClientMessage, Peer, ServerMessage, VERSION};
use wasm_bindgen::JsCast;
use worker::*;

fn now() -> u64 {
    (js_sys::Date::now() / 1000.0) as u64
}
fn random_id() -> Result<String> {
    let global = js_sys::global();
    let crypto = js_sys::Reflect::get(&global, &"crypto".into())?;
    let f = js_sys::Reflect::get(&crypto, &"randomUUID".into())?
        .dyn_into::<js_sys::Function>()
        .map_err(|_| Error::RustError("crypto unavailable".into()))?;
    Ok(f.call0(&crypto)?
        .as_string()
        .unwrap_or_default()
        .replace('-', ""))
}
fn clean_name(name: &str) -> String {
    let s: String = name.chars().filter(|c| !c.is_control()).take(40).collect();
    if s.trim().is_empty() {
        "อุปกรณ์".into()
    } else {
        s.trim().into()
    }
}
fn group_key(secret: &str, scope: &str, value: &str) -> String {
    let mut mac =
        Hmac::<Sha256>::new_from_slice(secret.as_bytes()).expect("HMAC accepts any key length");
    mac.update(scope.as_bytes());
    mac.update(b"\0");
    mac.update(value.as_bytes());
    mac.finalize()
        .into_bytes()
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}
#[event(fetch)]
pub async fn main(req: Request, env: Env, _ctx: Context) -> Result<Response> {
    let url = req.url()?;
    if url.path() == "/health" {
        return Response::from_json(&serde_json::json!({"ok":true,"version":VERSION}));
    }
    if url.path() != "/signal" {
        return Response::error("Not found", 404);
    }
    if req.method() != Method::Get
        || req
            .headers()
            .get("Upgrade")?
            .unwrap_or_default()
            .to_lowercase()
            != "websocket"
    {
        return Response::error("WebSocket required", 426);
    }
    let origin = req.headers().get("Origin")?.unwrap_or_default();
    let allowed = env
        .var("ALLOWED_ORIGINS")
        .map(|x| x.to_string())
        .unwrap_or_default();
    if !allowed
        .split(',')
        .map(str::trim)
        .any(|x| !x.is_empty() && x == origin)
    {
        return Response::error("Origin not allowed", 403);
    }
    let secret = env.secret("DISCOVERY_SECRET")?.to_string();
    if secret.len() < 32 {
        return Response::error("Discovery secret not configured", 503);
    }
    let room = url
        .query_pairs()
        .find(|(k, _)| k == "room")
        .map(|(_, v)| v.into_owned());
    let (key, expires) = if let Some(room) = room {
        if room.len() != 32 || !room.bytes().all(|c| c.is_ascii_hexdigit()) {
            return Response::error("Invalid room code", 400);
        }
        let created = u64::from_str_radix(&room[..8], 16).unwrap_or(0);
        if created > now() + 60 || now().saturating_sub(created) > 1800 {
            return Response::error("Room expired", 410);
        }
        (
            group_key(&secret, "room", &room.to_lowercase()),
            created + 1800,
        )
    } else {
        let ip = req
            .headers()
            .get("CF-Connecting-IP")?
            .unwrap_or_else(|| "local-development".into());
        (group_key(&secret, "network", &ip), 0)
    };
    let namespace = env.durable_object("ROOMS")?;
    let stub = namespace.id_from_name(&key)?.get_stub()?;
    let mut inner = Request::new("https://room.internal/connect", Method::Get)?;
    inner
        .headers_mut()?
        .set("X-Room-Expires", &expires.to_string())?;
    inner.headers_mut()?.set("Upgrade", "websocket")?;
    stub.fetch_with_request(inner).await
}
#[derive(Clone, Serialize, Deserialize)]
struct Session {
    id: String,
    name: Option<String>,
    expires: u64,
    window: u64,
    messages: u32,
}
#[durable_object]
pub struct Room {
    state: State,
}
impl Room {
    fn peers(&self, exclude: &str) -> Vec<Peer> {
        self.state
            .get_websockets()
            .iter()
            .filter_map(|ws| ws.deserialize_attachment::<Session>().ok().flatten())
            .filter(|s| s.id != exclude && s.name.is_some())
            .map(|s| Peer {
                id: s.id,
                name: s.name.unwrap(),
            })
            .collect()
    }
    fn broadcast(&self, exclude: &str) {
        for socket in self.state.get_websockets() {
            if let Ok(Some(s)) = socket.deserialize_attachment::<Session>() {
                if s.id != exclude && s.name.is_some() {
                    let _ = socket.send(&ServerMessage::Peers {
                        peers: self
                            .peers(&s.id)
                            .into_iter()
                            .filter(|p| p.id != exclude)
                            .collect(),
                    });
                }
            }
        }
    }
}
impl DurableObject for Room {
    fn new(state: State, _env: Env) -> Self {
        Self { state }
    }
    async fn fetch(&self, req: Request) -> Result<Response> {
        if self.state.get_websockets().len() >= 100 {
            return Response::error("Group full", 429);
        }
        let pair = WebSocketPair::new()?;
        let expires = req
            .headers()
            .get("X-Room-Expires")?
            .unwrap_or_default()
            .parse()
            .unwrap_or(0);
        pair.server.serialize_attachment(Session {
            id: random_id()?,
            name: None,
            expires,
            window: now(),
            messages: 0,
        })?;
        self.state.accept_web_socket(&pair.server);
        Response::from_websocket(pair.client)
    }
    async fn websocket_message(
        &self,
        ws: WebSocket,
        message: WebSocketIncomingMessage,
    ) -> Result<()> {
        let mut s = ws
            .deserialize_attachment::<Session>()?
            .ok_or_else(|| Error::RustError("missing session".into()))?;
        if s.expires > 0 && now() > s.expires {
            let _ = ws.send(&ServerMessage::Error {
                message: "กลุ่มหมดอายุแล้ว กรุณาสร้างกลุ่มใหม่".into(),
            });
            return ws.close(Some(1008), Some("Room expired"));
        }
        if now().saturating_sub(s.window) >= 10 {
            s.window = now();
            s.messages = 0;
        }
        s.messages += 1;
        if s.messages > 60 {
            return ws.close(Some(1008), Some("Rate limited"));
        }
        let text = match message {
            WebSocketIncomingMessage::String(t) if t.len() <= 80 * 1024 => t,
            _ => return ws.close(Some(1009), Some("Invalid message")),
        };
        let msg: ClientMessage = match serde_json::from_str(&text) {
            Ok(m) => m,
            Err(_) => return ws.close(Some(1008), Some("Invalid protocol")),
        };
        match msg {
            ClientMessage::Hello {
                version,
                device_id: _,
                name,
            } => {
                if version != VERSION || s.name.is_some() {
                    return ws.close(Some(1008), Some("Protocol mismatch"));
                }
                s.name = Some(clean_name(&name));
                ws.serialize_attachment(&s)?;
                ws.send(&ServerMessage::Welcome {
                    peer_id: s.id.clone(),
                    peers: self.peers(&s.id),
                })?;
                self.broadcast("");
            }
            ClientMessage::Rename { name } if s.name.is_some() => {
                s.name = Some(clean_name(&name));
                ws.serialize_attachment(&s)?;
                self.broadcast("");
            }
            ClientMessage::Signal { to, kind, sdp } if s.name.is_some() => {
                if !matches!(kind.as_str(), "offer" | "answer" | "connect") || sdp.len() > 64 * 1024
                {
                    return ws.close(Some(1008), Some("Invalid signal"));
                }
                if let Some(target) = self.state.get_websockets().into_iter().find(|w| {
                    w.deserialize_attachment::<Session>()
                        .ok()
                        .flatten()
                        .is_some_and(|a| a.id == to && a.name.is_some())
                }) {
                    target.send(&ServerMessage::Signal {
                        from: s.id.clone(),
                        kind,
                        sdp,
                    })?;
                } else {
                    ws.send(&ServerMessage::Error {
                        message: "ผู้รับออกจากกลุ่มแล้ว".into(),
                    })?;
                }
            }
            ClientMessage::Ping => {
                ws.serialize_attachment(&s)?;
                ws.send(&ServerMessage::Pong)?;
            }
            _ => return ws.close(Some(1008), Some("Hello required")),
        }
        ws.serialize_attachment(&s)?;
        Ok(())
    }
    async fn websocket_close(
        &self,
        ws: WebSocket,
        code: usize,
        _reason: String,
        _clean: bool,
    ) -> Result<()> {
        let s = ws.deserialize_attachment::<Session>()?;
        let _ = ws.close(Some(code as u16), Some("closed"));
        if let Some(s) = s {
            self.broadcast(&s.id);
        }
        Ok(())
    }
    async fn websocket_error(&self, ws: WebSocket, _error: Error) -> Result<()> {
        let s = ws.deserialize_attachment::<Session>()?;
        let _ = ws.close(Some(1011), Some("connection error"));
        if let Some(s) = s {
            self.broadcast(&s.id);
        }
        Ok(())
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn discovery_is_keyed_and_domain_separated() {
        let s = "a strong test secret for discovery";
        assert_ne!(
            group_key(s, "network", "1.2.3.4"),
            group_key(s, "room", "1.2.3.4")
        );
        assert_ne!(
            group_key(s, "network", "1.2.3.4"),
            group_key("another secret", "network", "1.2.3.4")
        );
    }
    #[test]
    fn names_are_bounded() {
        assert_eq!(clean_name(" \n "), "อุปกรณ์");
        assert_eq!(clean_name(&"ก".repeat(100)).chars().count(), 40);
    }
}
