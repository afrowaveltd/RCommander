//! A small syntax highlighter for the languages a file manager actually shows.
//!
//! This is **not** a parser and does not pretend to be one. It walks each line once, splits it into
//! spans and labels them `keyword`, `string`, `comment`, `number`, `type`, `function` or plain. That
//! is what makes a code block in a model answer, a help topic or the F3 viewer readable at a glance;
//! anything cleverer (semantic classification, error recovery) belongs in an editor, not here.
//!
//! Three deliberate properties:
//!
//! * **Streaming- and line-friendly.** [`Highlighter::line`] returns whole lines and carries the
//!   state that crosses a newline (a block comment, a multi-line string), so the caller can
//!   highlight an answer while it is still arriving.
//! * **Never panics and never loses text.** The concatenation of a line's spans is always the line,
//!   byte for byte. An unknown language yields a single plain span, which the tests check for every
//!   input rather than for a few examples.
//! * **No dependencies.** A regex engine would be a megabyte of build time for colouring a dozen
//!   tokens per line.

use std::fmt;

/// What a span is.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Kind {
    /// Nothing special: identifiers, whitespace, punctuation.
    Plain,
    /// A language keyword (`fn`, `let`, `if`, …).
    Keyword,
    /// A quoted string, including its delimiters.
    String,
    /// A comment, including its marker.
    Comment,
    /// A numeric literal.
    Number,
    /// A type, attribute or constant written in capitals.
    Type,
    /// An identifier directly followed by `(`.
    Function,
}

impl Kind {
    /// The theme role name for this kind, or `None` for plain text.
    pub fn role_name(self) -> Option<&'static str> {
        match self {
            Kind::Plain => None,
            Kind::Keyword => Some("code_keyword"),
            Kind::String => Some("code_string"),
            Kind::Comment => Some("code_comment"),
            Kind::Number => Some("code_number"),
            Kind::Type => Some("code_type"),
            Kind::Function => Some("code_function"),
        }
    }
}

/// One coloured piece of a line.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Span {
    pub kind: Kind,
    pub text: String,
}

impl Span {
    fn new(kind: Kind, text: impl Into<String>) -> Span {
        Span {
            kind,
            text: text.into(),
        }
    }
}

impl fmt::Display for Span {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.text)
    }
}

/// State that survives a newline.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct State {
    /// Inside a `/* … */` block comment.
    block_comment: bool,
    /// Inside a Python-style `"""` string.
    triple_double: bool,
    /// Inside a Python-style `'''` string.
    triple_single: bool,
}

/// The languages with a token table; everything else is highlighted as plain text.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Language {
    Rust,
    Toml,
    Json,
    Shell,
    Python,
    JavaScript,
    C,
    Go,
    Markdown,
    /// No highlighting, but the text is still split into one span.
    Plain,
}

impl Language {
    /// Guess from a fence info string (`rust`, `rs`, `sh`, `bash`, …) or a file name.
    pub fn parse(name: &str) -> Language {
        let name = name.trim().to_ascii_lowercase();
        // A fence may carry attributes (`rust,ignore`) or a path.
        let word = name
            .split([',', ' ', '\t'])
            .next()
            .unwrap_or("")
            .rsplit(['/', '\\'])
            .next()
            .unwrap_or("")
            .rsplit('.')
            .next()
            .unwrap_or("");
        match word {
            "rust" | "rs" => Language::Rust,
            "toml" | "ini" | "conf" | "cfg" => Language::Toml,
            "json" | "jsonc" => Language::Json,
            "sh" | "bash" | "shell" | "zsh" | "fish" | "console" => Language::Shell,
            "py" | "python" => Language::Python,
            "js" | "javascript" | "ts" | "typescript" | "jsx" | "tsx" => Language::JavaScript,
            "c" | "h" | "cpp" | "c++" | "cc" | "hpp" | "rs.h" => Language::C,
            "go" | "golang" => Language::Go,
            "md" | "markdown" => Language::Markdown,
            _ => Language::Plain,
        }
    }

    /// The keywords of this language.
    fn keywords(self) -> &'static [&'static str] {
        match self {
            Language::Rust => &[
                "as", "async", "await", "break", "const", "continue", "crate", "dyn", "else",
                "enum", "extern", "false", "fn", "for", "if", "impl", "in", "let", "loop", "match",
                "mod", "move", "mut", "pub", "ref", "return", "self", "Self", "static", "struct",
                "super", "trait", "true", "type", "unsafe", "use", "where", "while",
            ],
            Language::Toml => &["true", "false"],
            Language::Json => &["true", "false", "null"],
            Language::Shell => &[
                "break", "case", "continue", "do", "done", "elif", "else", "esac", "export", "fi",
                "for", "function", "if", "in", "local", "readonly", "return", "select", "then",
                "until", "while",
            ],
            Language::Python => &[
                "and", "as", "assert", "async", "await", "break", "class", "continue", "def",
                "del", "elif", "else", "except", "False", "finally", "for", "from", "global", "if",
                "import", "in", "is", "lambda", "None", "nonlocal", "not", "or", "pass", "raise",
                "return", "True", "try", "while", "with", "yield",
            ],
            Language::JavaScript => &[
                "async",
                "await",
                "break",
                "case",
                "catch",
                "class",
                "const",
                "continue",
                "default",
                "delete",
                "do",
                "else",
                "export",
                "extends",
                "false",
                "finally",
                "for",
                "function",
                "if",
                "import",
                "in",
                "instanceof",
                "let",
                "new",
                "null",
                "of",
                "return",
                "static",
                "super",
                "switch",
                "this",
                "throw",
                "true",
                "try",
                "typeof",
                "var",
                "void",
                "while",
                "yield",
            ],
            Language::C => &[
                "auto",
                "break",
                "case",
                "char",
                "const",
                "continue",
                "default",
                "do",
                "double",
                "else",
                "enum",
                "extern",
                "float",
                "for",
                "goto",
                "if",
                "int",
                "long",
                "register",
                "return",
                "short",
                "signed",
                "sizeof",
                "static",
                "struct",
                "switch",
                "typedef",
                "union",
                "unsigned",
                "void",
                "volatile",
                "while",
                "class",
                "namespace",
                "new",
                "private",
                "protected",
                "public",
                "template",
                "this",
                "throw",
                "try",
                "catch",
                "nullptr",
                "using",
                "virtual",
            ],
            Language::Go => &[
                "break",
                "case",
                "chan",
                "const",
                "continue",
                "default",
                "defer",
                "else",
                "fallthrough",
                "for",
                "func",
                "go",
                "goto",
                "if",
                "import",
                "interface",
                "map",
                "package",
                "range",
                "return",
                "select",
                "struct",
                "switch",
                "type",
                "var",
            ],
            Language::Markdown | Language::Plain => &[],
        }
    }

    /// Built-in primitive type names, which are lower case and would otherwise read as plain.
    fn primitive_types(self) -> &'static [&'static str] {
        match self {
            Language::Rust => &[
                "bool", "char", "f32", "f64", "i8", "i16", "i32", "i64", "i128", "isize", "str",
                "u8", "u16", "u32", "u64", "u128", "usize",
            ],
            Language::Go => &[
                "bool",
                "byte",
                "complex64",
                "complex128",
                "error",
                "float32",
                "float64",
                "int",
                "int8",
                "int16",
                "int32",
                "int64",
                "rune",
                "string",
                "uint",
                "uint8",
                "uint16",
                "uint32",
                "uint64",
                "uintptr",
            ],
            _ => &[],
        }
    }

    /// Comment prefixes that run to the end of the line.
    fn line_comments(self) -> &'static [&'static str] {
        match self {
            Language::Rust
            | Language::C
            | Language::Go
            | Language::JavaScript
            | Language::Toml
            | Language::Python => &["//", "#"],
            Language::Shell => &["#"],
            // JSON has no comments. Treating `//` as one would colour half a URL wrong, so nothing
            // is claimed here; `jsonc` is folded into `Json` and simply loses its comments.
            Language::Json | Language::Markdown | Language::Plain => &[],
        }
    }

    /// True when this language has `/* … */` block comments.
    fn has_block_comments(self) -> bool {
        matches!(
            self,
            Language::Rust | Language::C | Language::Go | Language::JavaScript
        )
    }

    /// True when this language has Python's triple-quoted strings.
    fn has_triple_strings(self) -> bool {
        matches!(self, Language::Python)
    }
}

/// A highlighter for one code block.
#[derive(Clone, Debug)]
pub struct Highlighter {
    language: Language,
    state: State,
}

impl Highlighter {
    /// A highlighter for `language`.
    pub fn new(language: &str) -> Highlighter {
        Highlighter {
            language: Language::parse(language),
            state: State::default(),
        }
    }

    /// The language being highlighted.
    pub fn language(&self) -> Language {
        self.language
    }

    /// Highlight one line; the line break is not included.
    ///
    /// The concatenation of the returned spans is exactly `line`, which is the property that keeps
    /// a half-received answer readable: nothing is dropped, only coloured.
    pub fn line(&mut self, line: &str) -> Vec<Span> {
        if self.language == Language::Markdown {
            return highlight_markdown(line);
        }
        if self.language == Language::Plain {
            return vec![Span::new(Kind::Plain, line)];
        }
        let chars: Vec<char> = line.chars().collect();
        let mut spans: Vec<Span> = Vec::new();
        let mut buffer = String::new();
        let mut index = 0;

        // A block comment or triple string that started on an earlier line owns the start of this
        // one; the state is the only thing a highlighter may carry across lines.
        if self.state.block_comment {
            let (text, next) = take_until(&chars, 0, "*/");
            spans.push(Span::new(Kind::Comment, text));
            if next.is_none() {
                return spans;
            }
            self.state.block_comment = false;
            index = next.unwrap_or(chars.len());
        } else if self.state.triple_double || self.state.triple_single {
            let quote = if self.state.triple_double {
                "\"\"\""
            } else {
                "'''"
            };
            let (text, next) = take_until(&chars, 0, quote);
            spans.push(Span::new(Kind::String, text));
            if next.is_none() {
                return spans;
            }
            self.state.triple_double = false;
            self.state.triple_single = false;
            index = next.unwrap_or(chars.len());
        }

        while index < chars.len() {
            let c = chars[index];

            // Whitespace and other punctuation keep the surrounding spans separate.
            if c.is_whitespace() {
                buffer.push(c);
                index += 1;
                continue;
            }

            // Block comment start.
            if self.language.has_block_comments() && starts_with(&chars, index, "/*") {
                flush(&mut buffer, &mut spans);
                // Skip the opening `/*`: `/**/` must not match its own delimiter.
                let (rest, next) = take_until(&chars, index + 2, "*/");
                let open_ended = next.is_none();
                spans.push(Span::new(Kind::Comment, format!("/*{rest}")));
                if open_ended {
                    self.state.block_comment = true;
                    return spans;
                }
                index = next.unwrap_or(chars.len());
                continue;
            }

            // Line comment: everything left on the line.
            if let Some(marker) = self
                .language
                .line_comments()
                .iter()
                .find(|marker| starts_with(&chars, index, marker))
            {
                // `//` inside a Rust lifetime (`&'a str`) does not occur, but `#` inside a Rust
                // attribute does: `#[derive(...)]` is code, not a comment. Rust uses `//` only.
                if !(self.language == Language::Rust && *marker == "#") {
                    flush(&mut buffer, &mut spans);
                    let text: String = chars[index..].iter().collect();
                    spans.push(Span::new(Kind::Comment, text));
                    return spans;
                }
            }

            // Triple-quoted string (Python).
            if self.language.has_triple_strings()
                && (starts_with(&chars, index, "\"\"\"") || starts_with(&chars, index, "'''"))
            {
                flush(&mut buffer, &mut spans);
                let quote = if starts_with(&chars, index, "\"\"\"") {
                    "\"\"\""
                } else {
                    "'''"
                };
                let (rest, next) = take_until(&chars, index + 3, quote);
                let open_ended = next.is_none();
                // The opening delimiter belongs to the string, so it is put back in front.
                spans.push(Span::new(Kind::String, format!("{quote}{rest}")));
                if open_ended {
                    self.state.triple_double = quote == "\"\"\"";
                    self.state.triple_single = !self.state.triple_double;
                    return spans;
                }
                index = next.unwrap_or(chars.len());
                continue;
            }

            // Quoted string, with backslash escapes.
            if c == '"' || c == '\'' || c == '`' {
                flush(&mut buffer, &mut spans);
                let (text, next) = take_string(&chars, index, c);
                spans.push(Span::new(Kind::String, text));
                index = next;
                continue;
            }

            // Number: digits with an optional fraction, exponent and unit suffix.
            if c.is_ascii_digit() {
                let start = index;
                while index < chars.len()
                    && (chars[index].is_ascii_alphanumeric()
                        || chars[index] == '.'
                        || chars[index] == '_')
                {
                    // `1.foo` is not a number: stop before a dot that is followed by a letter.
                    if chars[index] == '.'
                        && chars
                            .get(index + 1)
                            .is_some_and(|next| next.is_alphabetic())
                    {
                        break;
                    }
                    index += 1;
                }
                flush(&mut buffer, &mut spans);
                spans.push(Span::new(
                    Kind::Number,
                    chars[start..index].iter().collect::<String>(),
                ));
                continue;
            }

            // Identifier: keyword, function call, type-ish or plain.
            if c.is_alphabetic() || c == '_' {
                let start = index;
                while index < chars.len() && (chars[index].is_alphanumeric() || chars[index] == '_')
                {
                    index += 1;
                }
                let word: String = chars[start..index].iter().collect();
                let kind = classify_identifier(&self.language, &word, &chars, index);
                match kind {
                    Kind::Plain => buffer.push_str(&word),
                    other => {
                        flush(&mut buffer, &mut spans);
                        spans.push(Span::new(other, word));
                    }
                }
                continue;
            }

            buffer.push(c);
            index += 1;
        }
        flush(&mut buffer, &mut spans);
        if spans.is_empty() {
            spans.push(Span::new(Kind::Plain, line));
        }
        spans
    }
}

/// Classify one identifier.
fn classify_identifier(language: &Language, word: &str, chars: &[char], next_index: usize) -> Kind {
    if language.keywords().contains(&word) {
        return Kind::Keyword;
    }
    if language.primitive_types().contains(&word) {
        return Kind::Type;
    }
    // A call: the next non-space character is `(`.
    let mut look = next_index;
    while look < chars.len() && chars[look] == ' ' {
        look += 1;
    }
    if chars.get(look) == Some(&'(') && !word.chars().all(|c| c.is_ascii_uppercase()) {
        return Kind::Function;
    }
    // `SCREAMING_CASE`, `CamelCase` types and TOML table names read as types.
    let has_alpha = word.chars().any(|c| c.is_alphabetic());
    if has_alpha && word.chars().all(|c| !c.is_lowercase()) {
        return Kind::Type;
    }
    if word.chars().next().is_some_and(|c| c.is_uppercase())
        && word.chars().any(|c| c.is_lowercase())
    {
        return Kind::Type;
    }
    Kind::Plain
}

/// Markdown is highlighted by line shape: a heading, a fence marker, a quote, a list bullet.
fn highlight_markdown(line: &str) -> Vec<Span> {
    let trimmed = line.trim_start();
    let indent = &line[..line.len() - trimmed.len()];
    if trimmed.starts_with('#') {
        return vec![Span::new(Kind::Keyword, line)];
    }
    if trimmed.starts_with("```") || trimmed.starts_with("~~~") {
        return vec![Span::new(Kind::Comment, line)];
    }
    if trimmed.starts_with('>') {
        return vec![Span::new(Kind::Comment, line)];
    }
    if trimmed.starts_with("- ") || trimmed.starts_with("* ") || trimmed.starts_with("+ ") {
        let mut spans = Vec::new();
        if !indent.is_empty() {
            spans.push(Span::new(Kind::Plain, indent));
        }
        spans.push(Span::new(Kind::Keyword, &trimmed[..2]));
        spans.push(Span::new(Kind::Plain, &trimmed[2..]));
        return spans;
    }
    vec![Span::new(Kind::Plain, line)]
}

fn flush(buffer: &mut String, spans: &mut Vec<Span>) {
    if !buffer.is_empty() {
        spans.push(Span::new(Kind::Plain, std::mem::take(buffer)));
    }
}

fn starts_with(chars: &[char], index: usize, needle: &str) -> bool {
    needle
        .chars()
        .zip(index..)
        .all(|(expected, position)| chars.get(position) == Some(&expected))
}

/// Take from `index` up to and including `terminator`, returning the text and the next index.
///
/// `None` as the index means the terminator was not on this line.
fn take_until(chars: &[char], index: usize, terminator: &str) -> (String, Option<usize>) {
    let mut position = index;
    while position < chars.len() {
        if starts_with(chars, position, terminator) {
            let end = position + terminator.chars().count();
            let text: String = chars[index..end].iter().collect();
            return (text, Some(end));
        }
        position += 1;
    }
    (chars[index..].iter().collect(), None)
}

/// Take a quoted string starting at `index`, honouring backslash escapes.
fn take_string(chars: &[char], index: usize, quote: char) -> (String, usize) {
    let mut position = index + 1;
    while position < chars.len() {
        match chars[position] {
            '\\' => position += 2,
            c if c == quote => {
                let end = position + 1;
                return (chars[index..end].iter().collect(), end);
            }
            _ => position += 1,
        }
        // Clamp: a trailing backslash must not walk past the end.
        position = position.min(chars.len());
    }
    // Unterminated on this line: colour what is there, so a half-received string still reads.
    (chars[index..].iter().collect(), chars.len())
}

/// Highlight a whole block at once, which is what the viewer and tests use.
pub fn highlight_block(language: &str, lines: &[String]) -> Vec<Vec<Span>> {
    let mut highlighter = Highlighter::new(language);
    lines.iter().map(|line| highlighter.line(line)).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn kinds(spans: &[Span], word: &str) -> Option<Kind> {
        spans
            .iter()
            .find(|span| span.text == word)
            .map(|span| span.kind)
    }

    /// The single property every case below relies on: colouring never loses or reorders text.
    fn assert_lossless(language: &str, lines: &[&str]) {
        let owned: Vec<String> = lines.iter().map(|line| line.to_string()).collect();
        let highlighted = highlight_block(language, &owned);
        for (line, spans) in owned.iter().zip(&highlighted) {
            let rebuilt: String = spans.iter().map(Span::to_string).collect();
            assert_eq!(&rebuilt, line, "{language} lost text in {line:?}");
        }
    }

    #[test]
    fn the_fence_word_picks_the_language() {
        assert_eq!(Language::parse("rust"), Language::Rust);
        assert_eq!(Language::parse("RS"), Language::Rust);
        assert_eq!(Language::parse("bash"), Language::Shell);
        assert_eq!(Language::parse("sh,ignore"), Language::Shell);
        assert_eq!(Language::parse("/etc/nginx.conf"), Language::Toml);
        assert_eq!(Language::parse("main.rs"), Language::Rust);
        assert_eq!(Language::parse("python"), Language::Python);
        assert_eq!(Language::parse(""), Language::Plain);
        assert_eq!(Language::parse("brainfuck"), Language::Plain);
    }

    #[test]
    fn rust_keywords_strings_numbers_and_calls_are_told_apart() {
        let mut highlighter = Highlighter::new("rust");
        let spans = highlighter.line("let count: usize = read_file(\"/tmp/x\") + 42;");
        assert_eq!(kinds(&spans, "let"), Some(Kind::Keyword));
        assert_eq!(kinds(&spans, "usize"), Some(Kind::Type));
        assert_eq!(kinds(&spans, "read_file"), Some(Kind::Function));
        assert_eq!(kinds(&spans, "\"/tmp/x\""), Some(Kind::String));
        assert_eq!(kinds(&spans, "42"), Some(Kind::Number));
    }

    #[test]
    fn a_line_comment_takes_the_rest_of_the_line() {
        let mut highlighter = Highlighter::new("rust");
        let spans = highlighter.line("let a = 1; // the answer");
        assert_eq!(kinds(&spans, "// the answer"), Some(Kind::Comment));
        // A `#` in Rust is an attribute, not a comment.
        let spans = highlighter.line("#[derive(Debug)]");
        assert_eq!(kinds(&spans, "#[derive(Debug)]"), None);
        assert_eq!(kinds(&spans, "derive"), Some(Kind::Function));
        // In shell, `#` is a comment.
        let mut shell = Highlighter::new("sh");
        let spans = shell.line("ls -l # list");
        assert_eq!(kinds(&spans, "# list"), Some(Kind::Comment));
    }

    #[test]
    fn a_block_comment_continues_on_the_next_line() {
        let mut highlighter = Highlighter::new("rust");
        let spans = highlighter.line("/* start");
        assert_eq!(spans.last().unwrap().kind, Kind::Comment);
        let spans = highlighter.line("   middle");
        assert_eq!(spans.len(), 1);
        assert_eq!(spans[0].kind, Kind::Comment, "the state carries over");
        let spans = highlighter.line("end */ let a = 1;");
        assert_eq!(spans[0].kind, Kind::Comment);
        assert!(spans[0].text.ends_with("*/"), "{spans:?}");
        // After the terminator, normal highlighting resumes.
        assert_eq!(kinds(&spans, "let"), Some(Kind::Keyword));
        assert_eq!(kinds(&spans, "1"), Some(Kind::Number));
    }

    #[test]
    fn a_python_triple_string_spans_lines() {
        let mut highlighter = Highlighter::new("python");
        let spans = highlighter.line("doc = \"\"\"first");
        assert_eq!(spans.last().unwrap().kind, Kind::String);
        let spans = highlighter.line("second");
        assert_eq!(spans[0].kind, Kind::String);
        let spans = highlighter.line("third\"\"\" + 1");
        assert_eq!(spans[0].kind, Kind::String);
        assert_eq!(kinds(&spans, "1"), Some(Kind::Number));
    }

    #[test]
    fn an_unterminated_string_or_comment_still_colours_what_arrived() {
        // This is what a streaming answer looks like mid-line.
        let mut highlighter = Highlighter::new("rust");
        let spans = highlighter.line("let s = \"still typ");
        assert_eq!(kinds(&spans, "\"still typ"), Some(Kind::String));
        let spans = highlighter.line("/* nothing ends");
        assert_eq!(kinds(&spans, "/* nothing ends"), Some(Kind::Comment));
        // That block comment is still open, so the next line is a comment whatever it says: the
        // state is the highlighter's job, and a fresh one starts clean.
        let spans = highlighter.line("let a = 1;");
        assert_eq!(spans[0].kind, Kind::Comment);
        let mut fresh = Highlighter::new("rust");
        // An escape at the very end must not walk off the line.
        let spans = fresh.line("let s = \"x\\");
        assert_eq!(kinds(&spans, "\"x\\"), Some(Kind::String));
    }

    #[test]
    fn numbers_stop_before_a_field_access() {
        let mut highlighter = Highlighter::new("rust");
        let spans = highlighter.line("x.0.1");
        assert_eq!(kinds(&spans, "0.1"), Some(Kind::Number));
        let spans = highlighter.line("1.foo");
        assert_eq!(kinds(&spans, "1"), Some(Kind::Number));
        // `.foo` stays plain: a field access must not be eaten by the number.
        assert_eq!(kinds(&spans, ".foo"), Some(Kind::Plain));
    }

    #[test]
    fn other_languages_get_their_own_keywords() {
        for (language, line, keyword) in [
            ("toml", "enabled = true", "true"),
            ("json", "{\"ok\": true}", "true"),
            ("javascript", "const x = await f();", "await"),
            ("go", "func main() {}", "func"),
            ("c", "static int x = 0;", "static"),
            ("python", "def go():", "def"),
            ("sh", "if [ -f x ]; then", "if"),
        ] {
            let mut highlighter = Highlighter::new(language);
            let spans = highlighter.line(line);
            assert_eq!(kinds(&spans, keyword), Some(Kind::Keyword), "{language}");
        }
        // A TOML table header is plain, and its value is a number.
        let mut toml = Highlighter::new("toml");
        let spans = toml.line("[server]");
        assert_eq!(kinds(&spans, "[server]"), Some(Kind::Plain));
        let spans = toml.line("port = 8080");
        assert_eq!(kinds(&spans, "8080"), Some(Kind::Number));
    }

    #[test]
    fn json_does_not_invent_comments() {
        let mut highlighter = Highlighter::new("json");
        let spans = highlighter.line("{\"url\": \"http://example.org\"}");
        // The whole URL is a string, so the `//` inside it is never mistaken for a comment.
        assert_eq!(kinds(&spans, "\"http://example.org\""), Some(Kind::String));
        assert!(spans.iter().all(|span| span.kind != Kind::Comment));
    }

    #[test]
    fn markdown_is_coloured_by_line_shape() {
        let mut highlighter = Highlighter::new("markdown");
        let spans = highlighter.line("## A heading");
        assert_eq!(spans[0].kind, Kind::Keyword);
        let spans = highlighter.line("- a bullet");
        assert_eq!(spans[0].kind, Kind::Keyword);
        let spans = highlighter.line("plain text");
        assert_eq!(spans[0].kind, Kind::Plain);
    }

    #[test]
    fn nothing_is_lost_in_any_of_these_languages() {
        for language in [
            "rust",
            "toml",
            "json",
            "sh",
            "python",
            "javascript",
            "c",
            "go",
            "markdown",
            "weird",
        ] {
            assert_lossless(
                language,
                &[
                    "",
                    "   ",
                    "/* unterminated",
                    "*/",
                    "\"unterminated",
                    "'''triple",
                    "let x = 1; // done",
                    "ünïcödé čeština ěščřžýáíé",
                    "tab\there",
                    "0x1f 1e10 3.14 .5",
                    "a(b) A_B CamelCase SCREAMING",
                ],
            );
        }
    }

    #[test]
    fn an_unknown_language_is_one_plain_span() {
        let mut highlighter = Highlighter::new("brainfuck");
        let spans = highlighter.line("++[>+<-] let x = 1");
        assert_eq!(spans.len(), 1);
        assert_eq!(spans[0].kind, Kind::Plain);
        assert_eq!(spans[0].text, "++[>+<-] let x = 1");
    }

    #[test]
    fn every_coloured_kind_has_a_theme_role() {
        for kind in [
            Kind::Plain,
            Kind::Keyword,
            Kind::String,
            Kind::Comment,
            Kind::Number,
            Kind::Type,
            Kind::Function,
        ] {
            match kind.role_name() {
                Some(name) => assert!(
                    rc_core_theme_role_exists(name),
                    "{kind:?} names a role the theme does not define: {name}"
                ),
                None => assert_eq!(kind, Kind::Plain, "only plain text may lack a role"),
            }
        }
    }

    fn rc_core_theme_role_exists(name: &str) -> bool {
        crate::theme::Role::from_name(name).is_some()
    }
}
