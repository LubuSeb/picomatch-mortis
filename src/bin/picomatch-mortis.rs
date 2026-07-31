use std::env;
use std::io::{self, BufRead};
use std::process::ExitCode;

use picomatch_mortis::{
    GlobOptions, GlobPattern, ScanDepth, ScanOptions, ScanState, is_match, scan,
};

fn main() -> ExitCode {
    let mut args: Vec<_> = env::args().skip(1).collect();

    if args.first().is_some_and(|argument| argument == "serve") {
        return serve();
    }

    match run_command(&mut args) {
        Ok(output) => {
            println!("{output}");
            ExitCode::SUCCESS
        }
        Err(message) => {
            eprintln!("{message}");
            ExitCode::from(2)
        }
    }
}

fn run_command(args: &mut Vec<String>) -> Result<String, String> {
    if !args.iter().any(|argument| argument == "scan") {
        return run_glob_command(args);
    }
    let options = ScanOptions {
        scan_to_end: take_flag(args, "--scan-to-end"),
        parts: take_flag(args, "--parts"),
        tokens: take_flag(args, "--tokens"),
        noext: take_flag(args, "--noext"),
        nonegate: take_flag(args, "--nonegate"),
        noparen: take_flag(args, "--noparen"),
        unescape: take_flag(args, "--unescape"),
    };

    match args.first().map(String::as_str) {
        Some("scan") if args.len() == 2 => Ok(encode_scan(&scan(&args[1], options))),
        _ => run_glob_command(args),
    }
}

fn run_glob_command(args: &mut Vec<String>) -> Result<String, String> {
    let literal_brackets = take_value(args, "--literal-brackets").map(|value| value == "true");
    let options = GlobOptions {
        windows: take_flag(args, "--windows"),
        dot: take_flag(args, "--dot"),
        nocase: take_flag(args, "--nocase"),
        contains: take_flag(args, "--contains"),
        nonegate: take_flag(args, "--nonegate"),
        noextglob: take_flag(args, "--noextglob") || take_flag(args, "--noext"),
        noglobstar: take_flag(args, "--noglobstar"),
        nobrace: take_flag(args, "--nobrace"),
        nobracket: take_flag(args, "--nobracket"),
        strict_slashes: take_flag(args, "--strict-slashes"),
        bash: take_flag(args, "--bash"),
        basename: take_flag(args, "--basename") || take_flag(args, "--match-base"),
        literal_brackets,
        keep_quotes: take_flag(args, "--keep-quotes"),
        strict_brackets: take_flag(args, "--strict-brackets"),
        regex: take_flag(args, "--regex"),
        unescape: take_flag(args, "--unescape"),
    };
    match args.first().map(String::as_str) {
        Some("is-match") if args.len() == 3 => is_match(&args[2], &args[1], options)
            .map(|matched| matched.to_string())
            .map_err(|error| error.to_string()),
        Some("source") if args.len() == 2 => GlobPattern::new(&args[1], options)
            .map(|pattern| pattern.source().to_owned())
            .map_err(|error| error.to_string()),
        Some("parse") if args.len() == 2 => GlobPattern::new(&args[1], options)
            .map(|pattern| pattern.output().to_owned())
            .map_err(|error| error.to_string()),
        _ => Err("usage: picomatch-mortis is-match PATTERN INPUT [OPTIONS]".to_owned()),
    }
}

fn serve() -> ExitCode {
    for line in io::stdin().lock().lines() {
        let line = match line {
            Ok(line) => line,
            Err(error) => {
                println!("error\t{}", encode_hex(&error.to_string()));
                continue;
            }
        };
        let decoded: Result<Vec<_>, _> = line.split('\t').map(decode_hex).collect();
        let response = match decoded {
            Ok(mut args) => run_command(&mut args),
            Err(error) => Err(error),
        };
        match response {
            Ok(value) => println!("ok\t{}", encode_hex(&value)),
            Err(error) => println!("error\t{}", encode_hex(&error)),
        }
    }
    ExitCode::SUCCESS
}

fn take_flag(args: &mut Vec<String>, flag: &str) -> bool {
    if let Some(index) = args.iter().position(|argument| argument == flag) {
        args.remove(index);
        true
    } else {
        false
    }
}

fn take_value(args: &mut Vec<String>, flag: &str) -> Option<String> {
    let index = args.iter().position(|argument| argument == flag)?;
    args.remove(index);
    (index < args.len()).then(|| args.remove(index))
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

fn decode_hex(value: &str) -> Result<String, String> {
    if value.len() % 2 != 0 {
        return Err("Invalid hex request".to_owned());
    }
    let mut bytes = Vec::with_capacity(value.len() / 2);
    for pair in value.as_bytes().chunks_exact(2) {
        let high = from_hex(pair[0]).ok_or_else(|| "Invalid hex request".to_owned())?;
        let low = from_hex(pair[1]).ok_or_else(|| "Invalid hex request".to_owned())?;
        bytes.push((high << 4) | low);
    }
    String::from_utf8(bytes).map_err(|_| "Request is not UTF-8".to_owned())
}

fn from_hex(value: u8) -> Option<u8> {
    match value {
        b'0'..=b'9' => Some(value - b'0'),
        b'a'..=b'f' => Some(value - b'a' + 10),
        b'A'..=b'F' => Some(value - b'A' + 10),
        _ => None,
    }
}
