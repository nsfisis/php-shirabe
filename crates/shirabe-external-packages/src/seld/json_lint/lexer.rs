//! ref: composer/vendor/seld/jsonlint/src/Seld/JsonLint/Lexer.php

use super::JsonParser;
use super::ParsingException;

/// ref: array{first_line, first_column, last_line, last_column} ($yylloc)
#[derive(Debug, Clone, Default)]
pub struct YylLoc {
    pub first_line: i64,
    pub first_column: i64,
    pub last_line: i64,
    pub last_column: i64,
}

#[derive(Debug)]
pub struct Lexer {
    rules: Vec<Rule>,
    input: Vec<u8>,
    more: bool,
    done: bool,
    offset: usize,
    flags: u32,

    pub match_: Vec<u8>,
    pub yylineno: i64,
    pub yyleng: i64,
    pub yytext: Vec<u8>,
    pub yylloc: YylLoc,
}

/// Each variant replicates the corresponding `\G`-anchored PHP rule. The PCRE rules are not portable
/// to the `regex` crate (possessive quantifiers, `\G`, `\b`), so they are matched by hand.
#[derive(Debug, Clone, Copy)]
enum Rule {
    BreakLine,    // 0: /\G\s*\n\r?/
    Whitespace,   // 1: /\G\s+/
    Number,       // 2
    Str,          // 3
    BraceOpen,    // 4: {
    BraceClose,   // 5: }
    BracketOpen,  // 6: [
    BracketClose, // 7: ]
    Comma,        // 8: ,
    Colon,        // 9: :
    True,         // 10
    False,        // 11
    Null,         // 12
    End,          // 13: /\G$/
    LineComment,  // 14: //
    OpenComment,  // 15: /*
    CloseComment, // 16: */
    AnyChar,      // 17: /\G./
}

impl Lexer {
    pub const EOF: i64 = 1;
    pub const T_INVALID: i64 = -1;
    pub const T_SKIP_WHITESPACE: i64 = 0;
    pub const T_ERROR: i64 = 2;
    pub const T_BREAK_LINE: i64 = 3;
    pub const T_COMMENT: i64 = 30;
    pub const T_OPEN_COMMENT: i64 = 31;
    pub const T_CLOSE_COMMENT: i64 = 32;

    pub fn new(flags: u32) -> Self {
        let rules = vec![
            Rule::BreakLine,
            Rule::Whitespace,
            Rule::Number,
            Rule::Str,
            Rule::BraceOpen,
            Rule::BraceClose,
            Rule::BracketOpen,
            Rule::BracketClose,
            Rule::Comma,
            Rule::Colon,
            Rule::True,
            Rule::False,
            Rule::Null,
            Rule::End,
            Rule::LineComment,
            Rule::OpenComment,
            Rule::CloseComment,
            Rule::AnyChar,
        ];
        Self {
            rules,
            input: Vec::new(),
            more: false,
            done: false,
            offset: 0,
            flags,
            match_: Vec::new(),
            yylineno: 0,
            yyleng: 0,
            yytext: Vec::new(),
            yylloc: YylLoc::default(),
        }
    }

    pub fn lex(&mut self) -> Result<i64, ParsingException> {
        loop {
            let symbol = self.next()?;
            match symbol {
                Self::T_SKIP_WHITESPACE | Self::T_BREAK_LINE => {}
                Self::T_COMMENT | Self::T_OPEN_COMMENT => {
                    if self.flags & JsonParser::ALLOW_COMMENTS == 0 {
                        return Err(self.parse_error(format!(
                            "Lexical error on line {}. Comments are not allowed.\n{}",
                            self.yylineno + 1,
                            self.show_position()
                        )));
                    }
                    let until = if symbol == Self::T_COMMENT {
                        Self::T_BREAK_LINE
                    } else {
                        Self::T_CLOSE_COMMENT
                    };
                    self.skip_until(until)?;
                    if self.done {
                        // last symbol '/\G$/' before EOF
                        return Ok(14);
                    }
                }
                Self::T_CLOSE_COMMENT => {
                    return Err(self.parse_error(format!(
                        "Lexical error on line {}. Unexpected token.\n{}",
                        self.yylineno + 1,
                        self.show_position()
                    )));
                }
                _ => return Ok(symbol),
            }
        }
    }

    pub fn set_input(&mut self, input: &str) {
        self.input = input.as_bytes().to_vec();
        self.more = false;
        self.done = false;
        self.offset = 0;
        self.yylineno = 0;
        self.yyleng = 0;
        self.yytext = Vec::new();
        self.match_ = Vec::new();
        self.yylloc = YylLoc {
            first_line: 1,
            first_column: 0,
            last_line: 1,
            last_column: 0,
        };
    }

    pub fn show_position(&self) -> String {
        if self.yylineno == 0 && self.offset == 1 && self.match_ != b"{" {
            let mut s = String::from_utf8_lossy(&self.match_).into_owned();
            s.push_str("...\n^");
            return s;
        }

        let pre = str_replace_nl(&self.get_past_input());
        let dash_count = (pre.len() as i64 - 1).max(0) as usize;
        let c = "-".repeat(dash_count);

        let upcoming = str_replace_nl(&self.get_upcoming_input());
        format!("{pre}{upcoming}\n{c}^")
    }

    pub fn get_past_input(&self) -> Vec<u8> {
        let past_length = self.offset as i64 - self.match_.len() as i64;
        let prefix = if past_length > 20 { "..." } else { "" };
        let start = (past_length - 20).max(0) as usize;
        let len = past_length.min(20).max(0) as usize;
        let slice = substr_bytes(&self.input, start, len);
        let mut out = prefix.as_bytes().to_vec();
        out.extend_from_slice(&slice);
        out
    }

    pub fn get_upcoming_input(&self) -> Vec<u8> {
        let mut next = self.match_.clone();
        if next.len() < 20 {
            let want = 20 - next.len();
            next.extend_from_slice(&substr_bytes(&self.input, self.offset, want));
        }
        let too_long = next.len() > 20;
        let mut out = substr_bytes(&next, 0, 20);
        if too_long {
            out.extend_from_slice(b"...");
        }
        out
    }

    pub fn get_full_upcoming_input(&self) -> Vec<u8> {
        let mut next = self.match_.clone();
        if next.first() == Some(&b'"') && byte_count(&next, b'"') == 1 {
            let len = self.input.len();
            let str_end = if len == self.offset {
                len
            } else {
                let q = find_byte_from(&self.input, b'"', self.offset + 1).unwrap_or(len);
                let q = if q == 0 { len } else { q };
                let n = find_byte_from(&self.input, b'\n', self.offset + 1).unwrap_or(len);
                let n = if n == 0 { len } else { n };
                q.min(n)
            };
            next.extend_from_slice(&self.input[self.offset..str_end]);
        } else if next.len() < 20 {
            let want = 20 - next.len();
            next.extend_from_slice(&substr_bytes(&self.input, self.offset, want));
        }
        next
    }

    fn parse_error(&self, str: String) -> ParsingException {
        ParsingException::new(str, Default::default())
    }

    fn skip_until(&mut self, token: i64) -> Result<(), ParsingException> {
        let mut symbol = self.next()?;
        while symbol != token && !self.done {
            symbol = self.next()?;
        }
        Ok(())
    }

    fn next(&mut self) -> Result<i64, ParsingException> {
        if self.done {
            return Ok(Self::EOF);
        }
        if self.offset == self.input.len() {
            self.done = true;
        }

        if !self.more {
            self.yytext = Vec::new();
            self.match_ = Vec::new();
        }

        for i in 0..self.rules.len() {
            if let Some(matched) = self.match_rule(self.rules[i]) {
                let lines: Vec<&[u8]> = split_bytes(&matched, b'\n');
                // array_shift: drop first element, count remaining
                let line_count = lines.len().saturating_sub(1);
                self.yylineno += line_count as i64;
                self.yylloc = YylLoc {
                    first_line: self.yylloc.last_line,
                    last_line: self.yylineno + 1,
                    first_column: self.yylloc.last_column,
                    last_column: if line_count > 0 {
                        lines[lines.len() - 1].len() as i64
                    } else {
                        self.yylloc.last_column + matched.len() as i64
                    },
                };
                self.yytext.extend_from_slice(&matched);
                self.match_.extend_from_slice(&matched);
                self.yyleng = self.yytext.len() as i64;
                self.more = false;
                self.offset += matched.len();
                return Ok(self.perform_action(i, &matched));
            }
        }

        if self.offset == self.input.len() {
            return Ok(Self::EOF);
        }

        Err(self.parse_error(format!(
            "Lexical error on line {}. Unrecognized text.\n{}",
            self.yylineno + 1,
            self.show_position()
        )))
    }

    /// Returns the matched bytes (anchored at `self.offset`) if the rule applies, mirroring
    /// `preg_match($rule, $input, $m, 0, $offset)` for the corresponding `\G`-anchored PHP pattern.
    fn match_rule(&self, rule: Rule) -> Option<Vec<u8>> {
        let input = &self.input;
        let off = self.offset;
        match rule {
            Rule::BreakLine => match_break_line(input, off),
            Rule::Whitespace => match_whitespace(input, off),
            Rule::Number => match_number(input, off),
            Rule::Str => match_string(input, off),
            Rule::BraceOpen => match_char(input, off, b'{'),
            Rule::BraceClose => match_char(input, off, b'}'),
            Rule::BracketOpen => match_char(input, off, b'['),
            Rule::BracketClose => match_char(input, off, b']'),
            Rule::Comma => match_char(input, off, b','),
            Rule::Colon => match_char(input, off, b':'),
            Rule::True => match_keyword(input, off, b"true"),
            Rule::False => match_keyword(input, off, b"false"),
            Rule::Null => match_keyword(input, off, b"null"),
            Rule::End => match_end(input, off),
            Rule::LineComment => match_literal(input, off, b"//"),
            Rule::OpenComment => match_literal(input, off, b"/*"),
            Rule::CloseComment => match_literal(input, off, b"*/"),
            Rule::AnyChar => match_any_char(input, off),
        }
    }

    fn perform_action(&mut self, rule: usize, _matched: &[u8]) -> i64 {
        match rule {
            0 => Self::T_BREAK_LINE,
            1 => Self::T_SKIP_WHITESPACE,
            2 => 6,
            3 => {
                // strip surrounding quotes: substr($yytext, 1, $yyleng-2)
                let len = self.yyleng;
                self.yytext = substr_bytes(&self.yytext, 1, (len - 2).max(0) as usize);
                4
            }
            4 => 17,
            5 => 18,
            6 => 23,
            7 => 24,
            8 => 22,
            9 => 21,
            10 => 10,
            11 => 11,
            12 => 8,
            13 => 14,
            14 => Self::T_COMMENT,
            15 => Self::T_OPEN_COMMENT,
            16 => Self::T_CLOSE_COMMENT,
            17 => Self::T_INVALID,
            _ => panic!("Unsupported rule {rule}"),
        }
    }
}

/// PHP `\s` (matches space, \t, \n, \r, \f, \v).
fn is_php_space(b: u8) -> bool {
    matches!(b, b' ' | b'\t' | b'\n' | b'\r' | 0x0c | 0x0b)
}

/// Rule 0: /\G\s*\n\r?/ — greedy whitespace ending at a newline, optionally followed by `\r`.
fn match_break_line(input: &[u8], off: usize) -> Option<Vec<u8>> {
    // Find the run of whitespace that ends with `\n` (followed by optional `\r`). PCRE backtracks
    // so the `\s*` consumes up to and including the last reachable `\n` within the leading run.
    let mut i = off;
    let mut last_nl: Option<usize> = None;
    while i < input.len() && is_php_space(input[i]) {
        if input[i] == b'\n' {
            last_nl = Some(i);
        }
        i += 1;
    }
    let nl = last_nl?;
    let mut end = nl + 1;
    if end < input.len() && input[end] == b'\r' {
        end += 1;
    }
    Some(input[off..end].to_vec())
}

/// Rule 1: /\G\s+/
fn match_whitespace(input: &[u8], off: usize) -> Option<Vec<u8>> {
    let mut i = off;
    while i < input.len() && is_php_space(input[i]) {
        i += 1;
    }
    if i == off {
        None
    } else {
        Some(input[off..i].to_vec())
    }
}

/// Rule 2: /\G-?([0-9]|[1-9][0-9]+)(\.[0-9]+)?([eE][+-]?[0-9]+)?\b/
fn match_number(input: &[u8], off: usize) -> Option<Vec<u8>> {
    let mut i = off;
    if i < input.len() && input[i] == b'-' {
        i += 1;
    }
    // integer part: a single digit OR [1-9][0-9]+
    let int_start = i;
    if i >= input.len() || !input[i].is_ascii_digit() {
        return None;
    }
    if input[i] == b'0' {
        // single 0 only (the [1-9][0-9]+ alternative cannot start with 0)
        i += 1;
    } else {
        // [1-9] then [0-9]* ; but grammar is `[0-9]` (single) | `[1-9][0-9]+` (2+ digits).
        // Both collapse to: one or more digits starting with 1-9.
        i += 1;
        while i < input.len() && input[i].is_ascii_digit() {
            i += 1;
        }
    }
    let _ = int_start;
    // fraction
    if i < input.len() && input[i] == b'.' {
        let mut j = i + 1;
        let frac_start = j;
        while j < input.len() && input[j].is_ascii_digit() {
            j += 1;
        }
        if j > frac_start {
            i = j;
        }
    }
    // exponent
    if i < input.len() && (input[i] == b'e' || input[i] == b'E') {
        let mut j = i + 1;
        if j < input.len() && (input[j] == b'+' || input[j] == b'-') {
            j += 1;
        }
        let exp_start = j;
        while j < input.len() && input[j].is_ascii_digit() {
            j += 1;
        }
        if j > exp_start {
            i = j;
        }
    }
    // \b word boundary: previous char (input[i-1]) is a word char (digit), so the next char must be
    // a non-word char (or end).
    if i < input.len() && is_word_byte(input[i]) {
        return None;
    }
    Some(input[off..i].to_vec())
}

fn is_word_byte(b: u8) -> bool {
    b.is_ascii_alphanumeric() || b == b'_'
}

/// Rule 3: {\G"(?>\\["bfnrt/\\]|\\u[a-fA-F0-9]{4}|[^\0-\x1f\\"]++)*+"}
fn match_string(input: &[u8], off: usize) -> Option<Vec<u8>> {
    if input.get(off) != Some(&b'"') {
        return None;
    }
    let mut i = off + 1;
    loop {
        let c = *input.get(i)?;
        if c == b'"' {
            return Some(input[off..=i].to_vec());
        }
        if c == b'\\' {
            let n = *input.get(i + 1)?;
            match n {
                b'"' | b'b' | b'f' | b'n' | b'r' | b't' | b'/' | b'\\' => {
                    i += 2;
                }
                b'u' => {
                    // require 4 hex digits
                    for k in 0..4 {
                        let h = *input.get(i + 2 + k)?;
                        if !h.is_ascii_hexdigit() {
                            return None;
                        }
                    }
                    i += 6;
                }
                _ => return None,
            }
        } else if c <= 0x1f {
            // [^\0-\x1f\\"] excludes control chars
            return None;
        } else {
            i += 1;
        }
    }
}

/// Rules 4-9: single literal char.
fn match_char(input: &[u8], off: usize, ch: u8) -> Option<Vec<u8>> {
    if input.get(off) == Some(&ch) {
        Some(vec![ch])
    } else {
        None
    }
}

/// Rules 10-12: keyword followed by a `\b` boundary.
fn match_keyword(input: &[u8], off: usize, kw: &[u8]) -> Option<Vec<u8>> {
    if input.len() < off + kw.len() || &input[off..off + kw.len()] != kw {
        return None;
    }
    let after = off + kw.len();
    if after < input.len() && is_word_byte(input[after]) {
        return None;
    }
    Some(kw.to_vec())
}

/// Rule 13: /\G$/ — matches the empty string at end of input (or before a trailing final newline,
/// per PCRE `$` without the `m` modifier).
fn match_end(input: &[u8], off: usize) -> Option<Vec<u8>> {
    if off == input.len() {
        return Some(Vec::new());
    }
    if off == input.len() - 1 && input[off] == b'\n' {
        return Some(Vec::new());
    }
    None
}

/// Rules 14-16: multi-char literal.
fn match_literal(input: &[u8], off: usize, lit: &[u8]) -> Option<Vec<u8>> {
    if input.len() >= off + lit.len() && &input[off..off + lit.len()] == lit {
        Some(lit.to_vec())
    } else {
        None
    }
}

/// Rule 17: /\G./ — any single character except newline.
fn match_any_char(input: &[u8], off: usize) -> Option<Vec<u8>> {
    match input.get(off) {
        Some(&b'\n') | None => None,
        Some(&c) => Some(vec![c]),
    }
}

fn substr_bytes(input: &[u8], start: usize, len: usize) -> Vec<u8> {
    if start >= input.len() {
        return Vec::new();
    }
    let end = (start + len).min(input.len());
    input[start..end].to_vec()
}

fn str_replace_nl(input: &[u8]) -> String {
    let filtered: Vec<u8> = input.iter().copied().filter(|&b| b != b'\n').collect();
    String::from_utf8_lossy(&filtered).into_owned()
}

fn split_bytes(input: &[u8], sep: u8) -> Vec<&[u8]> {
    input.split(|&b| b == sep).collect()
}

fn byte_count(input: &[u8], b: u8) -> usize {
    input.iter().filter(|&&x| x == b).count()
}

fn find_byte_from(input: &[u8], b: u8, from: usize) -> Option<usize> {
    if from > input.len() {
        return None;
    }
    input[from..].iter().position(|&x| x == b).map(|p| p + from)
}
