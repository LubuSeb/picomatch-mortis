use std::fmt;

use regress::{Flags, Regex};

const MAX_LENGTH: usize = 1024 * 64;
const MAX_NESTING: usize = 64;
const MAX_ALTERNATION_BRANCHES: usize = 1024;
const MAX_BRACKET_MARKERS: usize = 512;
const MIN_COMPILE_WORK: usize = 4096;
const COMPILE_WORK_FACTOR: usize = 64;

/// Options shared with Picomatch's matching API.
#[derive(Clone, Debug, Default, Eq, Hash, PartialEq)]
pub struct GlobOptions {
    pub windows: bool,
    pub posix: bool,
    pub dot: bool,
    pub nocase: bool,
    pub unicode: bool,
    pub unicode_sets: bool,
    pub multiline: bool,
    pub dot_all: bool,
    pub contains: bool,
    pub nonegate: bool,
    pub noextglob: bool,
    pub noglobstar: bool,
    pub nobrace: bool,
    pub nobracket: bool,
    pub strict_slashes: bool,
    pub bash: bool,
    pub basename: bool,
    pub literal_brackets: Option<bool>,
    pub keep_quotes: bool,
    pub strict_brackets: bool,
    pub regex: bool,
    pub unescape: bool,
    pub max_length: Option<usize>,
    pub max_extglob_recursion: Option<usize>,
    pub unbounded_extglob_recursion: bool,
    pub capture: bool,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct GlobError(String);

impl fmt::Display for GlobError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}

impl std::error::Error for GlobError {}

/// A compiled glob. The source is exposed so compatibility tests can audit the
/// compiler independently from matching.
pub struct GlobPattern {
    output: String,
    source: String,
    regex: Regex,
    options: GlobOptions,
    original_pattern: String,
    has_globstar: bool,
    preserve_double_slash: bool,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ParseToken {
    pub kind: &'static str,
    pub value: String,
    pub output: Option<String>,
}

/// Match and capture ranges expressed as JavaScript UTF-16 code-unit offsets.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct GlobMatch {
    pub start: usize,
    pub end: usize,
    pub captures: Vec<Option<(usize, usize)>>,
}

impl GlobPattern {
    pub fn new(pattern: &str, options: GlobOptions) -> Result<Self, GlobError> {
        if pattern.is_empty() {
            return Err(GlobError(
                "Expected pattern to be a non-empty string".to_owned(),
            ));
        }
        let max_length = options.max_length.unwrap_or(MAX_LENGTH).min(MAX_LENGTH);
        let pattern_length = pattern.encode_utf16().count();
        if pattern_length > max_length {
            return Err(GlobError(format!(
                "Input length: {pattern_length}, exceeds maximum allowed length: {max_length}",
            )));
        }

        let mut value = pattern;
        if let Some(stripped) = value.strip_prefix("./") {
            value = stripped;
        }
        let unicode_mode = options.unicode || options.unicode_sets;
        let unicode_fastpath_failure =
            unicode_mode && unicode_fastpath_rejects_raw_non_ascii(value);
        let starts_with_bang = value.starts_with('!');
        let mut negated = false;
        if !options.nonegate && value.starts_with('!') {
            let leading_negative_extglob = !options.noextglob
                && value.starts_with("!(")
                && (value.as_bytes().get(2) != Some(&b'?')
                    || !matches!(value.as_bytes().get(3), Some(b'!' | b'=' | b'<' | b':')));
            if !leading_negative_extglob {
                let mut count = 1usize;
                value = &value[1..];
                while value.starts_with('!')
                    && (value.as_bytes().get(1) != Some(&b'(')
                        || value.as_bytes().get(2) == Some(&b'?'))
                {
                    count += 1;
                    value = &value[1..];
                }
                negated = count % 2 == 1;
            }
        }
        if let Some(stripped) = value.strip_prefix("./") {
            value = stripped;
        }
        value = match value {
            "***" => "*",
            "**/**" | "**/**/**" => "**",
            _ => value,
        };
        let collapsed_globstars;
        if !options.noglobstar {
            collapsed_globstars = collapse_adjacent_globstars(value);
            if let Some(collapsed) = &collapsed_globstars {
                value = collapsed;
            }
        }
        let chars: Vec<char> = value.chars().collect();
        let capture_negative_extglob_failure = capture_negative_extglob_fails(&chars, &options);
        let unicode_sets_negated_class_failure = unicode_sets_negated_class_fails(&chars, &options);
        validate_pattern_structure(&chars, &options)?;
        if options.strict_brackets {
            validate_brackets(&chars, &options)?;
        }
        let mut compiler = Compiler::new(&options, false, chars.len());
        compiler.whole_pattern_negated = negated;
        compiler.literal_top_level_pipes = uses_literal_pipe_fastpath(&chars);
        let body = compiler.compile(&chars, true)?;
        let contains_negation_body = if negated && options.contains && value == "**" {
            strip_leading_segment_guard(&body, options.dot)
        } else {
            &body
        };
        let optional_slash = !options.strict_slashes
            && (compiler.trailing_magic
                || (!starts_with_bang && final_segment_starts_with_star_dot(&chars)))
            && (starts_with_bang || !uses_simple_fastpath_without_trailing_slash(&chars));
        let positive_source = if options.contains {
            format!("(?:{body})")
        } else if optional_slash {
            format!(r"^(?:{body}\/?)$")
        } else {
            format!("^(?:{body})$")
        };
        let source = if negated {
            if options.contains {
                format!("^(?!(?:{contains_negation_body})).*$")
            } else {
                format!("^(?!{positive_source}).*$")
            }
        } else {
            positive_source
        };
        let source = if options.windows {
            windows_regex_source(&source)
        } else {
            source
        };
        let flags = Flags {
            icase: options.nocase,
            unicode: options.unicode,
            unicode_sets: options.unicode_sets,
            multiline: options.multiline,
            dot_all: options.dot_all,
            ..Flags::default()
        };
        // In UnicodeSets mode, JavaScript rejects several class characters
        // that legacy/u regexes accept. Compile the public source once here so
        // an incompatible generated regex becomes Picomatch's dead `$^`
        // matcher instead of surfacing as an adapter-side parser decision.
        let unicode_sets_source_failure = options.unicode_sets
            && (unicode_sets_negated_class_failure || Regex::with_flags(&source, flags).is_err());
        let source = if unicode_fastpath_failure
            || capture_negative_extglob_failure
            || unicode_sets_source_failure
        {
            "$^".to_owned()
        } else {
            source
        };
        // Compile a private execution form separately so public source
        // fail-closed decisions never leak into native matching state.
        let mut match_compiler = Compiler::new(&options, true, chars.len());
        match_compiler.whole_pattern_negated = negated;
        match_compiler.literal_top_level_pipes = uses_literal_pipe_fastpath(&chars);
        let match_body = match_compiler.compile(&chars, true)?;
        let match_contains_negation_body = if negated && options.contains && value == "**" {
            strip_leading_segment_guard(&match_body, options.dot)
        } else {
            &match_body
        };
        let positive_regex_source = if options.contains {
            format!("(?:{match_body})")
        } else if optional_slash {
            format!(r"^(?:{match_body}\/?)$")
        } else {
            format!("^(?:{match_body})$")
        };
        let regex_source = if negated {
            if options.contains {
                format!("^(?!(?:{match_contains_negation_body})).*$")
            } else {
                format!("^(?!{positive_regex_source}).*$")
            }
        } else {
            positive_regex_source
        };
        let execution_source = if unicode_fastpath_failure
            || capture_negative_extglob_failure
            || unicode_sets_source_failure
        {
            "$^".to_owned()
        } else if unicode_mode {
            regex_source
        } else {
            encode_non_bmp_for_ucs2(&regex_source)
        };
        let regex = Regex::with_flags(&execution_source, flags)
            .map_err(|error| GlobError(format!("Invalid generated regex: {error}")))?;

        Ok(Self {
            output: body,
            source,
            regex,
            options,
            original_pattern: pattern.to_owned(),
            has_globstar: value.contains("**"),
            preserve_double_slash: value.starts_with("//"),
        })
    }

    pub fn source(&self) -> &str {
        &self.source
    }

    pub fn output(&self) -> &str {
        &self.output
    }

    pub fn is_match(&self, input: &str) -> Result<bool, GlobError> {
        if input.is_empty() {
            return Ok(false);
        }
        if !self.options.capture && input == self.original_pattern {
            return Ok(true);
        }
        let normalized;
        let mut value = input;
        if self.options.windows && input.contains('\\') {
            normalized = input.replace('\\', "/");
            value = &normalized;
        }
        let collapsed;
        if self.options.windows
            && self.has_globstar
            && !self.preserve_double_slash
            && value.contains("//")
        {
            collapsed = collapse_slashes(value);
            value = &collapsed;
        }
        if self.options.basename {
            value = value
                .rsplit('/')
                .find(|part| !part.is_empty())
                .unwrap_or("");
        }
        regex_matches(
            &self.regex,
            value,
            self.options.unicode || self.options.unicode_sets,
        )
    }

    /// Find the first regex match at or after a JavaScript UTF-16 code-unit
    /// offset. When `sticky` is set, only the exact starting position is
    /// attempted. This preserves `g`/`y` matcher state in the proof adapter
    /// without moving execution back into JavaScript.
    pub fn find_match_from(
        &self,
        input: &str,
        start: usize,
        sticky: bool,
    ) -> Result<Option<GlobMatch>, GlobError> {
        if input.is_empty() {
            return Ok(None);
        }
        let normalized;
        let mut value = input;
        if self.options.windows && input.contains('\\') {
            normalized = input.replace('\\', "/");
            value = &normalized;
        }
        if self.options.basename {
            value = value
                .rsplit('/')
                .find(|part| !part.is_empty())
                .unwrap_or("");
        }
        regex_find_from(
            &self.regex,
            value,
            self.options.unicode || self.options.unicode_sets,
            start,
            sticky,
        )
    }
}

fn regex_matches(regex: &Regex, value: &str, unicode: bool) -> Result<bool, GlobError> {
    let (matched, limit_exceeded) = if value.is_ascii() {
        let mut matches = regex.find_iter_ascii(value);
        let matched = matches.next().is_some();
        (matched, matches.execution_limit_exceeded())
    } else if unicode {
        let utf16: Vec<u16> = value.encode_utf16().collect();
        let mut matches = regex.find_from_utf16(&utf16, 0);
        let matched = matches.next().is_some();
        (matched, matches.execution_limit_exceeded())
    } else {
        let utf16: Vec<u16> = value.encode_utf16().collect();
        let mut matches = regex.find_from_ucs2(&utf16, 0);
        let matched = matches.next().is_some();
        (matched, matches.execution_limit_exceeded())
    };
    if limit_exceeded {
        Err(GlobError(
            "Regular expression execution exceeded the safe work limit".to_owned(),
        ))
    } else {
        Ok(matched)
    }
}

fn regex_find_from(
    regex: &Regex,
    value: &str,
    unicode: bool,
    start: usize,
    sticky: bool,
) -> Result<Option<GlobMatch>, GlobError> {
    let (matched, limit_exceeded) = if value.is_ascii() {
        let mut matches = regex.find_from_ascii(value, start);
        let matched = if sticky {
            matches.next_at_current_position()
        } else {
            matches.next()
        };
        (matched.map(glob_match), matches.execution_limit_exceeded())
    } else if unicode {
        let utf16: Vec<u16> = value.encode_utf16().collect();
        let mut matches = regex.find_from_utf16(&utf16, start);
        let matched = if sticky {
            matches.next_at_current_position()
        } else {
            matches.next()
        };
        (matched.map(glob_match), matches.execution_limit_exceeded())
    } else {
        let utf16: Vec<u16> = value.encode_utf16().collect();
        let mut matches = regex.find_from_ucs2(&utf16, start);
        let matched = if sticky {
            matches.next_at_current_position()
        } else {
            matches.next()
        };
        (matched.map(glob_match), matches.execution_limit_exceeded())
    };
    if limit_exceeded {
        Err(GlobError(
            "Regular expression execution exceeded the safe work limit".to_owned(),
        ))
    } else {
        Ok(matched)
    }
}

fn glob_match(value: regress::Match) -> GlobMatch {
    GlobMatch {
        start: value.start(),
        end: value.end(),
        captures: value
            .captures
            .into_iter()
            .map(|capture| capture.map(|range| (range.start, range.end)))
            .collect(),
    }
}

pub fn is_match(input: &str, pattern: &str, options: GlobOptions) -> Result<bool, GlobError> {
    GlobPattern::new(pattern, options)?.is_match(input)
}

pub fn basename(path: &str, windows: bool) -> &str {
    path.rsplit(|character| character == '/' || (windows && character == '\\'))
        .find(|part| !part.is_empty())
        .unwrap_or("")
}

/// Tokenize the public parse-state surface used by Picomatch consumers.
pub fn parse_tokens(pattern: &str) -> Vec<ParseToken> {
    fn flush(tokens: &mut Vec<ParseToken>, text: &mut String, seen_paren: bool, parens: usize) {
        if !text.is_empty() {
            let value = std::mem::take(text);
            tokens.push(ParseToken {
                kind: "text",
                output: (!seen_paren || parens > 0).then(|| value.clone()),
                value,
            });
        }
    }

    let chars: Vec<_> = pattern.chars().collect();
    let mut tokens = vec![ParseToken {
        kind: "bos",
        value: String::new(),
        output: Some(String::new()),
    }];
    let mut text = String::new();
    let mut parens = 0usize;
    let mut seen_paren = false;

    for &character in &chars {
        let kind = match character {
            '{' | '}' => Some("brace"),
            ',' if parens == 0 => Some("comma"),
            '(' | ')' => Some("paren"),
            '*' => Some("star"),
            _ => None,
        };
        if let Some(kind) = kind {
            flush(&mut tokens, &mut text, seen_paren, parens);
            if character == '(' {
                parens += 1;
                seen_paren = true;
            } else if character == ')' {
                parens = parens.saturating_sub(1);
            }
            tokens.push(ParseToken {
                kind,
                value: character.to_string(),
                output: (character == ')').then(|| ")".to_owned()),
            });
        } else {
            text.push(character);
        }
    }
    flush(&mut tokens, &mut text, seen_paren, parens);
    if chars.last() == Some(&'*') {
        tokens.push(ParseToken {
            kind: "maybe_slash",
            value: String::new(),
            output: Some(r"\/?".to_owned()),
        });
    }
    tokens
}

struct Compiler<'a> {
    options: &'a GlobOptions,
    trailing_magic: bool,
    exclude_slash_in_negated_classes: bool,
    inside_negative: bool,
    inside_extglob: bool,
    omit_negative_boundary: bool,
    whole_pattern_negated: bool,
    literal_top_level_pipes: bool,
    remaining_work: usize,
}

impl<'a> Compiler<'a> {
    fn new(
        options: &'a GlobOptions,
        exclude_slash_in_negated_classes: bool,
        pattern_length: usize,
    ) -> Self {
        Self {
            options,
            trailing_magic: false,
            exclude_slash_in_negated_classes,
            inside_negative: false,
            inside_extglob: false,
            omit_negative_boundary: false,
            whole_pattern_negated: false,
            literal_top_level_pipes: false,
            remaining_work: pattern_length
                .saturating_mul(COMPILE_WORK_FACTOR)
                .max(MIN_COMPILE_WORK),
        }
    }

    fn compile(&mut self, chars: &[char], mut segment_start: bool) -> Result<String, GlobError> {
        self.remaining_work = self
            .remaining_work
            .checked_sub(chars.len())
            .ok_or_else(|| {
                GlobError("Pattern compilation exceeds the safe work limit".to_owned())
            })?;
        let mut output = String::new();
        let mut index = 0;
        let mut quoted = false;
        let mut paren_depth = 0usize;
        self.trailing_magic = false;

        while index < chars.len() {
            let value = chars[index];

            if value == '\\' {
                let slash_end = chars[index..]
                    .iter()
                    .position(|character| *character != '\\')
                    .map_or(chars.len(), |offset| index + offset);
                let slash_count = slash_end - index;
                if slash_count >= 2 && slash_count % 2 == 0 {
                    output.push_str(r"\\");
                    index = slash_end;
                    segment_start = false;
                    self.trailing_magic = false;
                    continue;
                }
                if let Some(&next) = chars.get(index + 1) {
                    if self.options.unescape && next == '{' {
                        let mut end = index + 2;
                        let mut body = String::new();
                        while end < chars.len() {
                            if chars[end] == '\\' && chars.get(end + 1) == Some(&'}') {
                                break;
                            }
                            body.push(chars[end]);
                            end += 1;
                        }
                        if end + 1 < chars.len()
                            && body
                                .chars()
                                .all(|character| character.is_ascii_digit() || character == ',')
                        {
                            output.push('{');
                            output.push_str(&body);
                            output.push('}');
                            index = end + 2;
                        } else {
                            push_literal(&mut output, next);
                            index += 2;
                        }
                    } else if self.options.unescape {
                        if self.options.windows && !is_regex_syntax(next) {
                            output.push_str(r"\/?");
                        }
                        push_literal(&mut output, next);
                        index += 2;
                    } else if next.is_ascii_digit()
                        || matches!(next, 'b' | 'B' | 'd' | 'D' | 's' | 'S' | 'w' | 'W')
                    {
                        output.push('\\');
                        output.push(next);
                        index += 2;
                    } else if next == '\\' {
                        output.push_str(r"\\");
                        index += 2;
                    } else {
                        push_literal(&mut output, next);
                        index += 2;
                    }
                } else {
                    output.push_str(r"\\");
                    index += 1;
                }
                segment_start = false;
                self.trailing_magic = false;
                continue;
            }

            if value == '"' {
                quoted = !quoted;
                if self.options.keep_quotes {
                    output.push('"');
                }
                index += 1;
                continue;
            }

            if quoted {
                push_literal(&mut output, value);
                segment_start = false;
                self.trailing_magic = false;
                index += 1;
                continue;
            }

            if value == '/' {
                output.push_str(r"\/");
                segment_start = true;
                self.trailing_magic = false;
                index += 1;
                continue;
            }

            if !self.options.nobrace && value == '{' {
                if let Some(end) = find_closing(chars, index, '{', '}') {
                    let body = &chars[index + 1..end];
                    let branches = split_top_level(body, ',');
                    let range = split_top_level_sequence(body, "..");
                    let expression = if branches.len() > 1 {
                        let mut compiled = Vec::with_capacity(branches.len());
                        let previous_omit_negative_boundary = self.omit_negative_boundary;
                        self.omit_negative_boundary = true;
                        for branch in branches {
                            if branch == ['/', '*', '*'] && chars.get(end + 1) == Some(&'/') {
                                let middle = if self.options.dot {
                                    r"\/(?:(?!\.{1,2}(?:/|$))[^\/]+\/)*(?!\.{1,2}(?:/|$))[^\/]+"
                                } else {
                                    r"\/(?:(?!\.)[^\/]+\/)*(?!\.)[^\/]+"
                                };
                                compiled.push(middle.to_owned());
                            } else {
                                compiled.push(self.compile(branch, segment_start)?);
                            }
                        }
                        self.omit_negative_boundary = previous_omit_negative_boundary;
                        // Picomatch deliberately exposes brace-set expansion as
                        // a capturing group, even when `options.capture` is not
                        // enabled. Backreferences after a brace set depend on
                        // that group number.
                        format!("({})", compiled.join("|"))
                    } else if (2..=3).contains(&range.len()) {
                        compile_range(&range)?
                    } else {
                        let inner = self.compile(body, segment_start)?;
                        format!(r"\{{{inner}\}}")
                    };
                    output.push_str(&expression);
                    segment_start = false;
                    self.trailing_magic = false;
                    index = end + 1;
                    continue;
                }
            }

            if !self.options.noextglob
                && matches!(value, '?' | '*' | '+' | '@' | '!')
                && chars.get(index + 1) == Some(&'(')
                && !(value == '?' && index > 0 && chars[index - 1] == '(')
                && (value == '!' || chars.get(index + 2) != Some(&'?'))
                && !(chars.get(index + 2) == Some(&'?')
                    && matches!(chars.get(index + 3), Some(':' | '=' | '!' | '<')))
            {
                if let Some(end) = find_closing(chars, index + 1, '(', ')') {
                    let body = &chars[index + 2..end];
                    if !self.options.unbounded_extglob_recursion && matches!(value, '+' | '*') {
                        if let Some(characters) = star_only_characters(body) {
                            if segment_start {
                                output.push_str("(?=.)");
                            }
                            let repeated = if characters.len() == 1 {
                                format!("{}*", escape_regex(&characters[0].to_string()))
                            } else {
                                format!("[{}]*", escape_class_characters(&characters))
                            };
                            if self.options.capture {
                                output.push_str(&format!("({repeated})"));
                            } else {
                                output.push_str(&repeated);
                            }
                            segment_start = false;
                            self.trailing_magic = false;
                            index = end + 1;
                            continue;
                        }

                        let allowed_depth = self.options.max_extglob_recursion.unwrap_or(0);
                        let nested_depth = split_top_level(body, '|')
                            .into_iter()
                            .map(trim_char_slice)
                            .map(repeated_extglob_depth)
                            .max()
                            .unwrap_or(0);
                        if has_ambiguous_repeated_alternation(body) || nested_depth > allowed_depth
                        {
                            let literal: String = chars[index..=end].iter().collect();
                            output.push_str(&escape_regex(&literal));
                            segment_start = false;
                            self.trailing_magic = false;
                            index = end + 1;
                            continue;
                        }
                    }
                    if value == '!'
                        && !self.options.contains
                        && !self.options.capture
                        && !self.omit_negative_boundary
                        && end + 1 == chars.len()
                    {
                        if let Some((inner_depth, literal)) = nested_negative_literal(body) {
                            let total_depth = inner_depth + 1;
                            let literal = self.compile(literal, false)?;
                            if total_depth % 2 == 0 {
                                output.push_str(&literal);
                            } else {
                                output.push_str(&format!("(?!(?:{literal})(?:/|$))[^/]*"));
                            }
                            segment_start = false;
                            self.trailing_magic = false;
                            index = end + 1;
                            continue;
                        }
                    }
                    let branches = split_top_level(body, '|');
                    let branch_count = branches.len();
                    let mut compiled = Vec::with_capacity(branches.len());
                    for (branch_index, branch) in branches.into_iter().enumerate() {
                        let previous_inside_negative = self.inside_negative;
                        let previous_inside_extglob = self.inside_extglob;
                        let previous_omit_negative_boundary = self.omit_negative_boundary;
                        self.inside_negative = value == '!' || previous_inside_negative;
                        self.inside_extglob = true;
                        if value != '!' && branch_index + 1 < branch_count {
                            self.omit_negative_boundary = true;
                        }
                        let compiled_result =
                            if value == '!' && branch_index == 0 && branch.first() == Some(&'?') {
                                format!(r"\?{}", self.compile(&branch[1..], false)?)
                            } else {
                                self.compile(branch, false)?
                            };
                        self.inside_negative = previous_inside_negative;
                        self.inside_extglob = previous_inside_extglob;
                        self.omit_negative_boundary = previous_omit_negative_boundary;
                        let mut compiled_branch = compiled_result;
                        if value == '!' && end + 1 < chars.len() && branch.starts_with(&['!', '('])
                        {
                            compiled_branch = compiled_branch.replacen("(?:/|$)", "", 1);
                        }
                        compiled.push(compiled_branch);
                    }
                    let alternatives = compiled.join("|");
                    let negative_suffix = if value == '!'
                        && body.contains(&'*')
                        && chars.get(end + 1) == Some(&'.')
                        && chars.get(end + 2) != Some(&'!')
                    {
                        self.compile(&chars[end + 1..], false)?
                    } else {
                        String::new()
                    };
                    let expression = match value {
                        '@' => format!("({alternatives})"),
                        '?' => capture_expression(
                            self.options.capture,
                            &format!("(?:{alternatives})?"),
                        ),
                        '+' => capture_expression(
                            self.options.capture,
                            &format!("(?:{alternatives})+"),
                        ),
                        '*' => capture_expression(
                            self.options.capture,
                            &format!("(?:{alternatives})*"),
                        ),
                        '!' => {
                            let consume = if self.options.bash {
                                if self.options.dot {
                                    r"(?:(?:(?!(?:^|\/)\.{1,2}(?:\/|$)).)*?)"
                                } else {
                                    r"(?:(?:(?!(?:^|\/)\.).)*?)"
                                }
                            } else if body.contains(&'/') {
                                if self.options.capture { ".*?" } else { ".*" }
                            } else if self.options.capture {
                                "[^/]*?"
                            } else {
                                "[^/]*"
                            };
                            let boundary = if self.omit_negative_boundary || end + 1 < chars.len() {
                                ""
                            } else if self.options.contains || self.options.bash {
                                "$"
                            } else {
                                "(?:/|$)"
                            };
                            capture_expression(
                                self.options.capture,
                                &format!(
                                    "(?:(?!(?:{alternatives}){negative_suffix}{boundary}){consume})"
                                ),
                            )
                        }
                        _ => unreachable!(),
                    };
                    if index == 0
                        && segment_start
                        && !self.omit_negative_boundary
                        && (matches!(value, '?' | '*' | '+') || value == '!')
                    {
                        output.push_str(r"(?=.)");
                    }
                    output.push_str(&expression);
                    segment_start = false;
                    self.trailing_magic = false;
                    index = end + 1;
                    continue;
                }
            }

            if value == '*' {
                if self.options.regex && index > 0 && matches!(chars[index - 1], ']' | ')') {
                    output.push('*');
                    segment_start = false;
                    self.trailing_magic = true;
                    index += 1;
                    continue;
                }
                let mut end = index + 1;
                while chars.get(end) == Some(&'*') {
                    if !self.options.noextglob
                        && chars.get(end + 1) == Some(&'(')
                        && (end - index == 1 || self.options.bash || self.options.noglobstar)
                    {
                        break;
                    }
                    end += 1;
                }
                if self.options.noglobstar
                    && end - index == 2
                    && index == 0
                    && chars.get(end) == Some(&'/')
                    && chars.get(end + 1) == Some(&'*')
                    && (end + 2 == chars.len()
                        || (chars.get(end + 2) == Some(&'.')
                            && end + 3 < chars.len()
                            && chars[end + 3..].iter().all(|character| {
                                character.is_ascii_alphanumeric() || *character == '_'
                            })))
                {
                    let guard = self.segment_guard();
                    let wildcard = if self.options.bash { ".*?" } else { "[^/]*?" };
                    output.push_str(&format!("(?:{guard}{wildcard}\\/)?"));
                    index = end + 1;
                    segment_start = true;
                    self.trailing_magic = false;
                    continue;
                }
                let followed_by_group = (!self.options.nobrace && chars.get(end) == Some(&'{'))
                    || (!self.options.noextglob
                        && chars.get(end) == Some(&'@')
                        && chars.get(end + 1) == Some(&'('))
                    || (self.options.noextglob && chars.get(end) == Some(&'('));
                let globstar = end - index == 2
                    && !self.options.noglobstar
                    && ((segment_start
                        && (end == chars.len()
                            || chars.get(end) == Some(&'/')
                            || followed_by_group))
                        || (index > 0
                            && chars.get(index - 1) == Some(&')')
                            && (end == chars.len()
                                || chars.get(end) == Some(&'/')
                                || followed_by_group)));
                if globstar && segment_start && chars.get(end) == Some(&'/') {
                    if index == 0 {
                        let traversal = if self.options.dot {
                            r"(?:(?!(?:^|\/)\.{1,2}(?:\/|$)).)*?"
                        } else {
                            r"(?:(?!(?:^|\/)\.).)*?"
                        };
                        if chars.get(end + 1) == Some(&'*')
                            && (end + 2 == chars.len()
                                || (chars.get(end + 2) == Some(&'.')
                                    && end + 3 < chars.len()
                                    && chars[end + 3..].iter().all(|character| {
                                        character.is_ascii_alphanumeric() || *character == '_'
                                    })))
                        {
                            let guard = self.segment_guard();
                            let traversal = capture_expression(self.options.capture, traversal);
                            output.push_str(&format!(r"(?:{guard}{traversal}\/)?"));
                        } else {
                            let traversal = capture_expression(self.options.capture, traversal);
                            output.push_str(&format!(r"(?:^|\/|{traversal}\/)"));
                        }
                    } else {
                        let traversal = if self.options.dot {
                            r"(?:(?!(?:^|\/)\.{1,2}(?:\/|$)).)*?"
                        } else {
                            r"(?:(?!(?:^|\/)\.).)*?"
                        };
                        let traversal = capture_expression(self.options.capture, traversal);
                        output.truncate(output.len().saturating_sub(2));
                        output.push_str(&format!(
                            r"(?:\/{}{traversal}\/|\/|$)",
                            self.segment_guard()
                        ));
                    }
                    index = end + 1;
                    segment_start = true;
                } else if globstar
                    && index > 0
                    && chars.get(index - 1) == Some(&')')
                    && (chars.get(end) == Some(&'/') || followed_by_group)
                {
                    let traversal = if self.options.bash {
                        ".*?"
                    } else if self.options.dot {
                        r"(?:(?:(?!(?:^|\/)\.{1,2}(?:\/|$)).)*?)"
                    } else {
                        r"(?:(?:(?!(?:^|\/)\.).)*?)"
                    };
                    output.push_str(&capture_expression(self.options.capture, traversal));
                    index = end;
                    segment_start = false;
                } else if globstar && end == chars.len() {
                    let follows_extglob = index > 0 && chars.get(index - 1) == Some(&')');
                    let body = if follows_extglob && self.options.bash {
                        ".*?"
                    } else if index == 0 && self.options.dot {
                        r"(?:(?:(?!(?:^|\/)\.{1,2}(?:\/|$)).)*?)"
                    } else if index == 0 {
                        r"(?:(?:(?!(?:^|\/)\.).)*?)"
                    } else if follows_extglob && self.options.dot {
                        r"(?:(?:(?!(?:^|\/)\.{1,2}(?:\/|$)).)*?)"
                    } else if follows_extglob {
                        r"(?:(?:(?!(?:^|\/)\.).)*?)"
                    } else if self.options.bash && self.options.dot {
                        r"(?:(?:(?!(?:^|\/)\.{1,2}(?:\/|$)).)*?)"
                    } else if self.options.bash {
                        r"(?:(?:(?!(?:^|\/)\.).)*?)"
                    } else if self.options.dot {
                        r"(?:(?:(?!\.{1,2}(?:/|$))[^\/]+(?:\/|$))|\/)*"
                    } else {
                        r"(?:(?:(?!\.)[^\/]+(?:\/|$))|\/)*"
                    };
                    let body = capture_expression(self.options.capture, body);
                    if index >= 2
                        && chars.get(index - 1) == Some(&'/')
                        && chars.get(index - 2) != Some(&'*')
                        && output.ends_with(r"\/")
                        && !self.options.strict_slashes
                        && !self.inside_negative
                    {
                        output.truncate(output.len().saturating_sub(2));
                        let guard =
                            if self.options.bash && has_explicit_dot_segment(&chars[..index]) {
                                ""
                            } else {
                                self.segment_guard()
                            };
                        if self.options.contains {
                            output.push_str(&format!(r"(?:\/{guard}{body}|$)"));
                        } else {
                            output.push_str(&format!(r"(?:\/{guard}{body})?"));
                        }
                    } else {
                        if index == 0 {
                            output.push_str(self.segment_guard());
                        }
                        if self.options.contains
                            && self.options.strict_slashes
                            && index > 0
                            && chars.get(index - 1) == Some(&'/')
                        {
                            output.push_str(self.segment_guard());
                        }
                        output.push_str(&body);
                    }
                    index = end;
                    segment_start = false;
                } else if globstar {
                    let body = if self.options.dot {
                        r"(?:(?!\.{1,2}(?:/|$))[^\/]+\/)*(?!\.{1,2}(?:/|$))[^\/]*?"
                    } else {
                        r"(?:(?!\.)[^\/]+\/)*(?!\.)[^\/]*?"
                    };
                    output.push_str(&capture_expression(self.options.capture, body));
                    index = end;
                    segment_start = false;
                } else {
                    if segment_start {
                        output.push_str(self.segment_guard());
                        if end - index == 1 {
                            output.push_str("(?=.)");
                        }
                    } else if index > 0
                        && chars[index - 1] == '.'
                        && (index == 1 || chars[index - 2] == '/')
                    {
                        output.push_str(r"(?!\.{0,1}(?:/|$))");
                    }
                    let wildcard = if self.options.bash {
                        if self.whole_pattern_negated
                            || self.options.dot
                            || segment_start
                            || !self.literal_top_level_pipes
                            || (index > 0 && chars.get(index - 1) == Some(&')'))
                        {
                            ".*?"
                        } else {
                            r"(?:(?:(?!(?:^|\/)\.).)*?)"
                        }
                    } else {
                        "[^/]*?"
                    };
                    output.push_str(&capture_expression(self.options.capture, wildcard));
                    index = end;
                    segment_start = false;
                }
                self.trailing_magic = true;
                continue;
            }

            if value == '?' {
                let follows_regex_group = index > 0 && chars[index - 1] == ')';
                if follows_regex_group {
                    output.push('?');
                    segment_start = false;
                    self.trailing_magic = false;
                    index += 1;
                    continue;
                }
                if index > 0
                    && chars[index - 1] == '('
                    && matches!(chars.get(index + 1), Some('!' | '=' | '<' | ':'))
                {
                    output.push('?');
                    segment_start = false;
                    self.trailing_magic = false;
                    index += 1;
                    continue;
                }
                if index > 0 && chars[index - 1] == '(' {
                    output.push_str(r"\?");
                    segment_start = false;
                    self.trailing_magic = false;
                    index += 1;
                    continue;
                }
                if segment_start && !self.options.dot {
                    // Picomatch's simple parser fastpath spells the leading
                    // qmark as one class with an escaped slash. The escape is
                    // observable under JavaScript's UnicodeSets (`v`) mode,
                    // where an unescaped slash in this class is invalid.
                    output.push_str(r"[^.\/]");
                } else {
                    output.push_str("[^/]");
                }
                segment_start = false;
                self.trailing_magic = false;
                index += 1;
                continue;
            }

            if value == '[' && !self.options.nobracket {
                if let Some(end) = find_class_end_with_posix(chars, index, self.options.posix) {
                    let raw: String = chars[index + 1..end].iter().collect();
                    if raw == r"\[" {
                        output.push_str(r"\[");
                        segment_start = false;
                        self.trailing_magic = false;
                        index = end + 1;
                        continue;
                    }
                    if is_unknown_posix_class(&raw) {
                        output.push_str(&format!(r"[{raw}\]"));
                        segment_start = false;
                        self.trailing_magic = false;
                        index = end + 1;
                        continue;
                    }
                    let translated = translate_class(
                        &raw,
                        self.exclude_slash_in_negated_classes,
                        self.options.posix,
                    );
                    let literal = format!(r"\[{}\]", escape_regex(&raw));
                    let class = format!("[{translated}]");
                    if segment_start && has_known_posix(&raw) {
                        output.push_str("(?=.)");
                    }
                    match self.options.literal_brackets {
                        _ if has_known_posix(&raw)
                            || raw.contains('-')
                            || (self.options.posix && raw.starts_with('!')) =>
                        {
                            output.push_str(&class)
                        }
                        Some(true) => output.push_str(&literal),
                        Some(false) => output.push_str(&class),
                        None if class_has_magic(&raw) => output.push_str(&class),
                        None if self.options.capture => {
                            output.push_str(&format!("({literal}|{class})"))
                        }
                        None => output.push_str(&format!("(?:{literal}|{class})")),
                    }
                    segment_start = false;
                    self.trailing_magic = true;
                    index = end + 1;
                    continue;
                }
            }

            if value == '+'
                && (paren_depth > 0
                    || (self.inside_extglob && index > 0)
                    || (index > 0 && matches!(chars[index - 1], ']' | ')' | '}')))
            {
                output.push('+');
            } else if (!self.options.strict_brackets
                && ((value == '(' && find_closing(chars, index, '(', ')').is_none())
                    || (value == ')' && paren_depth == 0)))
                || (value == '|'
                    && self.literal_top_level_pipes
                    && paren_depth == 0
                    && !self.inside_extglob
                    && (index == 0 || chars[index - 1] != '|'))
            {
                push_literal(&mut output, value);
            } else if value == '(' || value == ')' || value == '|' {
                output.push(value);
                if value == '(' {
                    paren_depth += 1;
                } else if value == ')' {
                    paren_depth = paren_depth.saturating_sub(1);
                }
            } else {
                push_literal(&mut output, value);
            }
            segment_start = false;
            self.trailing_magic = false;
            index += 1;
        }

        Ok(output)
    }

    fn segment_guard(&self) -> &'static str {
        if self.options.dot {
            r"(?!\.{1,2}(?:/|$))"
        } else {
            r"(?!\.)"
        }
    }
}

fn push_literal(output: &mut String, value: char) {
    if matches!(
        value,
        '.' | '*' | '?' | '+' | '^' | '$' | '(' | ')' | '|' | '[' | ']' | '{' | '}'
    ) {
        output.push('\\');
    }
    output.push(value);
}

fn capture_expression(enabled: bool, expression: &str) -> String {
    if enabled {
        format!("({expression})")
    } else {
        expression.to_owned()
    }
}

fn is_regex_syntax(value: char) -> bool {
    matches!(
        value,
        '*' | '?' | '+' | '@' | '!' | '(' | ')' | '[' | ']' | '{' | '}' | '|'
    )
}

fn escape_regex(value: &str) -> String {
    let mut output = String::with_capacity(value.len());
    for character in value.chars() {
        if matches!(
            character,
            '-' | '*' | '+' | '?' | '.' | '^' | '$' | '(' | ')' | '|' | '[' | ']' | '{' | '}'
        ) {
            output.push('\\');
        }
        output.push(character);
    }
    output
}

fn star_only_characters(chars: &[char]) -> Option<Vec<char>> {
    let mut characters = Vec::new();
    let mut saw_star_extglob = false;
    for branch in split_top_level(chars, '|') {
        if branch.len() == 1 && !is_regex_syntax(branch[0]) && branch[0] != '/' {
            if !characters.contains(&branch[0]) {
                characters.push(branch[0]);
            }
            continue;
        }

        let mut index = 0usize;
        let mut found = false;
        while index < branch.len() {
            if branch.get(index) != Some(&'*') || branch.get(index + 1) != Some(&'(') {
                return None;
            }
            let end = find_closing(branch, index + 1, '(', ')')?;
            let inner = &branch[index + 2..end];
            if inner.len() != 1 || is_regex_syntax(inner[0]) || inner[0] == '/' {
                return None;
            }
            if !characters.contains(&inner[0]) {
                characters.push(inner[0]);
            }
            saw_star_extglob = true;
            found = true;
            index = end + 1;
        }
        if !found {
            return None;
        }
    }
    (!characters.is_empty() && saw_star_extglob).then_some(characters)
}

fn escape_class_characters(characters: &[char]) -> String {
    let mut output = String::new();
    for &character in characters {
        if matches!(character, '\\' | ']' | '^' | '-') {
            output.push('\\');
        }
        output.push(character);
    }
    output
}

fn has_ambiguous_repeated_alternation(chars: &[char]) -> bool {
    let branches = split_top_level(chars, '|');
    if branches.len() < 2 {
        return false;
    }

    let mut values = Vec::new();
    for branch in branches {
        let branch = trim_char_slice(branch);
        if branch.is_empty()
            || branch
                .iter()
                .all(|character| matches!(character, '*' | '?'))
        {
            return true;
        }

        let Some(value) = normalize_simple_branch(branch) else {
            continue;
        };
        if value.is_empty() {
            continue;
        }
        values.push(
            value
                .iter()
                .collect::<String>()
                .encode_utf16()
                .collect::<Vec<_>>(),
        );
    }

    for left in 0..values.len() {
        for right in left + 1..values.len() {
            let a = &values[left];
            let b = &values[right];
            let Some(&character) = a.first() else {
                continue;
            };
            let a_repeats = a.iter().all(|value| *value == character);
            let b_repeats = b.iter().all(|value| *value == character);
            if a_repeats
                && b_repeats
                && (a == b || a.starts_with(b.as_slice()) || b.starts_with(a.as_slice()))
            {
                return true;
            }
        }
    }
    false
}

fn trim_char_slice(mut chars: &[char]) -> &[char] {
    while chars
        .first()
        .is_some_and(|character| character.is_whitespace())
    {
        chars = &chars[1..];
    }
    while chars
        .last()
        .is_some_and(|character| character.is_whitespace())
    {
        chars = &chars[..chars.len() - 1];
    }
    chars
}

fn normalize_simple_branch(mut branch: &[char]) -> Option<Vec<char>> {
    loop {
        branch = trim_char_slice(branch);
        if !branch.starts_with(&['@', '('])
            || find_closing(branch, 1, '(', ')') != Some(branch.len().saturating_sub(1))
        {
            break;
        }
        let inner = &branch[2..branch.len() - 1];
        if inner
            .iter()
            .any(|character| matches!(character, '\\' | '(' | ')' | '[' | ']' | '{' | '}' | '|'))
        {
            break;
        }
        branch = inner;
    }

    let mut value = Vec::with_capacity(branch.len());
    let mut escaped = false;
    for &character in branch {
        if escaped {
            value.push(character);
            escaped = false;
            continue;
        }
        if character == '\\' {
            escaped = true;
            continue;
        }
        if matches!(
            character,
            '?' | '*' | '+' | '@' | '!' | '(' | ')' | '[' | ']' | '{' | '}'
        ) {
            return None;
        }
        value.push(character);
    }
    if escaped {
        value.push('\\');
    }
    Some(value)
}

fn repeated_extglob_depth(chars: &[char]) -> usize {
    if chars.len() >= 3
        && matches!(chars[0], '+' | '*')
        && chars[1] == '('
        && find_closing(chars, 1, '(', ')') == Some(chars.len() - 1)
    {
        1 + repeated_extglob_depth(&chars[2..chars.len() - 1])
    } else {
        0
    }
}

fn nested_negative_literal(mut chars: &[char]) -> Option<(usize, &[char])> {
    let mut depth = 0usize;
    while chars.starts_with(&['!', '(']) {
        let end = find_closing(chars, 1, '(', ')')?;
        if end + 1 != chars.len() {
            return None;
        }
        depth += 1;
        chars = &chars[2..end];
    }
    if depth == 0
        || chars.is_empty()
        || chars.iter().any(|character| {
            matches!(
                character,
                '*' | '?' | '[' | ']' | '{' | '}' | '(' | ')' | '|'
            )
        })
    {
        return None;
    }
    Some((depth, chars))
}

fn find_closing(chars: &[char], start: usize, open: char, close: char) -> Option<usize> {
    let mut depth = 0;
    let mut escaped = false;
    let mut bracket = 0;
    for (index, &character) in chars.iter().enumerate().skip(start) {
        if escaped {
            escaped = false;
            continue;
        }
        if character == '\\' {
            escaped = true;
            continue;
        }
        if character == '[' && bracket == 0 && find_class_end(chars, index).is_some() {
            bracket += 1;
        } else if character == ']' && bracket > 0 {
            bracket -= 1;
        } else if bracket == 0 && character == open {
            depth += 1;
        } else if bracket == 0 && character == close {
            depth -= 1;
            if depth == 0 {
                return Some(index);
            }
        }
    }
    None
}

fn find_class_end(chars: &[char], start: usize) -> Option<usize> {
    find_class_end_with_posix(chars, start, false)
}

fn find_class_end_with_posix(chars: &[char], start: usize, posix_negation: bool) -> Option<usize> {
    let mut index = start + 1;
    if chars.get(index) == Some(&'^') || (posix_negation && chars.get(index) == Some(&'!')) {
        index += 1;
    }
    if chars.get(index) == Some(&']') {
        index += 1;
    }
    let mut escaped = false;
    let mut posix_end = None;
    while index < chars.len() {
        if !escaped && chars[index] == '[' && chars.get(index + 1) == Some(&':') {
            let mut probe = index + 2;
            let mut found = None;
            while probe + 1 < chars.len() {
                if chars[probe] == ':' && chars[probe + 1] == ']' {
                    found = Some(probe + 1);
                    break;
                }
                probe += 1;
            }
            if let Some(end) = found {
                posix_end = Some(end);
                index = end + 1;
                continue;
            }
        }
        match chars[index] {
            _ if escaped => escaped = false,
            '\\' => escaped = true,
            ']' => return Some(index),
            _ => {}
        }
        index += 1;
    }
    posix_end
}

fn split_top_level(chars: &[char], separator: char) -> Vec<&[char]> {
    let mut result = Vec::new();
    let mut start = 0;
    let mut parens = 0;
    let mut braces = 0;
    let mut brackets = 0;
    let mut escaped = false;
    for (index, &character) in chars.iter().enumerate() {
        if escaped {
            escaped = false;
            continue;
        }
        if character == '\\' {
            escaped = true;
            continue;
        }
        match character {
            '[' => brackets += 1,
            ']' if brackets > 0 => brackets -= 1,
            '(' if brackets == 0 => parens += 1,
            ')' if brackets == 0 && parens > 0 => parens -= 1,
            '{' if brackets == 0 => braces += 1,
            '}' if brackets == 0 && braces > 0 => braces -= 1,
            _ => {}
        }
        if character == separator && parens == 0 && braces == 0 && brackets == 0 {
            result.push(&chars[start..index]);
            start = index + 1;
        }
    }
    result.push(&chars[start..]);
    result
}

fn split_top_level_sequence<'a>(chars: &'a [char], separator: &str) -> Vec<&'a [char]> {
    if separator != ".." {
        return vec![chars];
    }
    let mut result = Vec::new();
    let mut start = 0;
    let mut index = 0;
    while index + 1 < chars.len() {
        if chars[index] == '.' && chars[index + 1] == '.' {
            result.push(&chars[start..index]);
            start = index + 2;
            index += 2;
        } else {
            index += 1;
        }
    }
    result.push(&chars[start..]);
    result
}

fn compile_range(parts: &[&[char]]) -> Result<String, GlobError> {
    let mut values: Vec<String> = parts.iter().map(|part| part.iter().collect()).collect();
    values.sort();
    let candidate = format!("[{}]", values.join("-"));
    if Regex::new(&candidate).is_ok() {
        return Ok(candidate);
    }
    Ok(values
        .iter()
        .map(|value| escape_regex(value))
        .collect::<Vec<_>>()
        .join(".."))
}

fn translate_class(raw: &str, exclude_slash: bool, posix: bool) -> String {
    let mut output = String::with_capacity(raw.len());
    let chars: Vec<_> = raw.chars().collect();
    let first_content =
        usize::from(chars.first() == Some(&'^') || (posix && chars.first() == Some(&'!')));
    for (index, character) in chars.iter().enumerate() {
        if (*character == '[' && chars.get(index + 1) != Some(&':'))
            || (*character == ']' && index == first_content)
        {
            output.push('\\');
        }
        output.push(*character);
    }
    if posix && output.starts_with('!') {
        output.replace_range(..1, "^");
    }
    if exclude_slash && output.starts_with('^') && !output.contains('/') {
        output.push('/');
    }
    for (name, value) in [
        ("alnum", "a-zA-Z0-9"),
        ("alpha", "a-zA-Z"),
        ("ascii", r"\x00-\x7F"),
        ("blank", r" \t"),
        ("cntrl", r"\x00-\x1F\x7F"),
        ("digit", "0-9"),
        ("graph", r"\x21-\x7E"),
        ("lower", "a-z"),
        ("print", r"\x20-\x7E "),
        ("punct", r##"\-!"#$%&'()\*+,./:;<=>?@[\]^_`{|}~"##),
        ("space", r" \t\r\n\v\f"),
        ("upper", "A-Z"),
        ("word", "A-Za-z0-9_"),
        ("xdigit", "A-Fa-f0-9"),
    ] {
        output = output.replace(&format!("[:{name}:]"), value);
    }
    output
}

fn has_known_posix(raw: &str) -> bool {
    [
        "alnum", "alpha", "ascii", "blank", "cntrl", "digit", "graph", "lower", "print", "punct",
        "space", "upper", "word", "xdigit",
    ]
    .iter()
    .any(|name| raw.contains(&format!("[:{name}:]")))
}

fn is_unknown_posix_class(raw: &str) -> bool {
    raw.strip_prefix("[:")
        .and_then(|value| value.strip_suffix(":]"))
        .is_some_and(|name| {
            !name.contains("][:")
                && ![
                    "alnum", "alpha", "ascii", "blank", "cntrl", "digit", "graph", "lower",
                    "print", "punct", "space", "upper", "word", "xdigit",
                ]
                .contains(&name)
        })
}

fn class_has_magic(raw: &str) -> bool {
    raw.starts_with('!') || raw.starts_with('^') || raw.contains('-') || raw.contains("[:")
}

fn collapse_slashes(value: &str) -> String {
    let mut output = String::with_capacity(value.len());
    let mut previous_slash = false;
    for character in value.chars() {
        if character == '/' {
            if !previous_slash {
                output.push(character);
            }
            previous_slash = true;
        } else {
            output.push(character);
            previous_slash = false;
        }
    }
    output
}

fn collapse_adjacent_globstars(value: &str) -> Option<String> {
    let mut segments = Vec::new();
    let mut changed = false;
    for segment in value.split('/') {
        if segment == "**" && segments.last() == Some(&"**") {
            changed = true;
            continue;
        }
        segments.push(segment);
    }
    changed.then(|| segments.join("/"))
}

fn has_explicit_dot_segment(chars: &[char]) -> bool {
    chars.iter().enumerate().any(|(index, character)| {
        *character == '.' && (index == 0 || chars.get(index - 1) == Some(&'/'))
    })
}

fn validate_pattern_structure(chars: &[char], options: &GlobOptions) -> Result<(), GlobError> {
    #[derive(Clone, Copy)]
    enum Frame {
        Paren(usize),
        Brace(usize),
    }

    let mut frames = Vec::new();
    let mut escaped = false;
    let mut quoted = false;
    let mut unmatched_bracket_markers = 0usize;
    let mut class_search_exhausted = false;
    let mut index = 0usize;

    while index < chars.len() {
        let character = chars[index];
        if escaped {
            escaped = false;
            index += 1;
            continue;
        }
        if character == '\\' {
            escaped = true;
            index += 1;
            continue;
        }
        if character == '"' {
            quoted = !quoted;
            index += 1;
            continue;
        }
        if quoted {
            index += 1;
            continue;
        }
        if !options.nobracket && character == '[' {
            if !class_search_exhausted {
                if let Some(end) = find_class_end_with_posix(chars, index, options.posix) {
                    index = end + 1;
                    continue;
                }
                class_search_exhausted = true;
            }
            unmatched_bracket_markers += 1;
            if unmatched_bracket_markers > MAX_BRACKET_MARKERS {
                return Err(GlobError(format!(
                    "Pattern contains more than {MAX_BRACKET_MARKERS} unmatched bracket markers"
                )));
            }
        }

        match character {
            '(' => frames.push(Frame::Paren(1)),
            ')' if matches!(frames.last(), Some(Frame::Paren(_))) => {
                frames.pop();
            }
            '{' if !options.nobrace => frames.push(Frame::Brace(1)),
            '}' if !options.nobrace && matches!(frames.last(), Some(Frame::Brace(_))) => {
                frames.pop();
            }
            '|' => {
                if let Some(Frame::Paren(branches)) = frames.last_mut() {
                    *branches = branches.saturating_add(1);
                    if *branches > MAX_ALTERNATION_BRANCHES {
                        return Err(GlobError(format!(
                            "Pattern contains more than {MAX_ALTERNATION_BRANCHES} alternation branches"
                        )));
                    }
                }
            }
            ',' => {
                if let Some(Frame::Brace(branches)) = frames.last_mut() {
                    *branches = branches.saturating_add(1);
                    if *branches > MAX_ALTERNATION_BRANCHES {
                        return Err(GlobError(format!(
                            "Pattern contains more than {MAX_ALTERNATION_BRANCHES} alternation branches"
                        )));
                    }
                }
            }
            _ => {}
        }
        if frames.len() > MAX_NESTING {
            return Err(GlobError(format!(
                "Pattern nesting exceeds the safe limit of {MAX_NESTING}"
            )));
        }
        index += 1;
    }

    Ok(())
}

fn uses_simple_fastpath_without_trailing_slash(chars: &[char]) -> bool {
    !chars.is_empty()
        && !matches!(chars[0], '*' | '.')
        && chars.iter().all(|character| {
            !matches!(
                character,
                '/' | '\\' | '[' | ']' | '{' | '}' | '(' | ')' | '|' | '+' | '@' | '!'
            )
        })
}

fn uses_literal_pipe_fastpath(chars: &[char]) -> bool {
    !matches!(chars.first(), Some('*' | '!'))
        && !chars
            .iter()
            .any(|character| matches!(character, '/' | '(' | ')' | '[' | ']' | '{' | '}' | '"'))
}

fn unicode_fastpath_rejects_raw_non_ascii(pattern: &str) -> bool {
    !pattern.is_empty()
        && !matches!(pattern.chars().next(), Some('*' | '!'))
        && !pattern.chars().any(|character| {
            matches!(
                character,
                '/' | '(' | ')' | '[' | ']' | '{' | '}' | '"' | '\\'
            )
        })
        && !pattern.is_ascii()
}

fn unicode_sets_negated_class_fails(chars: &[char], options: &GlobOptions) -> bool {
    if !options.unicode_sets || options.nobracket {
        return false;
    }
    let mut index = 0usize;
    let mut quoted = false;
    while index < chars.len() {
        match chars[index] {
            '\\' => {
                index = (index + 2).min(chars.len());
                continue;
            }
            '"' => quoted = !quoted,
            '[' if !quoted => {
                let first = chars.get(index + 1);
                if first == Some(&'^') || (options.posix && first == Some(&'!')) {
                    // Picomatch injects an unescaped `/` into negated classes.
                    // That generated class is invalid with JavaScript's `v`
                    // flag, so `toRegex` falls back to its dead matcher.
                    return true;
                }
                if let Some(end) = find_class_end(chars, index) {
                    index = end;
                }
            }
            _ => {}
        }
        index += 1;
    }
    false
}

fn final_segment_starts_with_star_dot(chars: &[char]) -> bool {
    let start = chars
        .iter()
        .rposition(|character| *character == '/')
        .map_or(0, |index| index + 1);
    let fastpath_prefix = start == 0 || chars.get(..start) == Some(&['*', '*', '/']);
    fastpath_prefix && chars.get(start) == Some(&'*') && chars.get(start + 1) == Some(&'.')
}

fn capture_negative_extglob_fails(chars: &[char], options: &GlobOptions) -> bool {
    if !options.capture || options.noextglob {
        return false;
    }

    let mut index = 0usize;
    let mut quoted = false;
    while index < chars.len() {
        match chars[index] {
            '\\' => {
                index = (index + 2).min(chars.len());
                continue;
            }
            '"' => {
                quoted = !quoted;
                index += 1;
                continue;
            }
            '[' if !quoted => {
                if let Some(end) = find_class_end(chars, index) {
                    index = end + 1;
                    continue;
                }
            }
            '!' if !quoted && chars.get(index + 1) == Some(&'(') => {
                if let Some(end) = find_closing(chars, index + 1, '(', ')') {
                    let body = &chars[index + 2..end];
                    let remaining = &chars[end + 1..];
                    let terminal =
                        remaining.is_empty() || remaining.iter().all(|character| *character == ')');
                    let special_dot_suffix = body.contains(&'*')
                        && remaining.len() >= 2
                        && remaining[0] == '.'
                        && remaining[1..]
                            .iter()
                            .all(|character| !matches!(character, '\\' | '/' | '.'));
                    if terminal || special_dot_suffix {
                        return true;
                    }
                }
            }
            _ => {}
        }
        index += 1;
    }
    false
}

fn strip_leading_segment_guard(body: &str, dot: bool) -> &str {
    let guard = if dot {
        r"(?!\.{1,2}(?:/|$))"
    } else {
        r"(?!\.)"
    };
    body.strip_prefix(guard).unwrap_or(body)
}

fn windows_regex_source(source: &str) -> String {
    let chars: Vec<char> = source.chars().collect();
    let mut output = String::with_capacity(source.len());
    let mut index = 0usize;
    let mut class_depth = 0usize;
    while index < chars.len() {
        if chars[index..].starts_with(&['[', '^', '.', '\\', '/', ']']) {
            output.push_str(r"[^.\\/]");
            index += 6;
        } else if chars[index..].starts_with(&['[', '^', '.', '/', ']']) {
            output.push_str(r"[^.\\/]");
            index += 5;
        } else if chars[index..].starts_with(&['[', '^', '\\', '/', ']']) {
            output.push_str(r"[^\\/]");
            index += 5;
        } else if chars[index..].starts_with(&['[', '^', '/', ']']) {
            output.push_str(r"[^\\/]");
            index += 4;
        } else if chars[index] == '\\' && chars.get(index + 1) == Some(&'/') {
            if class_depth > 0 {
                output.push_str(r"\/");
            } else {
                output.push_str(r"[\\/]");
            }
            index += 2;
        } else if chars[index] == '/' {
            if class_depth > 0 {
                output.push('/');
            } else {
                output.push_str(r"[\\/]");
            }
            index += 1;
        } else if chars[index] == '\\' {
            output.push(chars[index]);
            if let Some(&escaped) = chars.get(index + 1) {
                output.push(escaped);
                index += 2;
            } else {
                index += 1;
            }
        } else if chars[index] == '[' {
            class_depth += 1;
            output.push(chars[index]);
            index += 1;
        } else if chars[index] == ']' {
            class_depth = class_depth.saturating_sub(1);
            output.push(chars[index]);
            index += 1;
        } else {
            output.push(chars[index]);
            index += 1;
        }
    }
    output
}

fn encode_non_bmp_for_ucs2(source: &str) -> String {
    let mut output = String::with_capacity(source.len());
    for character in source.chars() {
        let code = character as u32;
        if code <= 0xffff {
            output.push(character);
        } else {
            let scalar = code - 0x1_0000;
            let high = 0xd800 + (scalar >> 10);
            let low = 0xdc00 + (scalar & 0x3ff);
            // Keep the two JavaScript code units separate. Regress otherwise
            // folds adjacent surrogate escapes back into one Unicode scalar,
            // while non-`u` JavaScript regexes observe two UTF-16 units.
            output.push_str(&format!(r"\u{high:04x}(?:)\u{low:04x}"));
        }
    }
    output
}

fn validate_brackets(chars: &[char], options: &GlobOptions) -> Result<(), GlobError> {
    let mut parens = 0usize;
    let mut braces = 0usize;
    let mut bracket_content: Option<(bool, bool)> = None;
    let mut escaped = false;
    let mut quoted = false;
    let mut index = 0usize;
    while index < chars.len() {
        let character = chars[index];

        if let Some((empty, only_caret)) = bracket_content.as_mut() {
            if character == '\\' {
                *empty = false;
                *only_caret = false;
                index = (index + 2).min(chars.len());
                continue;
            }
            if character == '[' {
                if let Some(end) = known_posix_class_end(chars, index) {
                    *empty = false;
                    *only_caret = false;
                    index = end + 1;
                    continue;
                }
            }
            if character == ']' {
                if *empty || *only_caret {
                    *empty = false;
                    *only_caret = false;
                } else {
                    bracket_content = None;
                }
                index += 1;
                continue;
            }
            *only_caret = *empty && (character == '^' || (options.posix && character == '!'));
            *empty = false;
            index += 1;
            continue;
        }

        if escaped {
            escaped = false;
            index += 1;
            continue;
        }
        if character == '\\' {
            escaped = true;
            index += 1;
            continue;
        }
        if character == '"' {
            quoted = !quoted;
            index += 1;
            continue;
        }
        if quoted {
            index += 1;
            continue;
        }
        if !options.nobracket && character == '[' {
            bracket_content = Some((true, false));
            index += 1;
            continue;
        }
        match character {
            ']' if !options.nobracket => {
                return Err(GlobError(
                    "Missing opening: \"[\" - use \"\\\\[\" to match literal characters".to_owned(),
                ));
            }
            '(' => parens += 1,
            ')' if parens == 0 => {
                return Err(GlobError(
                    "Missing opening: \"(\" - use \"\\\\(\" to match literal characters".to_owned(),
                ));
            }
            ')' => parens -= 1,
            '{' if !options.nobrace => braces += 1,
            '}' if !options.nobrace && braces > 0 => braces -= 1,
            _ => {}
        }
        index += 1;
    }
    if bracket_content.is_some() {
        return Err(GlobError(
            "Missing closing: \"]\" - use \"\\\\]\" to match literal characters".to_owned(),
        ));
    }
    if parens > 0 {
        return Err(GlobError(
            "Missing closing: \")\" - use \"\\\\)\" to match literal characters".to_owned(),
        ));
    }
    if braces > 0 {
        return Err(GlobError(
            "Missing closing: \"}\" - use \"\\\\}\" to match literal characters".to_owned(),
        ));
    }
    Ok(())
}

fn known_posix_class_end(chars: &[char], start: usize) -> Option<usize> {
    if chars.get(start) != Some(&'[') || chars.get(start + 1) != Some(&':') {
        return None;
    }
    let mut end = start + 2;
    let limit = (start + 2 + 6).min(chars.len());
    while end < limit && end + 1 < chars.len() {
        if chars[end] == ':' && chars[end + 1] == ']' {
            let name: String = chars[start + 2..end].iter().collect();
            return [
                "alnum", "alpha", "ascii", "blank", "cntrl", "digit", "graph", "lower", "print",
                "punct", "space", "upper", "word", "xdigit",
            ]
            .contains(&name.as_str())
            .then_some(end + 1);
        }
        end += 1;
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    fn matches(input: &str, pattern: &str) -> bool {
        is_match(input, pattern, GlobOptions::default()).unwrap()
    }

    #[test]
    fn matches_literals_stars_and_qmarks() {
        assert!(matches("a/b/c.md", "a/?/*.md"));
        assert!(!matches("a/bb/c.md", "a/?/*.md"));
        assert!(matches("abc", "a*c"));
        assert!(!matches("a/b/c", "a*c"));
    }

    #[test]
    fn globstars_cross_segments() {
        assert!(matches("a/b/c/d.txt", "a/**/d.txt"));
        assert!(matches("a/d.txt", "a/**/d.txt"));
        assert!(!matches("a/.hidden/d.txt", "a/**/d.txt"));
    }

    #[test]
    fn supports_braces_classes_and_extglobs() {
        assert!(matches("foo.rs", "*.{js,rs}"));
        assert!(matches("file3", "file[0-9]"));
        assert!(matches("foo.js", "@(foo|bar).js"));
        assert!(!matches("baz.js", "@(foo|bar).js"));
        assert!(matches("ab/b/_", "@(a|b)**"));
        assert!(matches("a/", "a/!(b)"));
        assert!(
            is_match(
                "a",
                "[a-c]",
                GlobOptions {
                    literal_brackets: Some(true),
                    ..GlobOptions::default()
                }
            )
            .unwrap()
        );
    }

    #[test]
    fn preserves_generated_captures_and_backreference_numbering() {
        let capture = GlobOptions {
            capture: true,
            ..GlobOptions::default()
        };

        assert_eq!(
            GlobPattern::new("{a*,b?}", capture.clone())
                .unwrap()
                .source(),
            r"^(?:(a([^/]*?)|b[^/]))$"
        );
        assert_eq!(
            GlobPattern::new("+(a|b)/\\1", capture.clone())
                .unwrap()
                .source(),
            r"^(?:(?=.)((?:a|b)+)\/\1)$"
        );
        assert_eq!(
            GlobPattern::new("!(a)/(*)/\\1", capture.clone())
                .unwrap()
                .source(),
            r"^(?:(?=.)((?:(?!(?:a))[^/]*?))\/(([^/]*?))\/\1)$"
        );

        assert!(is_match("a/a", "{a,b}/\\1", GlobOptions::default()).unwrap());
        assert!(is_match("x/a/x", "*/(*)/\\1", capture.clone()).unwrap());
        assert!(!is_match("x/a/a", "*/(*)/\\1", capture.clone()).unwrap());
        assert!(is_match("x/y/z/x/y", "**/(*)/\\1", capture.clone()).unwrap());
        assert!(is_match("a/x/a", "[ab]/(*)/\\1", capture.clone()).unwrap());
        assert!(!is_match("a/x/x", "[ab]/(*)/\\1", capture.clone()).unwrap());
        assert!(
            is_match(
                "a/x/x",
                "[ab]/(*)/\\1",
                GlobOptions {
                    capture: true,
                    literal_brackets: Some(false),
                    ..GlobOptions::default()
                }
            )
            .unwrap()
        );
        assert!(is_match("aba/aba", "+(a|b)/\\1", capture.clone()).unwrap());
        assert!(is_match("c/x/c", "!(a)/(*)/\\1", capture).unwrap());

        let windows = GlobPattern::new(
            "a/**",
            GlobOptions {
                windows: true,
                capture: true,
                ..GlobOptions::default()
            },
        )
        .unwrap();
        assert_eq!(
            windows.find_match_from("a//b", 0, false).unwrap(),
            Some(GlobMatch {
                start: 0,
                end: 4,
                captures: vec![Some((2, 4))],
            })
        );

        let repeated_separators = GlobPattern::new(
            "a/**/*",
            GlobOptions {
                capture: true,
                ..GlobOptions::default()
            },
        )
        .unwrap();
        assert_eq!(
            repeated_separators
                .find_match_from("a//b", 0, false)
                .unwrap(),
            Some(GlobMatch {
                start: 0,
                end: 4,
                captures: vec![Some((2, 2)), Some((3, 4))],
            })
        );
        assert_eq!(
            repeated_separators
                .find_match_from("a///b", 0, false)
                .unwrap(),
            Some(GlobMatch {
                start: 0,
                end: 5,
                captures: vec![Some((2, 3)), Some((4, 5))],
            })
        );
    }

    #[test]
    fn star_dot_fastpath_does_not_optionalize_nested_trailing_slashes() {
        let pattern = GlobPattern::new("a/*.js", GlobOptions::default()).unwrap();
        assert!(pattern.is_match("a/b.js").unwrap());
        assert!(!pattern.is_match("a/b.js/").unwrap());
        assert!(
            GlobPattern::new("*.js", GlobOptions::default())
                .unwrap()
                .is_match("b.js/")
                .unwrap()
        );
        assert!(
            GlobPattern::new("**/*.js", GlobOptions::default())
                .unwrap()
                .is_match("a/b.js/")
                .unwrap()
        );
    }

    #[test]
    fn capture_mode_fail_closes_only_malformed_negative_extglob_forms() {
        let capture = GlobOptions {
            capture: true,
            ..GlobOptions::default()
        };

        for pattern in ["!(a|b)", "x/!(a|b)", "+(!(a))", "a!(*b).js"] {
            let compiled = GlobPattern::new(pattern, capture.clone()).unwrap();
            assert_eq!(compiled.source(), "$^", "{pattern}");
            assert!(!compiled.is_match(pattern).unwrap(), "{pattern}");
        }

        let nonterminal = GlobPattern::new("a!(b)c", capture.clone()).unwrap();
        assert_ne!(nonterminal.source(), "$^");
        assert!(nonterminal.is_match("aac").unwrap());

        let alternative = GlobPattern::new("+(!(a)|b)", capture.clone()).unwrap();
        assert_ne!(alternative.source(), "$^");
        assert!(alternative.is_match("b").unwrap());

        let brace = GlobPattern::new("{!(a),b}", capture).unwrap();
        assert_ne!(brace.source(), "$^");
        assert!(brace.is_match("b").unwrap());
    }

    #[test]
    fn nested_negative_extglobs_preserve_alternative_context() {
        for capture in [false, true] {
            let options = GlobOptions {
                capture,
                ..GlobOptions::default()
            };
            for pattern in ["@(!(!(a))|b)", "{!(!(a)),b}"] {
                let compiled = GlobPattern::new(pattern, options.clone()).unwrap();
                for input in ["a", "ab", "abc", "b"] {
                    assert!(
                        compiled.is_match(input).unwrap(),
                        "{input:?} vs {pattern:?}"
                    );
                }
                assert!(!compiled.is_match("z").unwrap(), "z vs {pattern:?}");
            }
        }

        for (input, pattern) in [
            ("ab.js", "!(!(a)).js"),
            ("ab/x", "!(!(a))/x"),
            ("xaby", "x!(!(a))y"),
        ] {
            assert!(matches(input, pattern), "{input:?} vs {pattern:?}");
        }

        let terminal = GlobPattern::new("@(!(!(a)))", GlobOptions::default()).unwrap();
        assert!(terminal.is_match("a").unwrap());
        assert!(!terminal.is_match("abc").unwrap());
        for pattern in ["*(!(f))", "+(!(f))"] {
            assert!(matches("fa", pattern), "fa vs {pattern:?}");
        }
    }

    #[test]
    fn respects_negation_and_dotfiles() {
        assert!(matches("foo.js", "!*.ts"));
        assert!(!matches("foo.ts", "!*.ts"));
        assert!(matches("abxx_.c/", "!**/*.c"));
        assert!(!matches("abxx_.c", "!**/*.c"));
        assert!(!matches("abxx_.c/", "!!**/*.c"));
        assert!(
            !is_match(
                "c___-._/",
                "!c*",
                GlobOptions {
                    nobrace: true,
                    posix: true,
                    ..GlobOptions::default()
                }
            )
            .unwrap()
        );
        assert!(matches("c___-._/", "!!c*"));
        assert!(!matches(".git", "*"));
        assert!(
            is_match(
                ".git",
                "*",
                GlobOptions {
                    dot: true,
                    ..GlobOptions::default()
                }
            )
            .unwrap()
        );
    }

    #[test]
    fn collapses_long_even_escape_runs() {
        let pattern = format!("{}A", "\\".repeat(65_500));
        assert!(matches(r"\A", &pattern));
    }

    #[test]
    fn matches_javascript_utf16_code_units() {
        assert!(!matches("😀", "?"));
        assert!(matches("😀", "??"));
        let emoji = '\u{1f600}';
        assert!(matches(&format!("{emoji}x"), &format!("{emoji}*")));
        assert!(
            is_match(
                &format!("x/{emoji}/y"),
                &format!("{{{emoji},foo}}"),
                GlobOptions {
                    contains: true,
                    ..GlobOptions::default()
                }
            )
            .unwrap()
        );
    }

    #[test]
    fn unicode_flag_matches_non_bmp_characters_as_code_points() {
        assert!(
            is_match(
                "😀😀abc/b_9",
                "@(b|😀)**",
                GlobOptions {
                    unicode: true,
                    ..GlobOptions::default()
                }
            )
            .unwrap()
        );
    }

    #[test]
    fn preserves_unicode_fastpath_identity_escape_failure() {
        let options = GlobOptions {
            nocase: true,
            unicode: true,
            ..GlobOptions::default()
        };
        let raw = GlobPattern::new("\u{017f}", options.clone()).unwrap();
        assert_eq!(raw.source(), "$^");
        assert!(!raw.is_match("s").unwrap());
        assert!(raw.is_match("\u{017f}").unwrap());

        for (input, pattern) in [
            ("s", "@(\u{017f})"),
            ("s", "{\u{017f},x}"),
            ("x/s", "x/\u{017f}"),
            ("s", "*\u{017f}"),
        ] {
            let grouped = GlobPattern::new(pattern, options.clone()).unwrap();
            assert_ne!(grouped.source(), "$^");
            assert!(grouped.is_match(input).unwrap(), "{input:?} vs {pattern:?}");
        }
    }

    #[test]
    fn bash_stars_after_extglobs_follow_the_slow_parser() {
        let options = GlobOptions {
            bash: true,
            ..GlobOptions::default()
        };
        let pattern = "@(bar|\u{1f600})**@(\u{1f600}|bar)";
        let compiled = GlobPattern::new(pattern, options.clone()).unwrap();
        assert_eq!(compiled.source(), "^(?:(bar|\u{1f600}).*?(\u{1f600}|bar))$");
        assert!(
            compiled
                .is_match("\u{1f600}\u{1f600}\u{1f600}-/9_b.x_9/.\u{1f600}")
                .unwrap()
        );

        assert!(is_match("a/.hidden/b", "@(a)**/b", options).unwrap());
    }

    #[test]
    fn rejects_empty_inputs_and_patterns() {
        assert!(!matches("", "**"));
        assert!(GlobPattern::new("", GlobOptions::default()).is_err());
    }

    #[test]
    fn respects_posix_class_negation_mode() {
        assert!(matches("a", "[!a]"));
        assert!(!matches("b", "[!a]"));
        let options = GlobOptions {
            posix: true,
            ..GlobOptions::default()
        };
        assert!(!is_match("a", "[!a]", options.clone()).unwrap());
        assert!(is_match("b", "[!a]", options).unwrap());
        assert!(
            is_match(
                ".",
                "[!ab]",
                GlobOptions {
                    posix: true,
                    literal_brackets: Some(true),
                    ..GlobOptions::default()
                }
            )
            .unwrap()
        );
    }

    #[test]
    fn windows_classes_remain_valid_with_unicode_flags() {
        let options = GlobOptions {
            windows: true,
            unicode: true,
            ..GlobOptions::default()
        };
        let qmark = GlobPattern::new("?", options.clone()).unwrap();
        assert_eq!(qmark.source(), r"^(?:[^.\\/])$");
        assert!(qmark.is_match("a").unwrap());
        for input in [".", "/", "\\"] {
            assert!(!qmark.is_match(input).unwrap(), "{input:?}");
        }

        let negated = GlobPattern::new("[^a]", options).unwrap();
        assert!(negated.is_match("b").unwrap());
        for input in ["a", "/", "\\"] {
            assert!(!negated.is_match(input).unwrap(), "{input:?}");
        }
    }

    #[test]
    fn preserves_fastpath_and_repeated_globstar_semantics() {
        assert!(!matches("a/", "a*"));
        assert!(matches("a/", "*"));
        assert!(!matches("a/b", "****"));
        assert!(matches("a", "a/**/**"));
        let bash = GlobOptions {
            bash: true,
            ..GlobOptions::default()
        };
        assert!(is_match("a/.hidden", "*", bash.clone()).unwrap());
        assert!(!is_match("a/.hidden", "a*", bash.clone()).unwrap());
        assert!(is_match("a/b/.js/c.txt", "**/*", bash).unwrap());
        assert!(
            !is_match(
                "./.b-c/xb9b",
                "!(js|c)",
                GlobOptions {
                    bash: true,
                    dot: true,
                    ..GlobOptions::default()
                }
            )
            .unwrap()
        );
        assert!(
            is_match(
                "file.b/",
                "**/*.b",
                GlobOptions {
                    noglobstar: true,
                    ..GlobOptions::default()
                }
            )
            .unwrap()
        );
        assert!(matches("-.-9xb//xb.c", "**/*.c"));
    }

    #[test]
    fn contains_negation_checks_the_prefix() {
        let options = GlobOptions {
            contains: true,
            ..GlobOptions::default()
        };
        assert!(is_match("ba", "!a", options.clone()).unwrap());
        assert!(!is_match("ab", "!a", options.clone()).unwrap());
        assert!(!is_match("ab", "a/**", options).unwrap());
        assert!(
            !is_match(
                "ba",
                "**/a",
                GlobOptions {
                    contains: true,
                    ..GlobOptions::default()
                }
            )
            .unwrap()
        );
        assert!(
            !is_match(
                "a",
                "**/a",
                GlobOptions {
                    noglobstar: true,
                    ..GlobOptions::default()
                }
            )
            .unwrap()
        );
        assert!(
            !is_match(
                "b",
                "!(b|c)",
                GlobOptions {
                    contains: true,
                    ..GlobOptions::default()
                }
            )
            .unwrap()
        );
    }

    #[test]
    fn rejects_unsafe_nesting_and_range_overflow() {
        let accepted = format!("{}a{}", "{".repeat(MAX_NESTING), "}".repeat(MAX_NESTING));
        assert!(GlobPattern::new(&accepted, GlobOptions::default()).is_ok());

        let rejected = format!(
            "{}a{}",
            "{".repeat(MAX_NESTING + 1),
            "}".repeat(MAX_NESTING + 1)
        );
        assert!(GlobPattern::new(&rejected, GlobOptions::default()).is_err());
        let repeated_negative_suffix = "!(*).x".repeat(16);
        let error = GlobPattern::new(&repeated_negative_suffix, GlobOptions::default())
            .err()
            .expect("repeated negative suffixes must exhaust the compile-work budget");
        assert!(error.to_string().contains("safe work limit"));
        assert!(
            GlobPattern::new(
                "{9223372036854775806..9223372036854775807}",
                GlobOptions::default()
            )
            .is_ok()
        );
        assert!(GlobPattern::new("{1..2..-9223372036854775808}", GlobOptions::default()).is_ok());
    }

    #[test]
    fn rejects_wide_patterns_without_rejecting_literal_delimiters() {
        let wide = format!("({})", vec!["a"; MAX_ALTERNATION_BRANCHES + 1].join("|"));
        let error = GlobPattern::new(&wide, GlobOptions::default())
            .err()
            .expect("wide alternation must be rejected before regex compilation");
        assert!(error.to_string().contains("alternation branches"));

        let independent_groups = "(a|b)".repeat(MAX_ALTERNATION_BRANCHES);
        assert!(GlobPattern::new(&independent_groups, GlobOptions::default()).is_ok());

        let brackets = "[".repeat(MAX_BRACKET_MARKERS + 1);
        let error = GlobPattern::new(&brackets, GlobOptions::default())
            .err()
            .expect("unmatched bracket runs must have a structural bound");
        assert!(error.to_string().contains("bracket markers"));

        let escaped_brackets = r"\[".repeat(MAX_BRACKET_MARKERS + 1);
        assert!(GlobPattern::new(&escaped_brackets, GlobOptions::default()).is_ok());
        let literal_commas = ",".repeat(MAX_ALTERNATION_BRANCHES);
        assert!(is_match(&literal_commas, &literal_commas, GlobOptions::default()).unwrap());

        let in_class = format!("[{}]", "(".repeat(MAX_NESTING + 1));
        assert!(GlobPattern::new(&in_class, GlobOptions::default()).is_ok());
        let literal_braces = r"\{".repeat(MAX_NESTING + 1);
        assert!(GlobPattern::new(&literal_braces, GlobOptions::default()).is_ok());
        let disabled_braces = "{".repeat(MAX_NESTING + 1);
        assert!(
            GlobPattern::new(
                &disabled_braces,
                GlobOptions {
                    nobrace: true,
                    ..GlobOptions::default()
                }
            )
            .is_ok()
        );
        assert!(GlobPattern::new(&"(".repeat(MAX_NESTING + 1), GlobOptions::default()).is_err());
        assert!(GlobPattern::new(&"{".repeat(MAX_NESTING + 1), GlobOptions::default()).is_err());

        let quote_in_class = format!(
            "[\"]{}a{}",
            "{".repeat(MAX_NESTING + 1),
            "}".repeat(MAX_NESTING + 1)
        );
        assert!(GlobPattern::new(&quote_in_class, GlobOptions::default()).is_err());

        let oversized = "a".repeat(MAX_LENGTH + 1);
        let error = GlobPattern::new(
            &oversized,
            GlobOptions {
                max_length: Some(MAX_LENGTH * 2),
                ..GlobOptions::default()
            },
        )
        .err()
        .expect("maxLength must not raise the hard Picomatch ceiling");
        assert!(error.to_string().contains("65536"));
    }

    #[test]
    fn matches_adversarial_extglob_and_legacy_nocase_cases() {
        assert!(matches("b", "./!a"));
        assert!(!matches("b", "!(a|?)"));
        assert!(matches("a/", "a/?(b)"));
        assert!(matches("a/", "a/*(b)"));
        assert!(!matches("a", "@(?|a)"));
        assert!(!matches("a", "+(a|?)"));
        assert!(!matches("a", "+(a|a)"));
        assert!(!matches("@x", "@(?|a)"));
        assert!(!matches("a", "*(?)"));
        assert!(!matches("aa", "*(?a|b)"));
        assert!(matches("b", "*(?a|b)"));
        assert!(
            is_match(
                "x",
                "!(a|b)",
                GlobOptions {
                    noextglob: true,
                    ..GlobOptions::default()
                }
            )
            .unwrap()
        );
        assert!(
            !is_match(
                "x/{a,b}",
                "**{a,b}",
                GlobOptions {
                    nobrace: true,
                    ..GlobOptions::default()
                }
            )
            .unwrap()
        );
        assert!(
            !is_match(
                "@x/x@a",
                "@(x)**@(a)",
                GlobOptions {
                    noextglob: true,
                    ..GlobOptions::default()
                }
            )
            .unwrap()
        );
        assert!(
            is_match(
                "x",
                "!!(a|b)",
                GlobOptions {
                    noextglob: true,
                    ..GlobOptions::default()
                }
            )
            .unwrap()
        );
        assert!(matches("abab", "*(ab|abab)"));
        assert!(matches("\u{1f600}\u{1f600}", "*(\u{1f600}|\u{1f600})"));
        assert!(matches("?a", "(?(a))"));
        assert!(matches("@?a", "@(?(a))"));
        assert!(matches("a", "@(x|?(a))"));
        assert!(!matches("?a", "@(x|?(a))"));
        assert!(matches("a|", "a||b"));
        assert!(matches("b", "x/a|b"));
        assert!(matches("!", "[^]]"));
        assert!(
            GlobPattern::new(
                "[([:]a",
                GlobOptions {
                    strict_brackets: true,
                    ..GlobOptions::default()
                }
            )
            .is_ok()
        );
        assert!(
            is_match(
                "?",
                "!!(?)",
                GlobOptions {
                    noextglob: true,
                    ..GlobOptions::default()
                }
            )
            .unwrap()
        );
        assert!(
            is_match(
                "x/a",
                "**(a|b)",
                GlobOptions {
                    noextglob: true,
                    ..GlobOptions::default()
                }
            )
            .unwrap()
        );
        assert!(
            !is_match(
                "a",
                "**/*(a|b)",
                GlobOptions {
                    noglobstar: true,
                    ..GlobOptions::default()
                }
            )
            .unwrap()
        );

        let adversarial = format!("{}y", "a".repeat(30));
        let error = is_match(&adversarial, "+(a*)b", GlobOptions::default())
            .expect_err("catastrophic backtracking must stop at the execution budget");
        assert!(error.to_string().contains("execution exceeded"));
        assert!(
            is_match(
                &format!("{}b", "a".repeat(30)),
                "+(a*)b",
                GlobOptions::default()
            )
            .unwrap()
        );
        assert!(is_match(&"a".repeat(1_000_000), "*", GlobOptions::default()).unwrap());

        let nocase = GlobOptions {
            nocase: true,
            ..GlobOptions::default()
        };
        assert!(!is_match("\u{017f}", "s", nocase.clone()).unwrap());
        assert!(!is_match("\u{0131}", "i", nocase.clone()).unwrap());
        assert!(!is_match("\u{212a}", "[a-z]", nocase.clone()).unwrap());
        assert!(is_match("\u{017f}", "\u{017f}", nocase).unwrap());
        assert!(
            !is_match(
                "\u{1e9e}",
                "[\u{00df}]",
                GlobOptions {
                    nocase: true,
                    ..GlobOptions::default()
                }
            )
            .unwrap()
        );
        assert!(
            is_match(
                "\u{019b}",
                "\u{a7dc}",
                GlobOptions {
                    nocase: true,
                    ..GlobOptions::default()
                }
            )
            .unwrap()
        );
        assert!(
            !is_match(
                "\u{017f}",
                "[\u{e000}-\u{f8ff}]",
                GlobOptions {
                    nocase: true,
                    ..GlobOptions::default()
                }
            )
            .unwrap()
        );
        assert!(
            is_match(
                "\u{017f}",
                "[^\u{e000}-\u{f8ff}]",
                GlobOptions {
                    nocase: true,
                    ..GlobOptions::default()
                }
            )
            .unwrap()
        );
        assert!(
            !is_match(
                "\u{0200}",
                "[\u{0100}-\u{017f}]",
                GlobOptions {
                    nocase: true,
                    ..GlobOptions::default()
                }
            )
            .unwrap()
        );

        let strict = GlobOptions {
            strict_brackets: true,
            ..GlobOptions::default()
        };
        assert!(GlobPattern::new("{", strict.clone()).is_err());
        assert!(GlobPattern::new("}", strict.clone()).is_ok());
        assert!(GlobPattern::new("[[]", strict.clone()).is_ok());
        assert!(GlobPattern::new("[!]", strict.clone()).is_ok());
        assert!(GlobPattern::new("[!]]", strict.clone()).is_err());
        assert!(GlobPattern::new("[[:alpha:]", strict.clone()).is_err());
        assert!(
            GlobPattern::new(
                "{",
                GlobOptions {
                    strict_brackets: true,
                    nobrace: true,
                    ..GlobOptions::default()
                }
            )
            .is_ok()
        );
        assert!(
            GlobPattern::new(
                "[",
                GlobOptions {
                    strict_brackets: true,
                    nobracket: true,
                    ..GlobOptions::default()
                }
            )
            .is_ok()
        );
    }

    #[test]
    fn matches_directed_regex_and_contains_regressions() {
        let regex = GlobOptions {
            regex: true,
            ..GlobOptions::default()
        };
        assert!(is_match("/", "[^a]*", regex.clone()).unwrap());
        assert!(is_match("b/", "[^a]*", regex.clone()).unwrap());
        assert!(!is_match("/b", "[^a]*", regex).unwrap());

        let contains = GlobOptions {
            contains: true,
            ..GlobOptions::default()
        };
        assert!(is_match("a/b/c", "a/!(b)", contains.clone()).unwrap());
        assert!(!is_match("a/b", "a/!(b)", contains.clone()).unwrap());
        assert!(is_match("a/c", "a/!(b)", contains.clone()).unwrap());
        assert!(is_match("a", "!(!(a))", contains.clone()).unwrap());
        assert!(!is_match("ab", "!(!(a))", contains.clone()).unwrap());
        assert!(is_match("ba", "!(!(a))", contains.clone()).unwrap());
        assert!(!is_match("b", "!(!(a))", contains.clone()).unwrap());
        assert!(!is_match(".a", "**/*a", contains.clone()).unwrap());
        assert!(!is_match("x/.a", "**/*a", contains.clone()).unwrap());
        assert!(is_match("x/a", "**/*a", contains).unwrap());
        assert!(
            is_match(
                ".a",
                "**/*",
                GlobOptions {
                    contains: true,
                    ..GlobOptions::default()
                }
            )
            .unwrap()
        );

        assert!(!is_match("a/", "**?(a)", GlobOptions::default()).unwrap());
        assert!(!is_match("a/b", "**?(a)", GlobOptions::default()).unwrap());
        assert!(is_match("a", "**?(a)", GlobOptions::default()).unwrap());
        assert!(is_match("x/a", "**@(a)", GlobOptions::default()).unwrap());
        assert!(!is_match("b", "***(a)", GlobOptions::default()).unwrap());
        assert!(is_match("a/b", "@(a|b)**@(a|b)", GlobOptions::default()).unwrap());
        assert!(
            is_match(
                "x",
                "***(a|b)",
                GlobOptions {
                    bash: true,
                    ..GlobOptions::default()
                }
            )
            .unwrap()
        );
        assert!(
            is_match(
                "x",
                "!!(a|b)",
                GlobOptions {
                    noextglob: true,
                    ..GlobOptions::default()
                }
            )
            .unwrap()
        );
        assert!(is_match("a/a", "**{a,b}", GlobOptions::default()).unwrap());
        assert!(
            !is_match(
                "ab/.9.x",
                "!a*",
                GlobOptions {
                    bash: true,
                    ..GlobOptions::default()
                }
            )
            .unwrap()
        );
        assert!(
            is_match(
                ".\u{1f600}aa.a_._//9aa",
                "**/*.a",
                GlobOptions {
                    contains: true,
                    ..GlobOptions::default()
                }
            )
            .unwrap()
        );
        assert!(
            is_match(
                "._\u{1f600}__",
                "{\u{1f600},b}",
                GlobOptions {
                    contains: true,
                    nocase: true,
                    unicode: true,
                    literal_brackets: Some(false),
                    ..GlobOptions::default()
                }
            )
            .unwrap()
        );
        assert!(
            !is_match(
                ".-xcb-.\u{1f600}/.c_xc0ca9/_9x",
                "**/*.\u{1f600}",
                GlobOptions {
                    contains: true,
                    ..GlobOptions::default()
                }
            )
            .unwrap()
        );
        assert!(is_match("a/a/a", "!(b)**/*", GlobOptions::default()).unwrap());
        assert!(!is_match("b/a", "!(b)**/*", GlobOptions::default()).unwrap());
        assert!(
            is_match(
                "a/.x/y",
                "!(b)**/*",
                GlobOptions {
                    dot: true,
                    ..GlobOptions::default()
                }
            )
            .unwrap()
        );
        assert!(!is_match("a/.x/y", "!(b)**/*", GlobOptions::default()).unwrap());
    }

    #[test]
    fn reports_stateful_match_and_capture_ranges_in_utf16_units() {
        let astral = GlobPattern::new(
            "*",
            GlobOptions {
                capture: true,
                unicode: true,
                ..GlobOptions::default()
            },
        )
        .unwrap();
        assert_eq!(
            astral.find_match_from("\u{1f600}", 0, false).unwrap(),
            Some(GlobMatch {
                start: 0,
                end: 2,
                captures: vec![Some((0, 2))],
            })
        );

        let optional_capture = GlobPattern::new(
            "@(a|(b))",
            GlobOptions {
                capture: true,
                ..GlobOptions::default()
            },
        )
        .unwrap();
        assert_eq!(
            optional_capture.find_match_from("a", 0, false).unwrap(),
            Some(GlobMatch {
                start: 0,
                end: 1,
                captures: vec![Some((0, 1)), None],
            })
        );

        let contains = GlobPattern::new(
            "a",
            GlobOptions {
                contains: true,
                ..GlobOptions::default()
            },
        )
        .unwrap();
        assert_eq!(
            contains.find_match_from("ba", 0, false).unwrap(),
            Some(GlobMatch {
                start: 1,
                end: 2,
                captures: vec![],
            })
        );
        assert_eq!(contains.find_match_from("ba", 0, true).unwrap(), None);
    }
}
