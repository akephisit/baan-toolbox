use sha2::{Digest, Sha256};
use toolbox_protocol::{FRAME_BYTES, HEADER_BYTES};

#[derive(Default)]
pub struct IncrementalHash(Sha256);
impl IncrementalHash {
    pub fn update(&mut self, bytes: &[u8]) {
        self.0.update(bytes);
    }
    pub fn digest(&self) -> String {
        hex(&self.0.clone().finalize())
    }
}
pub fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}
pub fn id_bytes(id: &str) -> Result<[u8; 16], String> {
    if id.len() != 32 || !id.bytes().all(|c| c.is_ascii_hexdigit()) {
        return Err("invalid transfer id".into());
    }
    let mut out = [0; 16];
    for (i, b) in out.iter_mut().enumerate() {
        *b = u8::from_str_radix(&id[i * 2..i * 2 + 2], 16).map_err(|_| "invalid transfer id")?;
    }
    Ok(out)
}
pub fn encode_frame(job: &str, offset: u64, payload: &[u8]) -> Result<Vec<u8>, String> {
    if payload.len() > FRAME_BYTES - HEADER_BYTES {
        return Err("frame too large".into());
    }
    offset
        .checked_add(payload.len() as u64)
        .ok_or("offset overflow")?;
    let mut frame = Vec::with_capacity(HEADER_BYTES + payload.len());
    frame.extend_from_slice(&id_bytes(job)?);
    frame.extend_from_slice(&offset.to_le_bytes());
    frame.extend_from_slice(payload);
    Ok(frame)
}
pub fn decode_frame(frame: &[u8]) -> Result<(String, u64, &[u8]), String> {
    if !(HEADER_BYTES..=FRAME_BYTES).contains(&frame.len()) {
        return Err("invalid frame length".into());
    }
    let offset = u64::from_le_bytes(frame[16..24].try_into().unwrap());
    offset
        .checked_add((frame.len() - HEADER_BYTES) as u64)
        .ok_or("offset overflow")?;
    Ok((hex(&frame[..16]), offset, &frame[HEADER_BYTES..]))
}
pub fn safe_filename(name: &str) -> String {
    let name: String = name
        .chars()
        .map(|c| {
            if c.is_control() || matches!(c, '/' | '\\' | ':' | '*' | '?' | '"' | '<' | '>' | '|') {
                '_'
            } else {
                c
            }
        })
        .collect();
    let name = name.trim_matches(|c: char| c == '.' || c.is_whitespace());
    if name.is_empty() {
        "download".into()
    } else {
        name.to_owned()
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn offsets_above_four_gib_are_exact() {
        let f = encode_frame(
            "00112233445566778899aabbccddeeff",
            7_000_000_001,
            &[1, 2, 3],
        )
        .unwrap();
        let (id, n, b) = decode_frame(&f).unwrap();
        assert_eq!(id, "00112233445566778899aabbccddeeff");
        assert_eq!(n, 7_000_000_001);
        assert_eq!(b, &[1, 2, 3]);
    }
    #[test]
    fn rejects_bad_frames_and_overflow() {
        assert!(decode_frame(&[0; 23]).is_err());
        assert!(encode_frame("00112233445566778899aabbccddeeff", u64::MAX, &[1]).is_err());
        assert!(
            encode_frame("00112233445566778899aabbccddeeff", 0, &vec![0; FRAME_BYTES]).is_err()
        );
    }
    #[test]
    fn streaming_hash_matches_known_digest() {
        let mut h = IncrementalHash::default();
        h.update(b"a");
        h.update(b"bc");
        assert_eq!(
            h.digest(),
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
        assert_eq!(
            IncrementalHash::default().digest(),
            "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
        );
    }
    #[test]
    fn thai_filename_and_path_safety() {
        assert_eq!(safe_filename("ภาพวันหยุด.mp4"), "ภาพวันหยุด.mp4");
        assert_eq!(safe_filename("../bad/file"), "_bad_file");
    }
}
