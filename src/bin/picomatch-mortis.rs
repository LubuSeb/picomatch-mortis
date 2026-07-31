use std::env;
use std::process::ExitCode;

use picomatch_mortis::{ScanDepth, ScanOptions, ScanState, scan};

fn main() -> ExitCode {
    let mut args: Vec<_> = env::args().skip(1).collect();
    let options = ScanOptions {
        scan_to_end: take_flag(&mut args, "--scan-to-end"),
        parts: take_flag(&mut args, "--parts"),
        tokens: take_flag(&mut args, "--tokens"),
        noext: take_flag(&mut args, "--noext"),
        nonegate: take_flag(&mut args, "--nonegate"),
        noparen: take_flag(&mut args, "--noparen"),
        unescape: take_flag(&mut args, "--unescape"),
    };

    match args.first().map(String::as_str) {
        Some("scan") if args.len() == 2 => {
            println!("{}", encode_scan(&scan(&args[1], options)));
            ExitCode::SUCCESS
        }
        _ => {
            eprintln!(
                "usage: picomatch-mortis scan PATTERN [--scan-to-end] [--parts] [--tokens] [--noext] [--nonegate] [--noparen] [--unescape]"
            );
            ExitCode::from(2)
        }
    }
}

fn take_flag(args: &mut Vec<String>, flag: &str) -> bool {
    if let Some(index) = args.iter().position(|argument| argument == flag) {
        args.remove(index);
        true
    } else {
        false
    }
}

fn encode_scan(state: &ScanState) -> String {
    let slashes = state
        .slashes
        .as_ref()
        .map(|values| {
            values
                .iter()
                .map(usize::to_string)
                .collect::<Vec<_>>()
                .join(",")
        })
        .unwrap_or_default();
    let parts = state
        .parts
        .as_ref()
        .map(|values| {
            values
                .iter()
                .map(|value| encode_hex(value))
                .collect::<Vec<_>>()
                .join(",")
        })
        .unwrap_or_default();
    let max_depth = match state.max_depth {
        None => String::new(),
        Some(ScanDepth::Finite(value)) => value.to_string(),
        Some(ScanDepth::Infinite) => "inf".to_owned(),
    };

    [
        encode_hex(&state.prefix),
        encode_hex(&state.input),
        state.start.to_string(),
        encode_hex(&state.base),
        encode_hex(&state.glob),
        state.is_brace.to_string(),
        state.is_bracket.to_string(),
        state.is_glob.to_string(),
        state.is_extglob.to_string(),
        state.is_globstar.to_string(),
        state.negated.to_string(),
        state.negated_extglob.to_string(),
        slashes,
        parts,
        max_depth,
    ]
    .join("\t")
}

fn encode_hex(value: &str) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut encoded = String::with_capacity(value.len() * 2);
    for byte in value.as_bytes() {
        encoded.push(HEX[(byte >> 4) as usize] as char);
        encoded.push(HEX[(byte & 0x0f) as usize] as char);
    }
    encoded
}
