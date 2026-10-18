//! Wire format shared with the Android app (docs/PROTOCOL.md).

use std::io::{self, Read, Write};

use serde::{Deserialize, Serialize};

pub const PROTOCOL_VERSION: u32 = 1;
/// TCP port the phone listens on (Wi‑Fi and, through `adb forward`, USB).
pub const PHONE_PORT: u16 = 47820;
/// UDP port the phone answers discovery on.
pub const DISCOVERY_PORT: u16 = 47821;
/// RFCOMM service class registered in SDP.
pub const BLUETOOTH_UUID: u128 = 0xa3c8e0f2_5b17_4d9a_8e61_0c4f2b9d7a15;
pub const ANDROID_PACKAGE: &str = "app.micnexus.android";

pub const DISCOVER_REQUEST: &[u8; 4] = b"MNX?";
pub const DISCOVER_REPLY: &[u8; 4] = b"MNX!";

pub mod kind {
    pub const HELLO: u8 = 1;
    pub const WELCOME: u8 = 2;
    pub const PENDING: u8 = 3;
    pub const AUDIO: u8 = 4;
    pub const MUTE: u8 = 5;
    pub const PING: u8 = 6;
    pub const PONG: u8 = 7;
    pub const BYE: u8 = 8;
    pub const INFO: u8 = 10;
    pub const STATS: u8 = 11;
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Transport {
    Wifi,
    Usb,
    Bluetooth,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Codec {
    Pcm16,
    Opus,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Hello {
    pub v: u32,
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub model: String,
    #[serde(default)]
    pub version: String,
    #[serde(default)]
    pub codecs: Vec<String>,
    #[serde(default)]
    pub muted: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Welcome {
    pub v: u32,
    pub id: String,
    pub name: String,
    pub version: String,
    pub codec: Codec,
    pub rate: u32,
    pub frame_ms: u32,
}

/// Discovery request and `INFO` payload (the PC introducing itself).
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PcInfo {
    pub v: u32,
    pub id: String,
    pub name: String,
    pub version: String,
    pub busy: bool,
}

/// A phone's answer to discovery.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PhoneReply {
    pub v: u32,
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub model: String,
    pub port: u16,
    /// The PC this phone wants to talk to now.
    #[serde(default)]
    pub want: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Bye {
    pub reason: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Stats {
    pub buffer_ms: u32,
    pub underruns: u32,
}

/// Codec and rate for a session (docs/PROTOCOL.md, "WELCOME").
pub fn choose_codec(transport: Transport, offered: &[String]) -> (Codec, u32, u32) {
    match transport {
        Transport::Bluetooth if offered.iter().any(|c| c == "opus") => (Codec::Opus, 48_000, 20),
        // RFCOMM rarely sustains 768 kbit/s; half the rate still covers speech.
        Transport::Bluetooth => (Codec::Pcm16, 24_000, 20),
        _ => (Codec::Pcm16, 48_000, 10),
    }
}

pub struct Frame {
    pub kind: u8,
    pub payload: Vec<u8>,
}

pub fn read_frame(r: &mut impl Read) -> io::Result<Frame> {
    let mut head = [0u8; 3];
    r.read_exact(&mut head)?;
    let len = u16::from_le_bytes([head[1], head[2]]) as usize;
    let mut payload = vec![0u8; len];
    r.read_exact(&mut payload)?;
    Ok(Frame { kind: head[0], payload })
}

pub fn encode_frame(kind: u8, payload: &[u8]) -> Vec<u8> {
    let len = payload.len().min(u16::MAX as usize);
    let mut out = Vec::with_capacity(3 + len);
    out.push(kind);
    out.extend_from_slice(&(len as u16).to_le_bytes());
    out.extend_from_slice(&payload[..len]);
    out
}

pub fn write_frame(w: &mut impl Write, kind: u8, payload: &[u8]) -> io::Result<()> {
    w.write_all(&encode_frame(kind, payload))?;
    w.flush()
}

pub fn json_frame<T: Serialize>(kind: u8, value: &T) -> Vec<u8> {
    encode_frame(kind, &serde_json::to_vec(value).unwrap_or_default())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn frames_round_trip() {
        let mut buf = Vec::new();
        write_frame(&mut buf, kind::AUDIO, &[1, 2, 3]).unwrap();
        write_frame(&mut buf, kind::MUTE, &[1]).unwrap();
        let mut r = &buf[..];
        let a = read_frame(&mut r).unwrap();
        assert_eq!((a.kind, a.payload), (kind::AUDIO, vec![1, 2, 3]));
        let b = read_frame(&mut r).unwrap();
        assert_eq!((b.kind, b.payload), (kind::MUTE, vec![1]));
        assert!(read_frame(&mut r).is_err());
    }

    #[test]
    fn codec_choice() {
        let both = vec!["opus".to_string(), "pcm16".to_string()];
        assert_eq!(choose_codec(Transport::Wifi, &both), (Codec::Pcm16, 48_000, 10));
        assert_eq!(choose_codec(Transport::Usb, &both), (Codec::Pcm16, 48_000, 10));
        assert_eq!(choose_codec(Transport::Bluetooth, &both), (Codec::Opus, 48_000, 20));
        assert_eq!(
            choose_codec(Transport::Bluetooth, &["pcm16".into()]),
            (Codec::Pcm16, 24_000, 20)
        );
    }

    #[test]
    fn hello_parses_with_missing_optional_fields() {
        let h: Hello = serde_json::from_str(r#"{"v":1,"id":"a","name":"Pixel"}"#).unwrap();
        assert_eq!(h.name, "Pixel");
        assert!(h.codecs.is_empty());
    }
}
