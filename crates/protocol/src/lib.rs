use serde::{Deserialize, Serialize};

pub const VERSION: u16 = 1;
pub const FRAME_BYTES: usize = 16 * 1024;
pub const HEADER_BYTES: usize = 24;
pub const PAYLOAD_BYTES: usize = FRAME_BYTES - HEADER_BYTES;
pub const WINDOW_BYTES: u64 = (PAYLOAD_BYTES * 16) as u64;

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct Peer {
    pub id: String,
    pub name: String,
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct FileMeta {
    pub id: String,
    pub name: String,
    pub size: u64,
    pub mime: String,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ClientMessage {
    Hello {
        version: u16,
        device_id: String,
        name: String,
    },
    Rename {
        name: String,
    },
    Signal {
        to: String,
        kind: String,
        sdp: String,
    },
    Ping,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ServerMessage {
    Welcome {
        peer_id: String,
        peers: Vec<Peer>,
    },
    Peers {
        peers: Vec<Peer>,
    },
    Signal {
        from: String,
        kind: String,
        sdp: String,
    },
    Error {
        message: String,
    },
    Pong,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Control {
    OfferStart {
        version: u16,
        batch: String,
        sender: String,
    },
    OfferFile {
        batch: String,
        file: FileMeta,
    },
    OfferEnd {
        batch: String,
    },
    Accept {
        batch: String,
    },
    Reject {
        batch: String,
    },
    Start {
        batch: String,
        file: String,
        job: String,
    },
    Ready {
        job: String,
    },
    Ack {
        job: String,
        offset: u64,
    },
    End {
        job: String,
        digest: String,
    },
    Verified {
        job: String,
    },
    Cancel {
        batch: String,
        reason: String,
    },
    BatchDone {
        batch: String,
    },
}
