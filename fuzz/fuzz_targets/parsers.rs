#![no_main]

//! User-typed strings reach these parsers from both the CLI and MCP tool
//! input; none of them may panic.

use clickupc_core::{
    filter::parse_field_expr,
    time::{parse_duration, parse_when},
    TaskRef,
};
use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    if data.len() > 1024 {
        return;
    }
    let text = String::from_utf8_lossy(data);
    if let Ok(task) = text.parse::<TaskRef>() {
        let _ = task.query("9001");
    }
    let _ = parse_field_expr(&text);
    let _ = parse_when(&text, Some("Asia/Jakarta"));
    let _ = parse_when(&text, None);
    let _ = parse_duration(&text);
});
