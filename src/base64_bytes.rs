//! Compact JSON encoding for embedded binary payloads.
//!
//! `serde_json` represents every byte as a number, so a one-megabyte image
//! becomes several megabytes of ASCII and slows down the isolated parser
//! transfer. Base64 keeps the JSON schema simple while avoiding that blow-up.

const ALPHABET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";

pub fn serialize<S>(bytes: &[u8], serializer: S) -> Result<S::Ok, S::Error>
where
    S: serde::Serializer,
{
    serializer.serialize_str(&encode(bytes))
}

pub fn deserialize<'de, D>(deserializer: D) -> Result<Vec<u8>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    use serde::de::Error as _;
    use serde::Deserialize;
    let encoded = String::deserialize(deserializer)?;
    decode(&encoded).map_err(D::Error::custom)
}

fn encode(data: &[u8]) -> String {
    let mut result = String::with_capacity(data.len().div_ceil(3) * 4);
    for chunk in data.chunks(3) {
        let a = chunk[0];
        let b = chunk.get(1).copied().unwrap_or(0);
        let c = chunk.get(2).copied().unwrap_or(0);
        result.push(ALPHABET[(a >> 2) as usize] as char);
        result.push(ALPHABET[(((a & 3) << 4) | (b >> 4)) as usize] as char);
        result.push(if chunk.len() >= 2 {
            ALPHABET[(((b & 15) << 2) | (c >> 6)) as usize] as char
        } else {
            '='
        });
        result.push(if chunk.len() >= 3 {
            ALPHABET[(c & 63) as usize] as char
        } else {
            '='
        });
    }
    result
}

fn value(byte: u8) -> Option<u8> {
    match byte {
        b'A'..=b'Z' => Some(byte - b'A'),
        b'a'..=b'z' => Some(byte - b'a' + 26),
        b'0'..=b'9' => Some(byte - b'0' + 52),
        b'+' => Some(62),
        b'/' => Some(63),
        _ => None,
    }
}

fn decode(encoded: &str) -> Result<Vec<u8>, String> {
    let bytes = encoded.as_bytes();
    if !bytes.len().is_multiple_of(4) {
        return Err("base64 length is not divisible by four".into());
    }
    let mut decoded = Vec::with_capacity(bytes.len() / 4 * 3);
    let (chunks, remainder) = bytes.as_chunks::<4>();
    debug_assert!(remainder.is_empty());
    for (index, chunk) in chunks.iter().enumerate() {
        let last = index + 1 == chunks.len();
        let a = value(chunk[0]).ok_or("invalid base64 data")?;
        let b = value(chunk[1]).ok_or("invalid base64 data")?;
        let c = if chunk[2] == b'=' {
            None
        } else {
            Some(value(chunk[2]).ok_or("invalid base64 data")?)
        };
        let d = if chunk[3] == b'=' {
            None
        } else {
            Some(value(chunk[3]).ok_or("invalid base64 data")?)
        };
        if (!last && (c.is_none() || d.is_none())) || (c.is_none() && d.is_some()) {
            return Err("invalid base64 padding".into());
        }
        decoded.push((a << 2) | (b >> 4));
        if let Some(c) = c {
            decoded.push((b << 4) | (c >> 2));
            if let Some(d) = d {
                decoded.push((c << 6) | d);
            }
        }
    }
    Ok(decoded)
}

#[cfg(test)]
mod tests {
    use super::{decode, encode};

    #[test]
    fn binary_payload_roundtrips() {
        for bytes in [
            b"".as_slice(),
            b"f".as_slice(),
            b"fo".as_slice(),
            b"foo".as_slice(),
            b"hello world".as_slice(),
        ] {
            assert_eq!(decode(&encode(bytes)).unwrap(), bytes);
        }
    }
}
