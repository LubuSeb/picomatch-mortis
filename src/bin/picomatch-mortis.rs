use std::collections::HashMap;
use std::env;
use std::io::{self, BufRead};
use std::process::ExitCode;

use picomatch_mortis::{
    GlobOptions, GlobPattern, ParseToken, ScanDepth, ScanOptions, ScanState, basename,
    parse_tokens, scan,
};

const MAX_CACHE_ENTRIES: usize = 256;
const MAX_CACHE_PATTERN_BYTES: usize = 2 * 1024 * 1024;

struct PatternCache {
    entries: HashMap<(String, GlobOptions), GlobPattern>,
    pattern_bytes: usize,
}

impl PatternCache {
    fn new() -> Self {
        Self {
            entries: HashMap::new(),
            pattern_bytes: 0,
        }
    }

    fn clear(&mut self) {
        self.entries.clear();
        self.pattern_bytes = 0;
    }
}

fn main() -> ExitCode {
    let mut args: Vec<_> = env::args().skip(1).collect();

    if args.first().is_some_and(|argument| argument == "serve") {
        return serve();
    }

    let mut cache = PatternCache::new();
    match run_command(&mut args, &mut cache) {
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

fn run_command(args: &mut Vec<String>, cache: &mut PatternCache) -> Result<String, String> {
    if !args.iter().any(|argument| argument == "scan") {
        return run_glob_command(args, cache);
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
        _ => run_glob_command(args, cache),
    }
}

fn run_glob_command(args: &mut Vec<String>, cache: &mut PatternCache) -> Result<String, String> {
    let literal_brackets = take_value(args, "--literal-brackets").map(|value| value == "true");
    let max_length = take_value(args, "--max-length").and_then(|value| value.parse().ok());
    let max_extglob_recursion =
        take_value(args, "--max-extglob-recursion").and_then(|value| value.parse().ok());
    let options = GlobOptions {
        windows: take_flag(args, "--windows"),
        posix: take_flag(args, "--posix"),
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
        max_length,
        max_extglob_recursion,
        unbounded_extglob_recursion: take_flag(args, "--unbounded-extglob-recursion"),
        capture: take_flag(args, "--capture"),
    };
    match args.first().map(String::as_str) {
        Some("is-match") if args.len() == 3 || args.len() == 4 => {
            let exact_match = args.len() == 4 && !options.capture && args[3] == args[1];
            cached_pattern(cache, &args[1], options)
                .map(|pattern| (exact_match || pattern.is_match(&args[2])).to_string())
        }
        Some("source") if args.len() == 2 => {
            cached_pattern(cache, &args[1], options).map(|pattern| pattern.source().to_owned())
        }
        Some("parse") if args.len() == 2 => {
            cached_pattern(cache, &args[1], options).map(|pattern| pattern.output().to_owned())
        }
        Some("tokens") if args.len() == 2 => Ok(encode_tokens(&parse_tokens(&args[1]))),
        Some("basename") if args.len() == 2 => Ok(basename(&args[1], options.windows).to_owned()),
        _ => Err("usage: picomatch-mortis is-match PATTERN INPUT [OPTIONS]".to_owned()),
    }
}

fn cached_pattern<'a>(
    cache: &'a mut PatternCache,
    pattern: &str,
    options: GlobOptions,
) -> Result<&'a GlobPattern, String> {
    let key = (pattern.to_owned(), options);
    if !cache.entries.contains_key(&key) {
        if cache.entries.len() >= MAX_CACHE_ENTRIES
            || cache.pattern_bytes.saturating_add(pattern.len()) > MAX_CACHE_PATTERN_BYTES
        {
            cache.clear();
        }
        let compiled =
            GlobPattern::new(pattern, key.1.clone()).map_err(|error| error.to_string())?;
        cache.pattern_bytes = cache.pattern_bytes.saturating_add(pattern.len());
        cache.entries.insert(key.clone(), compiled);
    }
    Ok(cache
        .entries
        .get(&key)
        .expect("compiled pattern was inserted"))
}

fn serve() -> ExitCode {
    let mut cache = PatternCache::new();
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
            Ok(mut args) => run_command(&mut args, &mut cache),
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
    let tokens = state
        .tokens
        .as_ref()
        .map(|tokens| {
            tokens
                .iter()
                .map(|token| {
                    let depth = match token.depth {
                        ScanDepth::Finite(value) => value.to_string(),
                        ScanDepth::Infinite => "inf".to_owned(),
                    };
                    format!(
                        "{}:{depth}:{}:{}:{}:{}:{}:{}:{}:{}",
                        encode_hex(&token.value),
                        token.is_glob,
                        token.is_prefix,
                        token.is_globstar,
                        token.is_brace,
                        token.is_bracket,
                        token.is_extglob,
                        token.negated,
                        token.backslashes
                    )
                })
                .collect::<Vec<_>>()
                .join(",")
        })
        .unwrap_or_default();

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
        tokens,
    ]
    .join("\t")
}

fn encode_tokens(tokens: &[ParseToken]) -> String {
    tokens
        .iter()
        .map(|token| {
            format!(
                "{}\x1f{}\x1f{}\x1f{}",
                token.kind,
                token.value,
                token.output.is_some(),
                token.output.as_deref().unwrap_or_default()
            )
        })
        .collect::<Vec<_>>()
        .join("\x1e")
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
