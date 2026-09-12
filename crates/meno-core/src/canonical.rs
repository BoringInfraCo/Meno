use serde_json::Value;
use sha2::{Digest, Sha256};

pub fn sha256_bytes(data: &[u8]) -> [u8; 32] {
    let mut hasher = Sha256::new();
    hasher.update(data);
    hasher.finalize().into()
}

pub fn sha256_hex(data: &[u8]) -> String {
    hex::encode(sha256_bytes(data))
}

pub fn put_u32(buf: &mut Vec<u8>, value: u32) {
    buf.extend_from_slice(&value.to_be_bytes());
}

pub fn put_len_bytes(buf: &mut Vec<u8>, bytes: &[u8]) {
    put_u32(buf, bytes.len() as u32);
    buf.extend_from_slice(bytes);
}

pub fn put_tagged_bytes(buf: &mut Vec<u8>, tag: u8, bytes: &[u8]) {
    buf.push(tag);
    put_len_bytes(buf, bytes);
}

/// Compact JSON with object keys in sorted order (serde_json Map is a BTreeMap).
pub fn canonical_json(value: &Value) -> Result<Vec<u8>, serde_json::Error> {
    serde_json::to_vec(&sort_value(value))
}

fn sort_value(value: &Value) -> Value {
    match value {
        Value::Object(map) => {
            let mut out = serde_json::Map::new();
            for (k, v) in map {
                out.insert(k.clone(), sort_value(v));
            }
            Value::Object(out)
        }
        Value::Array(items) => Value::Array(items.iter().map(sort_value).collect()),
        other => other.clone(),
    }
}
