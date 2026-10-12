//! Cheap stand-in for the fuzz target: feed deterministic pseudo-random
//! (often invalid UTF-8) inputs to every user-input parser and make sure none
//! of them panic. The real fuzzing runs in CI via ClusterFuzzLite.

use clickupc_core::{
    filter::parse_field_expr,
    time::{parse_duration, parse_when},
    TaskRef,
};

/// xorshift64 — deterministic, no rand dependency.
struct Rng(u64);

impl Rng {
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0
    }
}

const ALPHABET: &[u8] =
    b"+-=<>!/:#_ 0123456789dhmswDEVtT\x00\xc3\xa9\xe2\x82\xac\xf0\x9f\x98\x80\xf6\xd9\xdf";

#[test]
fn parsers_never_panic() {
    let mut rng = Rng(0x9E37_79B9_7F4A_7C15);
    for _ in 0..50_000 {
        let len = (rng.next() % 12) as usize;
        let bytes: Vec<u8> = (0..len)
            .map(|_| ALPHABET[(rng.next() % ALPHABET.len() as u64) as usize])
            .collect();
        let text = String::from_utf8_lossy(&bytes);
        if let Ok(t) = text.parse::<TaskRef>() {
            let _ = t.query("9001");
        }
        let _ = parse_field_expr(&text);
        let _ = parse_when(&text, Some("Asia/Jakarta"));
        let _ = parse_duration(&text);
    }
}
