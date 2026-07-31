#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct ScanOptions {
    pub scan_to_end: bool,
    pub parts: bool,
    pub tokens: bool,
    pub noext: bool,
    pub nonegate: bool,
    pub noparen: bool,
    pub unescape: bool,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ScanDepth {
    Finite(usize),
    Infinite,
}

impl Default for ScanDepth {
    fn default() -> Self {
        Self::Finite(0)
    }
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct ScanToken {
    pub value: String,
    pub depth: ScanDepth,
    pub is_glob: bool,
    pub is_prefix: bool,
    pub is_globstar: bool,
    pub is_brace: bool,
    pub is_bracket: bool,
    pub is_extglob: bool,
    pub negated: bool,
    pub backslashes: bool,
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct ScanState {
    pub prefix: String,
    pub input: String,
    pub start: usize,
    pub base: String,
    pub glob: String,
    pub is_brace: bool,
    pub is_bracket: bool,
    pub is_glob: bool,
    pub is_extglob: bool,
    pub is_globstar: bool,
    pub negated: bool,
    pub negated_extglob: bool,
    pub max_depth: Option<ScanDepth>,
    pub tokens: Option<Vec<ScanToken>>,
    pub slashes: Option<Vec<usize>>,
    pub parts: Option<Vec<String>>,
}

fn advance(
    bytes: &[u8],
    index: &mut isize,
    previous: &mut Option<u8>,
    current: &mut Option<u8>,
) -> Option<u8> {
    *previous = *current;
    *index += 1;
    *current = bytes.get(*index as usize).copied();
    *current
}

fn is_separator(value: Option<u8>) -> bool {
    matches!(value, Some(b'/' | b'\\'))
}

fn set_depth(token: &mut ScanToken) {
    if !token.is_prefix {
        token.depth = if token.is_globstar {
            ScanDepth::Infinite
        } else {
            ScanDepth::Finite(1)
        };
    }
}

fn add_depth(total: &mut ScanDepth, value: ScanDepth) {
    *total = match (*total, value) {
        (ScanDepth::Infinite, _) | (_, ScanDepth::Infinite) => ScanDepth::Infinite,
        (ScanDepth::Finite(left), ScanDepth::Finite(right)) => ScanDepth::Finite(left + right),
    };
}

fn remove_backslashes(input: &str) -> String {
    let mut output = String::with_capacity(input.len());
    let mut chars = input.chars().peekable();
    let mut bracket = 0usize;

    while let Some(ch) = chars.next() {
        match ch {
            '[' => {
                bracket += 1;
                output.push(ch);
            }
            ']' if bracket > 0 => {
                bracket -= 1;
                output.push(ch);
            }
            '\\' if bracket == 0 && chars.peek().is_some() => {
                let escaped = chars.next().expect("peeked character must exist");
                if escaped == '[' {
                    bracket += 1;
                }
                output.push(escaped);
            }
            _ => output.push(ch),
        }
    }

    output
}

/// Quickly separates a glob pattern into its static base and dynamic suffix.
///
/// This mirrors picomatch's public `scan()` state, including its opt-in token,
/// slash, and part details.
#[must_use]
pub fn scan(input: &str, options: ScanOptions) -> ScanState {
    let bytes = input.as_bytes();
    let last = bytes.len() as isize - 1;
    let scan_to_end = options.parts || options.scan_to_end;
    let mut slashes = Vec::new();
    let mut tokens = Vec::new();
    let mut parts = Vec::new();

    let mut text = input;
    let mut index = -1isize;
    let mut start = 0usize;
    let mut last_index = 0usize;
    let mut is_brace = false;
    let mut is_bracket = false;
    let mut is_glob = false;
    let mut is_extglob = false;
    let mut is_globstar = false;
    let mut brace_escaped = false;
    let mut backslashes = false;
    let mut negated = false;
    let mut negated_extglob = false;
    let mut finished = false;
    let mut braces = 0usize;
    let mut previous = None;
    let mut current = None;
    let mut token = ScanToken::default();

    let eos = |cursor: isize| cursor >= last;

    while index < last {
        let Some(mut code) = advance(bytes, &mut index, &mut previous, &mut current) else {
            break;
        };

        if code == b'\\' {
            backslashes = true;
            token.backslashes = true;
            let Some(next) = advance(bytes, &mut index, &mut previous, &mut current) else {
                break;
            };
            code = next;
            if code == b'{' {
                brace_escaped = true;
            }
            continue;
        }

        if brace_escaped || code == b'{' {
            braces += 1;
            while !eos(index) {
                let Some(next) = advance(bytes, &mut index, &mut previous, &mut current) else {
                    break;
                };
                code = next;

                if code == b'\\' {
                    backslashes = true;
                    token.backslashes = true;
                    advance(bytes, &mut index, &mut previous, &mut current);
                    continue;
                }

                if code == b'{' {
                    braces += 1;
                    continue;
                }

                if !brace_escaped && code == b'.' {
                    if let Some(next) = advance(bytes, &mut index, &mut previous, &mut current) {
                        code = next;
                        if code == b'.' {
                            is_brace = true;
                            token.is_brace = true;
                            is_glob = true;
                            token.is_glob = true;
                            finished = true;
                            if scan_to_end {
                                continue;
                            }
                            break;
                        }
                    }
                }

                if !brace_escaped && code == b',' {
                    is_brace = true;
                    token.is_brace = true;
                    is_glob = true;
                    token.is_glob = true;
                    finished = true;
                    if scan_to_end {
                        continue;
                    }
                    break;
                }

                if code == b'}' {
                    braces = braces.saturating_sub(1);
                    if braces == 0 {
                        brace_escaped = false;
                        is_brace = true;
                        token.is_brace = true;
                        finished = true;
                        break;
                    }
                }
            }

            if scan_to_end {
                continue;
            }
            break;
        }

        if code == b'/' {
            slashes.push(index as usize);
            tokens.push(token);
            token = ScanToken::default();

            if finished {
                continue;
            }
            if previous == Some(b'.') && index as usize == start + 1 {
                start += 2;
                continue;
            }
            last_index = index as usize + 1;
            continue;
        }

        if !options.noext {
            let is_extglob_char = matches!(code, b'+' | b'@' | b'*' | b'?' | b'!');
            let peek = bytes.get(index as usize + 1).copied();
            if is_extglob_char && peek == Some(b'(') {
                is_glob = true;
                token.is_glob = true;
                is_extglob = true;
                token.is_extglob = true;
                finished = true;
                if code == b'!' && index as usize == start {
                    negated_extglob = true;
                }

                if scan_to_end {
                    while !eos(index) {
                        let Some(next) = advance(bytes, &mut index, &mut previous, &mut current)
                        else {
                            break;
                        };
                        code = next;
                        if code == b'\\' {
                            backslashes = true;
                            token.backslashes = true;
                            advance(bytes, &mut index, &mut previous, &mut current);
                            continue;
                        }
                        if code == b')' {
                            is_glob = true;
                            token.is_glob = true;
                            finished = true;
                            break;
                        }
                    }
                    continue;
                }
                break;
            }
        }

        if code == b'*' {
            if previous == Some(b'*') {
                is_globstar = true;
                token.is_globstar = true;
            }
            is_glob = true;
            token.is_glob = true;
            finished = true;
            if scan_to_end {
                continue;
            }
            break;
        }

        if code == b'?' {
            is_glob = true;
            token.is_glob = true;
            finished = true;
            if scan_to_end {
                continue;
            }
            break;
        }

        if code == b'[' {
            while !eos(index) {
                let Some(next) = advance(bytes, &mut index, &mut previous, &mut current) else {
                    break;
                };
                if next == b'\\' {
                    backslashes = true;
                    token.backslashes = true;
                    advance(bytes, &mut index, &mut previous, &mut current);
                    continue;
                }
                if next == b']' {
                    is_bracket = true;
                    token.is_bracket = true;
                    is_glob = true;
                    token.is_glob = true;
                    finished = true;
                    break;
                }
            }
            if scan_to_end {
                continue;
            }
            break;
        }

        if !options.nonegate && code == b'!' && index as usize == start {
            negated = true;
            token.negated = true;
            start += 1;
            continue;
        }

        if !options.noparen && code == b'(' {
            is_glob = true;
            token.is_glob = true;
            if scan_to_end {
                while !eos(index) {
                    let Some(next) = advance(bytes, &mut index, &mut previous, &mut current) else {
                        break;
                    };
                    if next == b'(' {
                        backslashes = true;
                        token.backslashes = true;
                        advance(bytes, &mut index, &mut previous, &mut current);
                        continue;
                    }
                    if next == b')' {
                        finished = true;
                        break;
                    }
                }
                continue;
            }
            break;
        }

        if is_glob {
            finished = true;
            if scan_to_end {
                continue;
            }
            break;
        }
    }

    if options.noext {
        is_extglob = false;
        is_glob = false;
    }

    let mut base = text.to_owned();
    let mut prefix = String::new();
    let mut glob = String::new();

    if start > 0 {
        prefix = text[..start].to_owned();
        text = &text[start..];
        last_index = last_index.saturating_sub(start);
    }

    if !base.is_empty() && is_glob && last_index > 0 {
        base = text[..last_index].to_owned();
        glob = text[last_index..].to_owned();
    } else if is_glob {
        base.clear();
        glob = text.to_owned();
    } else {
        base = text.to_owned();
    }

    if !base.is_empty()
        && base != "/"
        && base != text
        && is_separator(base.as_bytes().last().copied())
    {
        base.pop();
    }

    if options.unescape {
        if !glob.is_empty() {
            glob = remove_backslashes(&glob);
        }
        if !base.is_empty() && backslashes {
            base = remove_backslashes(&base);
        }
    }

    let mut state = ScanState {
        prefix,
        input: input.to_owned(),
        start,
        base,
        glob,
        is_brace,
        is_bracket,
        is_glob,
        is_extglob,
        is_globstar,
        negated,
        negated_extglob,
        ..ScanState::default()
    };

    if options.tokens {
        let mut max_depth = ScanDepth::Finite(0);
        if !is_separator(current) {
            tokens.push(token);
        }

        let mut previous_index = None;
        for (part_index, &slash_index) in slashes.iter().enumerate() {
            let from = match previous_index {
                Some(value) if value > 0 => value + 1,
                _ => start,
            };
            let value = if from <= slash_index {
                &input[from..slash_index]
            } else {
                ""
            };
            if part_index == 0 && start != 0 {
                tokens[part_index].is_prefix = true;
                tokens[part_index].value.clone_from(&state.prefix);
            } else {
                tokens[part_index].value = value.to_owned();
            }
            set_depth(&mut tokens[part_index]);
            add_depth(&mut max_depth, tokens[part_index].depth);
            if part_index != 0 || !value.is_empty() {
                parts.push(value.to_owned());
            }
            previous_index = Some(slash_index);
        }

        if let Some(value) = previous_index.filter(|value| value + 1 < input.len()) {
            let part = &input[value + 1..];
            parts.push(part.to_owned());
            let last_token = tokens.last_mut().expect("a trailing part has a token");
            last_token.value = part.to_owned();
            set_depth(last_token);
            add_depth(&mut max_depth, last_token.depth);
        }

        state.max_depth = Some(max_depth);
        state.tokens = Some(tokens);
    } else if options.parts {
        let mut previous_index = None;
        for (part_index, &slash_index) in slashes.iter().enumerate() {
            let from = match previous_index {
                Some(value) if value > 0 => value + 1,
                _ => start,
            };
            let value = if from <= slash_index {
                &input[from..slash_index]
            } else {
                ""
            };
            if part_index != 0 || !value.is_empty() {
                parts.push(value.to_owned());
            }
            previous_index = Some(slash_index);
        }
        if let Some(value) = previous_index.filter(|value| value + 1 < input.len()) {
            parts.push(input[value + 1..].to_owned());
        }
    }

    if options.parts || options.tokens {
        state.slashes = Some(slashes);
        state.parts = Some(parts);
    }

    state.start = utf16_index(input, state.start);
    if let Some(slashes) = &mut state.slashes {
        for slash in slashes {
            *slash = utf16_index(input, *slash);
        }
    }

    state
}

fn utf16_index(input: &str, byte_index: usize) -> usize {
    input[..byte_index].encode_utf16().count()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn separates_static_base_from_glob() {
        let state = scan(
            "./foo/@(bar)/**/*.js",
            ScanOptions {
                parts: true,
                ..ScanOptions::default()
            },
        );
        assert_eq!(state.prefix, "./");
        assert_eq!(state.start, 2);
        assert_eq!(state.base, "foo");
        assert_eq!(state.glob, "@(bar)/**/*.js");
        assert!(state.is_extglob);
        assert!(state.is_globstar);
        assert_eq!(state.slashes, Some(vec![1, 5, 12, 15]));
        assert_eq!(
            state.parts,
            Some(vec![
                "foo".into(),
                "@(bar)".into(),
                "**".into(),
                "*.js".into()
            ])
        );
    }

    #[test]
    fn respects_negation_and_noext() {
        let negated = scan("!foo/bar/*.js", ScanOptions::default());
        assert_eq!(negated.prefix, "!");
        assert_eq!(negated.base, "foo/bar");
        assert_eq!(negated.glob, "*.js");
        assert!(negated.negated);

        let literal = scan(
            "./foo/bar/*.js",
            ScanOptions {
                noext: true,
                ..ScanOptions::default()
            },
        );
        assert_eq!(literal.base, "foo/bar/*.js");
        assert!(!literal.is_glob);
    }

    #[test]
    fn unescapes_outside_brackets_only() {
        let state = scan(
            "path/foo\\[a\\/]",
            ScanOptions {
                unescape: true,
                ..ScanOptions::default()
            },
        );
        assert_eq!(state.base, "path/foo[a\\/]");
    }

    #[test]
    fn reports_javascript_utf16_indexes() {
        let accented = scan(
            "é/*.js",
            ScanOptions {
                parts: true,
                tokens: true,
                ..ScanOptions::default()
            },
        );
        assert_eq!(accented.slashes, Some(vec![1]));

        let astral = scan(
            "😀/*.js",
            ScanOptions {
                parts: true,
                tokens: true,
                ..ScanOptions::default()
            },
        );
        assert_eq!(astral.slashes, Some(vec![2]));
    }
}
