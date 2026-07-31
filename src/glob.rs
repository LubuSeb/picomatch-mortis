use std::fmt;

use regress::{Flags, Regex};

const MAX_LENGTH: usize = 1024 * 64;

/// Options shared with Picomatch's matching API.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct GlobOptions {
    pub windows: bool,
    pub dot: bool,
    pub nocase: bool,
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
    negated: bool,
    has_globstar: bool,
    preserve_double_slash: bool,
}

impl GlobPattern {
    pub fn new(pattern: &str, options: GlobOptions) -> Result<Self, GlobError> {
        if pattern.len() > MAX_LENGTH {
            return Err(GlobError(format!(
                "Input length: {}, exceeds maximum allowed length: {MAX_LENGTH}",
                pattern.len()
            )));
        }

        let mut value = pattern;
        let mut negated = false;
        if !options.nonegate {
            let mut count = 0;
            while value.starts_with('!') && !value.starts_with("!(") {
                count += 1;
                value = &value[1..];
            }
            negated = count % 2 == 1;
        }
        if let Some(stripped) = value.strip_prefix("./") {
            value = stripped;
        }

        value = match value {
            "***" => "*",
            "**/**" | "**/**/**" => "**",
            _ => value,
        };
        let chars: Vec<char> = value.chars().collect();
        if options.strict_brackets {
            validate_brackets(&chars)?;
        }
        let mut compiler = Compiler::new(&options);
        let body = compiler.compile(&chars, true)?;
        let optional_slash = !options.strict_slashes && compiler.trailing_magic;
        let source = if options.contains {
            format!("(?:{body})")
        } else if optional_slash {
            format!(r"^(?:{body})\/?$")
        } else {
            format!("^(?:{body})$")
        };
        let flags = Flags {
            icase: options.nocase,
            ..Flags::default()
        };
        let regex = Regex::with_flags(&source, flags)
            .map_err(|error| GlobError(format!("Invalid generated regex: {error}")))?;

        Ok(Self {
            output: body,
            source,
            regex,
            options,
            negated,
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

    pub fn is_match(&self, input: &str) -> bool {
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
        self.regex.find(value).is_some() ^ self.negated
    }
}

pub fn is_match(input: &str, pattern: &str, options: GlobOptions) -> Result<bool, GlobError> {
    Ok(GlobPattern::new(pattern, options)?.is_match(input))
}

struct Compiler<'a> {
    options: &'a GlobOptions,
    trailing_magic: bool,
}

impl<'a> Compiler<'a> {
    fn new(options: &'a GlobOptions) -> Self {
        Self {
            options,
            trailing_magic: false,
        }
    }

    fn compile(&mut self, chars: &[char], mut segment_start: bool) -> Result<String, GlobError> {
        let mut output = String::new();
        let mut index = 0;
        let mut quoted = false;
        self.trailing_magic = false;

        while index < chars.len() {
            let value = chars[index];

            if value == '\\' {
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
                        push_literal(&mut output, next);
                        index += 2;
                    } else if next == '\\' {
                        output.push_str(r"\\");
                        index += 2;
                    } else if matches!(
                        next,
                        '*' | '?' | '+' | '@' | '!' | '(' | ')' | '[' | ']' | '{' | '}' | '|'
                    ) {
                        push_literal(&mut output, next);
                        index += 2;
                    } else {
                        output.push_str(r"\\");
                        index += 1;
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
                        for branch in branches {
                            if branch == ['/', '*', '*'] && chars.get(end + 1) == Some(&'/') {
                                let middle = if self.options.dot {
                                    r"\/(?:(?!\.{1,2}(?:/|$))[^/]+\/)*(?!\.{1,2}(?:/|$))[^/]+"
                                } else {
                                    r"\/(?:(?!\.)[^/]+\/)*(?!\.)[^/]+"
                                };
                                compiled.push(middle.to_owned());
                            } else {
                                compiled.push(self.compile(branch, segment_start)?);
                            }
                        }
                        format!("(?:{})", compiled.join("|"))
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
            {
                if let Some(end) = find_closing(chars, index + 1, '(', ')') {
                    let body = &chars[index + 2..end];
                    let branches = split_top_level(body, '|');
                    let mut compiled = Vec::with_capacity(branches.len());
                    for branch in branches {
                        compiled.push(self.compile(branch, segment_start)?);
                    }
                    let alternatives = compiled.join("|");
                    let expression = match value {
                        '@' => format!("(?:{alternatives})"),
                        '?' => format!("(?:{alternatives})?"),
                        '+' => format!("(?:{alternatives})+"),
                        '*' => format!("(?:{alternatives})*"),
                        '!' => {
                            let tail = if chars.get(end + 1) == Some(&'*') {
                                "[^/]*"
                            } else {
                                ""
                            };
                            let consume = if body.contains(&'/') { ".*" } else { "[^/]*" };
                            format!("(?!(?:{alternatives}){tail}(?:/|$)){consume}")
                        }
                        _ => unreachable!(),
                    };
                    if segment_start && !self.options.dot && value != '!' {
                        output.push_str(r"(?!\.)");
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
                    self.trailing_magic = false;
                    index += 1;
                    continue;
                }
                let mut end = index + 1;
                while chars.get(end) == Some(&'*') {
                    end += 1;
                }
                let followed_by_extglob =
                    matches!(chars.get(end), Some('?' | '*' | '+' | '@' | '!'))
                        && chars.get(end + 1) == Some(&'(');
                let followed_by_group = chars.get(end) == Some(&'{') || followed_by_extglob;
                let globstar = end - index > 1
                    && !self.options.noglobstar
                    && segment_start
                    && (end == chars.len() || chars.get(end) == Some(&'/') || followed_by_group);
                if globstar && segment_start && chars.get(end) == Some(&'/') {
                    if index == 0 {
                        output.push_str(r"(?:\/)?");
                    }
                    if self.options.dot {
                        output.push_str(r"(?:(?!\.{1,2}(?:/|$))[^/]+\/)*");
                    } else {
                        output.push_str(r"(?:(?!\.)[^/]+\/)*");
                    }
                    index = end + 1;
                    segment_start = true;
                } else if globstar && end == chars.len() {
                    let body = if self.options.dot {
                        r"(?:(?:(?!\.{1,2}(?:/|$))[^/]+(?:\/|$))|\/)*"
                    } else {
                        r"(?:(?:(?!\.)[^/]+(?:\/|$))|\/)*"
                    };
                    if index >= 2
                        && chars.get(index - 1) == Some(&'/')
                        && chars.get(index - 2) != Some(&'*')
                        && output.ends_with(r"\/")
                    {
                        output.truncate(output.len().saturating_sub(2));
                        output.push_str(&format!(r"(?:\/{body})?"));
                    } else {
                        if index == 0 {
                            output.push_str(r"\/*");
                        }
                        output.push_str(body);
                    }
                    index = end;
                    segment_start = false;
                } else if globstar {
                    if self.options.dot {
                        output.push_str(r"(?:(?!\.{1,2}(?:/|$))[^/]+\/)*(?!\.{1,2}(?:/|$))[^/]*?");
                    } else {
                        output.push_str(r"(?:(?!\.)[^/]+\/)*(?!\.)[^/]*?");
                    }
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
                    output.push_str(if self.options.bash { ".*?" } else { "[^/]*?" });
                    index = end;
                    segment_start = false;
                }
                self.trailing_magic = true;
                continue;
            }

            if value == '?' {
                if segment_start && !self.options.dot {
                    output.push_str(self.segment_guard());
                }
                output.push_str("[^/]");
                segment_start = false;
                self.trailing_magic = false;
                index += 1;
                continue;
            }

            if value == '[' && !self.options.nobracket {
                if let Some(end) = find_class_end(chars, index) {
                    let raw: String = chars[index + 1..end].iter().collect();
                    let translated = translate_class(&raw);
                    let literal = format!(r"\[{}\]", escape_regex(&raw));
                    let class = format!("[{translated}]");
                    if segment_start && has_known_posix(&raw) {
                        output.push_str("(?=.)");
                    }
                    match self.options.literal_brackets {
                        Some(true) => output.push_str(&literal),
                        Some(false) => output.push_str(&class),
                        None if class_has_magic(&raw) => output.push_str(&class),
                        None => output.push_str(&format!("(?:{literal}|{class})")),
                    }
                    segment_start = false;
                    self.trailing_magic = true;
                    index = end + 1;
                    continue;
                }
            }

            if value == '+' && index > 0 && matches!(chars[index - 1], ']' | ')' | '}') {
                output.push('+');
            } else if value == '(' || value == ')' || value == '|' {
                output.push(value);
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
        if character == '[' {
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
    let mut index = start + 1;
    if matches!(chars.get(index), Some('!') | Some('^')) {
        index += 1;
    }
    if chars.get(index) == Some(&']') {
        index += 1;
    }
    let mut escaped = false;
    let mut posix_end = None;
    while index < chars.len() {
        if !escaped && chars[index] == '[' && chars.get(index + 1) == Some(&':') {
            index += 2;
            while index + 1 < chars.len() {
                if chars[index] == ':' && chars[index + 1] == ']' {
                    posix_end = Some(index + 1);
                    index += 2;
                    break;
                }
                index += 1;
            }
            continue;
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
    let values: Vec<String> = parts.iter().map(|part| part.iter().collect()).collect();
    let step = if values.len() == 3 {
        values[2]
            .parse::<i64>()
            .ok()
            .filter(|step| *step != 0)
            .unwrap_or(1)
    } else {
        1
    };
    if let (Ok(start), Ok(end)) = (values[0].parse::<i64>(), values[1].parse::<i64>()) {
        let direction = if start <= end {
            step.abs()
        } else {
            -step.abs()
        };
        let mut current = start;
        let mut entries = Vec::new();
        while (direction > 0 && current <= end) || (direction < 0 && current >= end) {
            entries.push(current.to_string());
            if entries.len() > MAX_LENGTH {
                return Err(GlobError("Brace range is too large".to_owned()));
            }
            current += direction;
        }
        return Ok(format!("(?:{})", entries.join("|")));
    }
    let start: Vec<char> = values[0].chars().collect();
    let end: Vec<char> = values[1].chars().collect();
    if start.len() == 1 && end.len() == 1 {
        let a = start[0] as i64;
        let b = end[0] as i64;
        let direction = if a <= b { step.abs() } else { -step.abs() };
        let mut current = a;
        let mut entries = Vec::new();
        while (direction > 0 && current <= b) || (direction < 0 && current >= b) {
            if let Some(value) = char::from_u32(current as u32) {
                entries.push(escape_regex(&value.to_string()));
            }
            current += direction;
        }
        return Ok(format!("(?:{})", entries.join("|")));
    }
    Err(GlobError("Invalid brace range".to_owned()))
}

fn translate_class(raw: &str) -> String {
    let mut output = String::with_capacity(raw.len());
    let chars: Vec<_> = raw.chars().collect();
    for (index, character) in chars.iter().enumerate() {
        if *character == '[' && chars.get(index + 1) != Some(&':') {
            output.push('\\');
        }
        output.push(*character);
    }
    if output.starts_with('!') {
        output.replace_range(..1, "^");
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

fn validate_brackets(chars: &[char]) -> Result<(), GlobError> {
    let mut parens = 0usize;
    let mut brackets = 0usize;
    let mut escaped = false;
    let mut quoted = false;
    for &character in chars {
        if escaped {
            escaped = false;
            continue;
        }
        if character == '\\' {
            escaped = true;
            continue;
        }
        if character == '"' {
            quoted = !quoted;
            continue;
        }
        if quoted {
            continue;
        }
        match character {
            '[' => brackets += 1,
            ']' if brackets == 0 => {
                return Err(GlobError(
                    "Missing opening: \"[\" - use \"\\\\[\" to match literal characters".to_owned(),
                ));
            }
            ']' => brackets -= 1,
            '(' if brackets == 0 => parens += 1,
            ')' if brackets == 0 && parens == 0 => {
                return Err(GlobError(
                    "Missing opening: \"(\" - use \"\\\\(\" to match literal characters".to_owned(),
                ));
            }
            ')' if brackets == 0 => parens -= 1,
            _ => {}
        }
    }
    if brackets > 0 {
        return Err(GlobError(
            "Missing closing: \"]\" - use \"\\\\]\" to match literal characters".to_owned(),
        ));
    }
    if parens > 0 {
        return Err(GlobError(
            "Missing closing: \")\" - use \"\\\\)\" to match literal characters".to_owned(),
        ));
    }
    Ok(())
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
    }

    #[test]
    fn respects_negation_and_dotfiles() {
        assert!(matches("foo.js", "!*.ts"));
        assert!(!matches("foo.ts", "!*.ts"));
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
}
