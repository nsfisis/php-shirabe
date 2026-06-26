//! ref: composer/vendor/seld/jsonlint/src/Seld/JsonLint/JsonParser.php

use std::collections::HashMap;

use indexmap::IndexMap;
use shirabe_php_shim::PhpMixed;

use super::DuplicateKeyException;
use super::ParsingException;
use super::lexer::{Lexer, YylLoc};

/// Semantic value held on the parser value stack ($vstack). Most values are JSON values (PhpMixed),
/// but object members are represented as a `[key, value]` pair while being reduced.
#[derive(Debug, Clone)]
enum SemValue {
    Null,
    Value(PhpMixed),
    /// Raw token text from the lexer (yytext), before semantic actions transform it.
    Text(String),
    /// A `[key, value]` member pair (production 15).
    Member(String, PhpMixed),
}

impl SemValue {
    fn into_value(self) -> PhpMixed {
        match self {
            SemValue::Null => PhpMixed::Null,
            SemValue::Value(v) => v,
            SemValue::Text(s) => PhpMixed::String(s),
            SemValue::Member(_, _) => PhpMixed::Null,
        }
    }
}

/// Result of `performAction`: the new semantic value plus an optional early-return (production 6).
enum ActionResult {
    /// `$$` was assigned but parsing continues (PHP returns `[token, new Undefined()]`).
    Continue(SemValue),
    /// Production 6 returned the accumulated value directly.
    Return(PhpMixed),
}

#[derive(Debug)]
pub struct JsonParser {
    productions_: HashMap<i64, (i64, i64)>,
    terminals_: HashMap<i64, &'static str>,
    table: Vec<HashMap<i64, TableAction>>,
    default_actions: HashMap<i64, (i64, i64)>,
}

/// Per-parse mutable state. PHP keeps `$flags`/`$stack`/`$vstack`/`$lstack` on the parser instance;
/// holding them here keeps `parse()` `&self` so the parser can be shared/reused.
#[derive(Debug)]
struct ParseState {
    flags: u32,
    stack: Vec<i64>,
    vstack: Vec<SemValue>,
    lstack: Vec<YylLoc>,
}

/// An entry in the parse table: either a shift/reduce action pair `[type, arg]` or a goto state.
#[derive(Debug, Clone, Copy)]
enum TableAction {
    /// `array(type, arg)` — type 1 = shift, 2 = reduce, 3 = accept.
    Action(i64, i64),
    /// A bare integer = goto state (used for nonterminals).
    Goto(i64),
}

impl Default for JsonParser {
    fn default() -> Self {
        Self::new()
    }
}

impl JsonParser {
    pub const DETECT_KEY_CONFLICTS: u32 = 1;
    pub const ALLOW_DUPLICATE_KEYS: u32 = 2;
    pub const PARSE_TO_ASSOC: u32 = 4;
    pub const ALLOW_COMMENTS: u32 = 8;
    pub const ALLOW_DUPLICATE_KEYS_TO_ARRAY: u32 = 16;

    pub fn new() -> Self {
        let productions_: HashMap<i64, (i64, i64)> = [
            (1, (3, 1)),
            (2, (5, 1)),
            (3, (7, 1)),
            (4, (9, 1)),
            (5, (9, 1)),
            (6, (12, 2)),
            (7, (13, 1)),
            (8, (13, 1)),
            (9, (13, 1)),
            (10, (13, 1)),
            (11, (13, 1)),
            (12, (13, 1)),
            (13, (15, 2)),
            (14, (15, 3)),
            (15, (20, 3)),
            (16, (19, 1)),
            (17, (19, 3)),
            (18, (16, 2)),
            (19, (16, 3)),
            (20, (25, 1)),
            (21, (25, 3)),
        ]
        .into_iter()
        .collect();

        let terminals_: HashMap<i64, &'static str> = [
            (2, "error"),
            (4, "STRING"),
            (6, "NUMBER"),
            (8, "NULL"),
            (10, "TRUE"),
            (11, "FALSE"),
            (14, "EOF"),
            (17, "{"),
            (18, "}"),
            (21, ":"),
            (22, ","),
            (23, "["),
            (24, "]"),
        ]
        .into_iter()
        .collect();

        let table = build_table();

        let default_actions: HashMap<i64, (i64, i64)> = [(16, (2, 6))].into_iter().collect();

        Self {
            productions_,
            terminals_,
            table,
            default_actions,
        }
    }

    /// ref: JsonParser::lint() — returns null on success, ParsingException on failure.
    pub fn lint(&mut self, input: &str) -> Option<ParsingException> {
        match self.parse(input, 0) {
            Ok(_) => None,
            Err(e) => match e.downcast::<ParsingException>() {
                Ok(pe) => Some(pe),
                // PHP only catches ParsingException; DuplicateKeyException (a subclass) is also
                // caught, but lint() uses no flags so DETECT_KEY_CONFLICTS never triggers.
                Err(other) => Some(ParsingException::new(other.to_string(), Default::default())),
            },
        }
    }

    pub fn parse(&self, input: &str, flags: u32) -> anyhow::Result<PhpMixed> {
        if (flags & Self::ALLOW_DUPLICATE_KEYS_TO_ARRAY != 0)
            && (flags & Self::ALLOW_DUPLICATE_KEYS != 0)
        {
            // PHP throws \InvalidArgumentException (uncaught fatal).
            panic!(
                "Only one of ALLOW_DUPLICATE_KEYS and ALLOW_DUPLICATE_KEYS_TO_ARRAY can be used, you passed in both."
            );
        }

        self.fail_on_bom(input)?;

        let mut state_ = ParseState {
            flags,
            stack: vec![0],
            vstack: vec![SemValue::Null],
            lstack: Vec::new(),
        };

        let mut yytext = String::new();
        let mut yylineno: i64 = 0;
        let mut yyleng: i64 = 0;
        let mut recovering: i64 = 0;

        let mut lexer = Lexer::new(flags);
        lexer.set_input(input);

        let mut yyloc = lexer.yylloc.clone();
        state_.lstack.push(yyloc.clone());

        let mut symbol: Option<i64> = None;
        let mut pre_error_symbol: Option<i64> = None;
        let err_str: Option<String> = None;

        loop {
            let mut state = state_.stack[state_.stack.len() - 1];

            // use default actions if available
            let mut action: Option<(i64, i64)> = if let Some(a) = self.default_actions.get(&state) {
                Some(*a)
            } else {
                if symbol.is_none() {
                    symbol = Some(lexer.lex()?);
                }
                self.table[state as usize]
                    .get(&symbol.unwrap())
                    .and_then(|a| match *a {
                        TableAction::Action(t, arg) => Some((t, arg)),
                        TableAction::Goto(_) => None,
                    })
            };

            // handle parse error
            if action.is_none() || action.unwrap().0 == 0 {
                let sym = symbol.expect("symbol should be set");
                if recovering == 0 {
                    // PHP iterates table entries in insertion order; mimic by sorting on symbol id.
                    let mut expected_pairs: Vec<(i64, String)> = self.table[state as usize]
                        .keys()
                        .filter_map(|p| {
                            self.terminals_
                                .get(p)
                                .filter(|_| *p > 2)
                                .map(|name| (*p, format!("'{name}'")))
                        })
                        .collect();
                    expected_pairs.sort_by_key(|(p, _)| *p);
                    let expected: Vec<String> =
                        expected_pairs.into_iter().map(|(_, s)| s).collect();

                    let mut message: Option<String> = None;
                    let match_first = lexer.match_.first().copied();
                    if expected.iter().any(|e| e == "'STRING'")
                        && matches!(match_first, Some(b'"') | Some(b'\''))
                    {
                        let mut msg = String::from("Invalid string");
                        if match_first == Some(b'\'') {
                            msg.push_str(
                                ", it appears you used single quotes instead of double quotes",
                            );
                        } else if let Some(found) =
                            detect_unescaped_backslash(&lexer.get_full_upcoming_input())
                        {
                            msg.push_str(", it appears you have an unescaped backslash at: ");
                            msg.push_str(&found);
                        } else if detect_unterminated_string(&lexer.get_full_upcoming_input()) {
                            msg.push_str(", it appears you forgot to terminate a string, or attempted to write a multiline string which is invalid");
                        }
                        message = Some(msg);
                    }

                    let mut err_str = format!("Parse error on line {}:\n", yylineno + 1);
                    err_str.push_str(&lexer.show_position());
                    err_str.push('\n');
                    if let Some(msg) = &message {
                        err_str.push_str(msg);
                    } else {
                        err_str.push_str(if expected.len() > 1 {
                            "Expected one of: "
                        } else {
                            "Expected: "
                        });
                        err_str.push_str(&expected.join(", "));
                    }

                    let past = lexer.get_past_input();
                    let trimmed = trim_bytes(&past);
                    if trimmed.last() == Some(&b',') {
                        err_str.push_str(" - It appears you have an extra trailing comma");
                    }

                    let token = if let Some(name) = self.terminals_.get(&sym) {
                        super::parsing_exception::ParsingExceptionToken::Name(name.to_string())
                    } else {
                        super::parsing_exception::ParsingExceptionToken::Symbol(sym)
                    };
                    let details = super::parsing_exception::ParsingExceptionDetails {
                        text: Some(String::from_utf8_lossy(&lexer.match_).into_owned()),
                        token: Some(token),
                        line: Some(lexer.yylineno),
                        loc: Some(yyloc_to_loc(&yyloc)),
                        expected: Some(expected.clone()),
                    };
                    return Err(ParsingException::new(err_str, details).into());
                }

                // recovery path (recovering != 0). Not reachable for this grammar because the error
                // is always reported (and thrown) above on the first failure; kept for fidelity.
                if recovering == 3 {
                    if sym == Lexer::EOF {
                        return Err(ParsingException::new(
                            err_str.clone().unwrap_or_else(|| "Parsing halted.".into()),
                            Default::default(),
                        )
                        .into());
                    }
                    yyleng = lexer.yyleng;
                    yytext = String::from_utf8_lossy(&lexer.yytext).into_owned();
                    yylineno = lexer.yylineno;
                    yyloc = lexer.yylloc.clone();
                    symbol = Some(lexer.lex()?);
                }

                loop {
                    if self.table[state as usize].contains_key(&Lexer::T_ERROR) {
                        break;
                    }
                    if state == 0 {
                        return Err(ParsingException::new(
                            err_str.clone().unwrap_or_else(|| "Parsing halted.".into()),
                            Default::default(),
                        )
                        .into());
                    }
                    state_.pop_stack(1);
                    state = state_.stack[state_.stack.len() - 1];
                }

                pre_error_symbol = symbol;
                symbol = Some(Lexer::T_ERROR);
                state = state_.stack[state_.stack.len() - 1];
                action = self.table[state as usize]
                    .get(&Lexer::T_ERROR)
                    .and_then(|a| match *a {
                        TableAction::Action(t, arg) => Some((t, arg)),
                        TableAction::Goto(_) => None,
                    });
                if action.is_none() {
                    panic!("No table value found for {} => {}", state, Lexer::T_ERROR);
                }
                recovering = 3;
            }

            let action = action.unwrap();
            match action.0 {
                1 => {
                    // shift
                    let sym = symbol.expect("symbol should be set");
                    state_.stack.push(sym);
                    state_.vstack.push(SemValue::Text(
                        String::from_utf8_lossy(&lexer.yytext).into_owned(),
                    ));
                    state_.lstack.push(lexer.yylloc.clone());
                    state_.stack.push(action.1);
                    symbol = None;
                    if pre_error_symbol.is_none() {
                        yyleng = lexer.yyleng;
                        yytext = String::from_utf8_lossy(&lexer.yytext).into_owned();
                        yylineno = lexer.yylineno;
                        yyloc = lexer.yylloc.clone();
                        if recovering > 0 {
                            recovering -= 1;
                        }
                    } else {
                        symbol = pre_error_symbol;
                        pre_error_symbol = None;
                    }
                }
                2 => {
                    // reduce
                    let prod = self.productions_[&action.1];
                    let len = prod.1;

                    let position = YylLoc {
                        first_line: state_.lstack[state_.lstack.len() - (len.max(1) as usize)]
                            .first_line,
                        last_line: state_.lstack[state_.lstack.len() - 1].last_line,
                        first_column: state_.lstack[state_.lstack.len() - (len.max(1) as usize)]
                            .first_column,
                        last_column: state_.lstack[state_.lstack.len() - 1].last_column,
                    };

                    match state_.perform_action(&yytext, yyleng, yylineno, action.1)? {
                        ActionResult::Return(v) => return Ok(v),
                        ActionResult::Continue(new_token) => {
                            if len != 0 {
                                state_.pop_stack(len);
                            }
                            state_.stack.push(prod.0);
                            state_.vstack.push(new_token);
                            state_.lstack.push(position);
                            let goto_state = match self.table
                                [state_.stack[state_.stack.len() - 2] as usize]
                                .get(&state_.stack[state_.stack.len() - 1])
                            {
                                Some(TableAction::Goto(s)) => *s,
                                Some(TableAction::Action(_, arg)) => *arg,
                                None => panic!("No goto state"),
                            };
                            state_.stack.push(goto_state);
                        }
                    }
                }
                3 => {
                    // accept
                    return Ok(PhpMixed::Bool(true));
                }
                _ => {}
            }
        }
    }

    fn fail_on_bom(&self, input: &str) -> Result<(), ParsingException> {
        let bom = [0xEF, 0xBB, 0xBF];
        if input.len() >= 3 && input.as_bytes()[0..3] == bom {
            return Err(ParsingException::new(
                "BOM detected, make sure your input does not include a Unicode Byte-Order-Mark"
                    .to_string(),
                Default::default(),
            ));
        }
        Ok(())
    }
}

impl ParseState {
    /// ref: JsonParser::performAction
    fn perform_action(
        &mut self,
        _yytext: &str,
        _yyleng: i64,
        yylineno: i64,
        yystate: i64,
    ) -> anyhow::Result<ActionResult> {
        // `$len = count($vstack) - 1` indexes the top semantic value.
        let len = self.vstack.len() - 1;
        // default: $$ = $1, the value at (count - production_len) ; PHP captures `$currentToken`
        // separately, but for productions we rebuild explicitly below, defaulting to the top value.
        let mut token: SemValue = self.vstack[len].clone();

        match yystate {
            1 => {
                // string interpolation of escape sequences
                let raw = match &self.vstack[len] {
                    SemValue::Text(s) => s.clone(),
                    other => semvalue_string(other),
                };
                token = SemValue::Value(PhpMixed::String(string_interpolation_all(&raw)));
            }
            2 => {
                let raw = match &self.vstack[len] {
                    SemValue::Text(s) => s.clone(),
                    other => semvalue_string(other),
                };
                let v = if raw.contains('e') || raw.contains('E') {
                    PhpMixed::Float(php_floatval(&raw))
                } else if !raw.contains('.') {
                    PhpMixed::Int(php_intval(&raw))
                } else {
                    PhpMixed::Float(php_floatval(&raw))
                };
                token = SemValue::Value(v);
            }
            3 => token = SemValue::Value(PhpMixed::Null),
            4 => token = SemValue::Value(PhpMixed::Bool(true)),
            5 => token = SemValue::Value(PhpMixed::Bool(false)),
            6 => {
                let v = self.vstack[len - 1].clone().into_value();
                return Ok(ActionResult::Return(v));
            }
            13 => {
                let v = if self.flags & JsonParser::PARSE_TO_ASSOC != 0 {
                    PhpMixed::Array(IndexMap::new())
                } else {
                    PhpMixed::Object(IndexMap::new())
                };
                token = SemValue::Value(v);
            }
            14 => {
                token = SemValue::Value(self.vstack[len - 1].clone().into_value());
            }
            15 => {
                let key = match self.vstack[len - 2].clone() {
                    SemValue::Value(PhpMixed::String(s)) => s,
                    SemValue::Text(s) => s,
                    other => semvalue_string(&other),
                };
                let value = self.vstack[len].clone().into_value();
                token = SemValue::Member(key, value);
            }
            16 => {
                let (property, value) = match self.vstack[len].clone() {
                    SemValue::Member(k, v) => (k, v),
                    _ => panic!("expected member pair"),
                };
                let mut map = IndexMap::new();
                map.insert(property, value);
                token = if self.flags & JsonParser::PARSE_TO_ASSOC != 0 {
                    SemValue::Value(PhpMixed::Array(map))
                } else {
                    SemValue::Value(PhpMixed::Object(map))
                };
            }
            17 => {
                let (key, value) = match self.vstack[len].clone() {
                    SemValue::Member(k, v) => (k, v),
                    _ => panic!("expected member pair"),
                };
                let mut container = self.vstack[len - 2].clone().into_value();
                let map = match &mut container {
                    PhpMixed::Array(m) | PhpMixed::Object(m) => m,
                    _ => panic!("expected object/array container"),
                };

                if (self.flags & JsonParser::DETECT_KEY_CONFLICTS != 0) && map.contains_key(&key) {
                    // PHP inserts $this->lexer->showPosition() here; the lexer is owned by parse()
                    // and not reachable from this method, so the position line is omitted. Only the
                    // "Duplicate key" body is read by ConfigValidator (DETECT_KEY_CONFLICTS).
                    let mut err_str = format!("Parse error on line {}:\n", yylineno + 1);
                    err_str.push('\n');
                    err_str.push_str(&format!("Duplicate key: {key}"));
                    let mut details: IndexMap<String, PhpMixed> = IndexMap::new();
                    // PHP details: array('line' => $yylineno+1); the 'key' entry is read by
                    // ConfigValidator, so include it as well (PHP DuplicateKeyException stores the
                    // key separately, but Shirabe's details map carries both).
                    details.insert("key".to_string(), PhpMixed::String(key.clone()));
                    details.insert("line".to_string(), PhpMixed::Int(yylineno + 1));
                    return Err(DuplicateKeyException {
                        message: err_str,
                        code: 0,
                        details,
                    }
                    .into());
                }

                if (self.flags & JsonParser::ALLOW_DUPLICATE_KEYS != 0) && map.contains_key(&key) {
                    let mut duplicate_count = 1;
                    let mut duplicate_key;
                    loop {
                        duplicate_key = format!("{key}.{duplicate_count}");
                        duplicate_count += 1;
                        if !map.contains_key(&duplicate_key) {
                            break;
                        }
                    }
                    map.insert(duplicate_key, value);
                } else {
                    // ALLOW_DUPLICATE_KEYS_TO_ARRAY path omitted: requires `__duplicates__` nesting;
                    // unused by Composer's lint/parse defaults.
                    map.insert(key, value);
                }
                token = SemValue::Value(container);
            }
            18 => token = SemValue::Value(PhpMixed::List(Vec::new())),
            19 => {
                token = SemValue::Value(self.vstack[len - 1].clone().into_value());
            }
            20 => {
                let v = self.vstack[len].clone().into_value();
                token = SemValue::Value(PhpMixed::List(vec![v]));
            }
            21 => {
                let mut list = self.vstack[len - 2].clone().into_value();
                let v = self.vstack[len].clone().into_value();
                if let PhpMixed::List(items) = &mut list {
                    items.push(v);
                } else {
                    panic!("expected list container");
                }
                token = SemValue::Value(list);
            }
            _ => {}
        }

        Ok(ActionResult::Continue(token))
    }

    fn pop_stack(&mut self, n: i64) {
        let n = n as usize;
        let new_stack_len = self.stack.len() - 2 * n;
        self.stack.truncate(new_stack_len);
        let new_v = self.vstack.len() - n;
        self.vstack.truncate(new_v);
        let new_l = self.lstack.len() - n;
        self.lstack.truncate(new_l);
    }
}

fn yyloc_to_loc(loc: &YylLoc) -> super::parsing_exception::ParsingExceptionLoc {
    super::parsing_exception::ParsingExceptionLoc {
        first_line: loc.first_line,
        first_column: loc.first_column,
        last_line: loc.last_line,
        last_column: loc.last_column,
    }
}

fn semvalue_string(v: &SemValue) -> String {
    match v {
        SemValue::Text(s) => s.clone(),
        SemValue::Value(PhpMixed::String(s)) => s.clone(),
        _ => String::new(),
    }
}

/// ref: JsonParser::stringInterpolation, applied to every escape sequence in the string
/// (PHP uses preg_replace_callback with '{(?:\\["bfnrt/\\]|\\u[a-fA-F0-9]{4})}').
fn string_interpolation_all(input: &str) -> String {
    let bytes = input.as_bytes();
    let mut out: Vec<u8> = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'\\' && i + 1 < bytes.len() {
            let n = bytes[i + 1];
            match n {
                b'\\' => {
                    out.push(b'\\');
                    i += 2;
                    continue;
                }
                b'"' => {
                    out.push(b'"');
                    i += 2;
                    continue;
                }
                b'b' => {
                    out.push(8);
                    i += 2;
                    continue;
                }
                b'f' => {
                    out.push(12);
                    i += 2;
                    continue;
                }
                b'n' => {
                    out.push(b'\n');
                    i += 2;
                    continue;
                }
                b'r' => {
                    out.push(b'\r');
                    i += 2;
                    continue;
                }
                b't' => {
                    out.push(b'\t');
                    i += 2;
                    continue;
                }
                b'/' => {
                    out.push(b'/');
                    i += 2;
                    continue;
                }
                b'u' if i + 5 < bytes.len()
                    && bytes[i + 2..i + 6].iter().all(|b| b.is_ascii_hexdigit()) =>
                {
                    let hex = &input[i + 2..i + 6];
                    if let Ok(cp) = u32::from_str_radix(hex, 16)
                        && let Some(c) = char::from_u32(cp)
                    {
                        let mut buf = [0u8; 4];
                        out.extend_from_slice(c.encode_utf8(&mut buf).as_bytes());
                    }
                    i += 6;
                    continue;
                }
                _ => {}
            }
        }
        out.push(bytes[i]);
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

/// PHP floatval for a JSON number literal.
fn php_floatval(s: &str) -> f64 {
    s.parse::<f64>().unwrap_or(0.0)
}

/// PHP intval for a JSON integer literal.
fn php_intval(s: &str) -> i64 {
    // PHP intval parses leading integer portion; JSON numbers here are already valid integers.
    s.parse::<i64>().unwrap_or_else(|_| {
        // overflow fallback: PHP would clamp/convert, but valid lexed ints rarely overflow i64.
        if s.starts_with('-') {
            i64::MIN
        } else {
            i64::MAX
        }
    })
}

/// ref: preg_match('{".+?(\\[^"bfnrt/\\u](...)?)}', $fullUpcomingInput) — returns the captured
/// group 1 (the unescaped backslash plus up to 3 following chars) if found.
fn detect_unescaped_backslash(input: &[u8]) -> Option<String> {
    // Pattern: a `"`, then `.+?` (at least one char, non-greedy), then a `\` that is NOT followed by
    // a valid escape char (one of "bfnrt/\\u), then optionally up to 3 more chars (the `(...)?`).
    // `.` does not match newline. We scan for the first `"`, then for the first qualifying backslash
    // after at least one character.
    let valid_escape = |b: u8| {
        matches!(
            b,
            b'"' | b'b' | b'f' | b'n' | b'r' | b't' | b'/' | b'\\' | b'u'
        )
    };
    let n = input.len();
    // find first `"`
    let mut q = 0;
    while q < n {
        if input[q] == b'"' && input[q] != b'\n' {
            break;
        }
        q += 1;
    }
    if q >= n || input[q] != b'"' {
        return None;
    }
    // need at least one char (.+?) after the quote, then the backslash.
    // The `.+?` must match at least one non-newline char before the backslash.
    let mut i = q + 1;
    // ensure at least one char consumed by .+? : the backslash cannot be the char immediately
    // following the quote? `.+?` is greedy-minimal but must consume >=1, and `.` excludes newline.
    let mut consumed_one = false;
    while i < n {
        let c = input[i];
        if c == b'\n' {
            // `.` cannot cross a newline; PCRE `.+?` would stop — no match across lines here.
            return None;
        }
        if c == b'\\' && consumed_one {
            let next = input.get(i + 1).copied();
            let bad = match next {
                None => true, // `[^...]` requires a char; if none, no match
                Some(b) => !valid_escape(b) && b != b'\n',
            };
            if next.is_some() && bad {
                // capture group 1: the `\`, the disallowed char, then up to 3 more (...)?
                let start = i;
                let mut end = i + 2; // backslash + the [^...] char
                // (...)? = exactly 3 chars if present
                let mut extra = 0;
                let mut k = end;
                while extra < 3 && k < n && input[k] != b'\n' {
                    k += 1;
                    extra += 1;
                }
                if extra == 3 {
                    end = k;
                }
                return Some(String::from_utf8_lossy(&input[start..end]).into_owned());
            }
        }
        consumed_one = true;
        i += 1;
    }
    None
}

/// ref: preg_match('{"(?:[^"]+|\\")*$}m', $fullUpcomingInput) — a `"` followed by string body that
/// reaches a line end without a closing quote (multiline, with `m` so `$` is end-of-line).
fn detect_unterminated_string(input: &[u8]) -> bool {
    // Find a `"`; from there consume `[^"]+ | \"` repeatedly; succeed if we reach end-of-line/string
    // without hitting an unescaped closing `"`.
    let n = input.len();
    let mut start = 0;
    while start < n {
        if input[start] == b'"' {
            // try matching from this quote
            let mut i = start + 1;
            loop {
                if i >= n {
                    return true; // reached end of string ($ at end)
                }
                let c = input[i];
                if c == b'\n' {
                    return true; // $ matches before newline in multiline mode
                }
                if c == b'\\' && input.get(i + 1) == Some(&b'"') {
                    i += 2; // \" alternative
                    continue;
                }
                if c == b'"' {
                    break; // closing quote: this start fails, try next `"`
                }
                i += 1; // [^"]+
            }
        }
        start += 1;
    }
    false
}

fn trim_bytes(input: &[u8]) -> Vec<u8> {
    // PHP trim() default strips " \t\n\r\0\x0B"
    let is_trim = |b: u8| matches!(b, b' ' | b'\t' | b'\n' | b'\r' | 0 | 0x0b);
    let mut start = 0;
    let mut end = input.len();
    while start < end && is_trim(input[start]) {
        start += 1;
    }
    while end > start && is_trim(input[end - 1]) {
        end -= 1;
    }
    input[start..end].to_vec()
}

/// Builds the LALR parse `table`. Indexed by state id (0..=31).
fn build_table() -> Vec<HashMap<i64, TableAction>> {
    use TableAction::{Action, Goto};
    let mut t: Vec<HashMap<i64, TableAction>> = (0..32).map(|_| HashMap::new()).collect();

    let mut set = |state: usize, entries: &[(i64, TableAction)]| {
        for (sym, act) in entries {
            t[state].insert(*sym, *act);
        }
    };

    set(
        0,
        &[
            (3, Goto(5)),
            (4, Action(1, 12)),
            (5, Goto(6)),
            (6, Action(1, 13)),
            (7, Goto(3)),
            (8, Action(1, 9)),
            (9, Goto(4)),
            (10, Action(1, 10)),
            (11, Action(1, 11)),
            (12, Goto(1)),
            (13, Goto(2)),
            (15, Goto(7)),
            (16, Goto(8)),
            (17, Action(1, 14)),
            (23, Action(1, 15)),
        ],
    );
    set(1, &[(1, Action(3, 0))]);
    set(2, &[(14, Action(1, 16))]);
    set(
        3,
        &[
            (14, Action(2, 7)),
            (18, Action(2, 7)),
            (22, Action(2, 7)),
            (24, Action(2, 7)),
        ],
    );
    set(
        4,
        &[
            (14, Action(2, 8)),
            (18, Action(2, 8)),
            (22, Action(2, 8)),
            (24, Action(2, 8)),
        ],
    );
    set(
        5,
        &[
            (14, Action(2, 9)),
            (18, Action(2, 9)),
            (22, Action(2, 9)),
            (24, Action(2, 9)),
        ],
    );
    set(
        6,
        &[
            (14, Action(2, 10)),
            (18, Action(2, 10)),
            (22, Action(2, 10)),
            (24, Action(2, 10)),
        ],
    );
    set(
        7,
        &[
            (14, Action(2, 11)),
            (18, Action(2, 11)),
            (22, Action(2, 11)),
            (24, Action(2, 11)),
        ],
    );
    set(
        8,
        &[
            (14, Action(2, 12)),
            (18, Action(2, 12)),
            (22, Action(2, 12)),
            (24, Action(2, 12)),
        ],
    );
    set(
        9,
        &[
            (14, Action(2, 3)),
            (18, Action(2, 3)),
            (22, Action(2, 3)),
            (24, Action(2, 3)),
        ],
    );
    set(
        10,
        &[
            (14, Action(2, 4)),
            (18, Action(2, 4)),
            (22, Action(2, 4)),
            (24, Action(2, 4)),
        ],
    );
    set(
        11,
        &[
            (14, Action(2, 5)),
            (18, Action(2, 5)),
            (22, Action(2, 5)),
            (24, Action(2, 5)),
        ],
    );
    set(
        12,
        &[
            (14, Action(2, 1)),
            (18, Action(2, 1)),
            (21, Action(2, 1)),
            (22, Action(2, 1)),
            (24, Action(2, 1)),
        ],
    );
    set(
        13,
        &[
            (14, Action(2, 2)),
            (18, Action(2, 2)),
            (22, Action(2, 2)),
            (24, Action(2, 2)),
        ],
    );
    set(
        14,
        &[
            (3, Goto(20)),
            (4, Action(1, 12)),
            (18, Action(1, 17)),
            (19, Goto(18)),
            (20, Goto(19)),
        ],
    );
    set(
        15,
        &[
            (3, Goto(5)),
            (4, Action(1, 12)),
            (5, Goto(6)),
            (6, Action(1, 13)),
            (7, Goto(3)),
            (8, Action(1, 9)),
            (9, Goto(4)),
            (10, Action(1, 10)),
            (11, Action(1, 11)),
            (13, Goto(23)),
            (15, Goto(7)),
            (16, Goto(8)),
            (17, Action(1, 14)),
            (23, Action(1, 15)),
            (24, Action(1, 21)),
            (25, Goto(22)),
        ],
    );
    set(16, &[(1, Action(2, 6))]);
    set(
        17,
        &[
            (14, Action(2, 13)),
            (18, Action(2, 13)),
            (22, Action(2, 13)),
            (24, Action(2, 13)),
        ],
    );
    set(18, &[(18, Action(1, 24)), (22, Action(1, 25))]);
    set(19, &[(18, Action(2, 16)), (22, Action(2, 16))]);
    set(20, &[(21, Action(1, 26))]);
    set(
        21,
        &[
            (14, Action(2, 18)),
            (18, Action(2, 18)),
            (22, Action(2, 18)),
            (24, Action(2, 18)),
        ],
    );
    set(22, &[(22, Action(1, 28)), (24, Action(1, 27))]);
    set(23, &[(22, Action(2, 20)), (24, Action(2, 20))]);
    set(
        24,
        &[
            (14, Action(2, 14)),
            (18, Action(2, 14)),
            (22, Action(2, 14)),
            (24, Action(2, 14)),
        ],
    );
    set(25, &[(3, Goto(20)), (4, Action(1, 12)), (20, Goto(29))]);
    set(
        26,
        &[
            (3, Goto(5)),
            (4, Action(1, 12)),
            (5, Goto(6)),
            (6, Action(1, 13)),
            (7, Goto(3)),
            (8, Action(1, 9)),
            (9, Goto(4)),
            (10, Action(1, 10)),
            (11, Action(1, 11)),
            (13, Goto(30)),
            (15, Goto(7)),
            (16, Goto(8)),
            (17, Action(1, 14)),
            (23, Action(1, 15)),
        ],
    );
    set(
        27,
        &[
            (14, Action(2, 19)),
            (18, Action(2, 19)),
            (22, Action(2, 19)),
            (24, Action(2, 19)),
        ],
    );
    set(
        28,
        &[
            (3, Goto(5)),
            (4, Action(1, 12)),
            (5, Goto(6)),
            (6, Action(1, 13)),
            (7, Goto(3)),
            (8, Action(1, 9)),
            (9, Goto(4)),
            (10, Action(1, 10)),
            (11, Action(1, 11)),
            (13, Goto(31)),
            (15, Goto(7)),
            (16, Goto(8)),
            (17, Action(1, 14)),
            (23, Action(1, 15)),
        ],
    );
    set(29, &[(18, Action(2, 17)), (22, Action(2, 17))]);
    set(30, &[(18, Action(2, 15)), (22, Action(2, 15))]);
    set(31, &[(22, Action(2, 21)), (24, Action(2, 21))]);

    t
}
