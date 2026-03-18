use hmac::{Hmac, Mac};
use sha2::Sha256;
use base64::{engine::general_purpose::STANDARD as B64, Engine};
use chrono::Utc;

type HmacSha256 = Hmac<Sha256>;

pub fn hmac_hex(secret: &str, msg: &str) -> String {
    let mut m = HmacSha256::new_from_slice(secret.as_bytes()).unwrap();
    m.update(msg.as_bytes());
    hex::encode(m.finalize().into_bytes())
}

pub fn hmac_b64(secret: &str, msg: &str) -> String {
    let mut m = HmacSha256::new_from_slice(secret.as_bytes()).unwrap();
    m.update(msg.as_bytes());
    B64.encode(m.finalize().into_bytes())
}

pub fn timestamp_ms()   -> u64 { Utc::now().timestamp_millis() as u64 }
pub fn timestamp_secs() -> u64 { Utc::now().timestamp() as u64 }

pub fn iso8601_now() -> String {
    Utc::now().format("%Y-%m-%dT%H:%M:%S%.3fZ").to_string()
}

pub fn build_qs(params: &[(&str, String)]) -> String {
    params.iter().map(|(k, v)| format!("{}={}", k, v)).collect::<Vec<_>>().join("&")
}

pub fn ms_to_dt(ms: u64) -> chrono::DateTime<Utc> {
    chrono::DateTime::from_timestamp_millis(ms as i64).unwrap_or_else(Utc::now)
}

pub fn secs_to_dt(s: i64) -> chrono::DateTime<Utc> {
    chrono::DateTime::from_timestamp(s, 0).unwrap_or_else(Utc::now)
}

pub fn parse_f64(s: &str) -> f64 {
    s.parse().unwrap_or(0.0)
}
