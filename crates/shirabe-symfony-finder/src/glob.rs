//! ref: composer/vendor/symfony/finder/Glob.php

// Regex pattern compatibility:
// PHP's Glob::toRegex builds its regex in a single character loop and emits PCRE-only
// constructs: the look-ahead `(?=[^\.])` enforcing the strict-leading-dot rule, and a
// possessive `[^/]++` inside the `/**/` construct. The regex crate supports neither, so
// instead of a verbatim port of the loop, the glob is first tokenized (mirroring the
// loop's dispatch exactly) and every no-dot constraint is then resolved by recursive
// union expansion, producing a regex-crate-compatible pattern that matches the same
// inputs:
//
//   (?=[^.])c        => c                       (branch is unmatchable when c is `\.`)
//   (?=[^.])[^/]     => [^/.]
//   (?=[^.])[abc]    => [[abc]&&[^.]]           (character class intersection)
//   (?=[^.])[^/]*R   => (?:[^/.][^/]*R | expand((?=[^.])R))
//   (?=[^.])(a|b)R   => (?:expand((?=[^.])aR) | expand((?=[^.])bR))
//   (?=[^.])$        => branch dropped          (no next character can exist)
//   (?=[^.])(?:$|/)  => /                       (dir-boundary variant only)
//   [^/]++           => [^/]+                   (the regex crate never backtracks)
//
// Unmatchable branches are dropped; a wholly unmatchable glob renders as `\b\B`, which
// compiles and never matches, just like PHP's own output for such globs.

#[derive(Debug)]
pub struct Glob;

#[derive(Debug, Clone, Copy)]
struct Ctx {
    strict_leading_dot: bool,
    strict_wildcard_slash: bool,
}

#[derive(Debug, Clone, Copy)]
enum Terminal {
    /// The `$` anchor of `to_regex` follows the body.
    EndAnchor,
    /// The `(?:$|/)` suffix of `to_regex_dir_boundary` ends the body.
    EndOrSlash,
}

#[derive(Debug, Clone)]
enum Token {
    /// A literal regex piece for one glob character, escaped as needed.
    Text(String),
    /// An unescaped `*`.
    Star,
    /// An unescaped `?`.
    Question,
    /// A `/**/` (or trailing `/**`) segment, leading slash included.
    Globstar { trailing_slash_optional: bool },
    /// An `{a,b}` alternation. `closed` is false when the `}` is missing.
    Group {
        alternatives: Vec<Vec<Token>>,
        closed: bool,
    },
    /// The position of PHP's `(?=[^\.])` look-ahead.
    NoDot,
    /// End of the pattern.
    End(Terminal),
}

#[derive(Debug)]
enum Out {
    /// Fully rendered.
    Ok(String),
    /// Cannot match anything; the branch must be dropped.
    Dead,
    /// A no-dot constraint reached the end of an isolated sub-sequence (e.g. `{a/,b}`
    /// puts a look-ahead right before the closing brace); the caller must splice in the
    /// following context and re-render.
    Escapes,
}

impl Glob {
    pub fn to_regex(glob: &str, strict_leading_dot: bool, strict_wildcard_slash: bool) -> String {
        format!(
            "#^{}$#",
            expand(
                glob,
                strict_leading_dot,
                strict_wildcard_slash,
                Terminal::EndAnchor
            )
        )
    }

    /// Not part of Symfony's Glob. Composer's BaseExcludeFilter derives its patterns
    /// from toRegex output by stripping the delimiters and anchors and appending the
    /// `(?=$|/)` dir-boundary look-ahead. Here that suffix has to take part in the
    /// union expansion — when a trailing `*` matches zero characters, the no-dot
    /// constraint falls onto the boundary itself — so it cannot be appended by the
    /// caller after the fact. Returns the undelimited, unanchored body with the
    /// boundary woven in as the consuming `(?:$|/)` (equivalent under boolean
    /// matching, since nothing follows it).
    pub fn to_regex_dir_boundary(
        glob: &str,
        strict_leading_dot: bool,
        strict_wildcard_slash: bool,
    ) -> String {
        expand(
            glob,
            strict_leading_dot,
            strict_wildcard_slash,
            Terminal::EndOrSlash,
        )
    }
}

fn expand(
    glob: &str,
    strict_leading_dot: bool,
    strict_wildcard_slash: bool,
    terminal: Terminal,
) -> String {
    let ctx = Ctx {
        strict_leading_dot,
        strict_wildcard_slash,
    };
    let mut tokens = tokenize(glob, ctx);
    tokens.push(Token::End(terminal));
    match render_seq(&tokens, ctx) {
        Out::Ok(body) => body,
        Out::Dead => "\\b\\B".to_string(),
        Out::Escapes => unreachable!("the terminal token resolves every constraint"),
    }
}

// PHP iterates over bytes; iterating over chars is equivalent for valid UTF-8 because
// every character the loop treats specially is ASCII.
fn tokenize(glob: &str, ctx: Ctx) -> Vec<Token> {
    let chars: Vec<char> = glob.chars().collect();
    let mut first_byte = true;
    let mut escaping = false;
    // stack[0] is the root sequence (a pseudo-group with a single alternative); deeper
    // frames are open `{` groups, so PHP's $inCurlies == stack.len() - 1.
    let mut stack: Vec<Vec<Vec<Token>>> = vec![vec![Vec::new()]];

    fn push(stack: &mut [Vec<Vec<Token>>], token: Token) {
        stack.last_mut().unwrap().last_mut().unwrap().push(token);
    }

    let mut i = 0;
    while i < chars.len() {
        let car = chars[i];
        if first_byte && ctx.strict_leading_dot && car != '.' {
            push(&mut stack, Token::NoDot);
        }

        first_byte = car == '/';

        if first_byte
            && ctx.strict_wildcard_slash
            && i + 2 < chars.len()
            && chars[i + 1] == '*'
            && chars[i + 2] == '*'
            && (i + 3 >= chars.len() || chars[i + 3] == '/')
        {
            push(
                &mut stack,
                Token::Globstar {
                    trailing_slash_optional: i + 3 >= chars.len(),
                },
            );
            i += 2 + usize::from(i + 3 < chars.len());
            escaping = false;
            i += 1;
            continue;
        }

        if car == '#'
            || car == '.'
            || car == '('
            || car == ')'
            || car == '|'
            || car == '+'
            || car == '^'
            || car == '$'
        {
            push(&mut stack, Token::Text(format!("\\{car}")));
        } else if car == '*' {
            push(
                &mut stack,
                if escaping {
                    Token::Text("\\*".to_string())
                } else {
                    Token::Star
                },
            );
        } else if car == '?' {
            push(
                &mut stack,
                if escaping {
                    Token::Text("\\?".to_string())
                } else {
                    Token::Question
                },
            );
        } else if car == '{' {
            if escaping {
                push(&mut stack, Token::Text("\\{".to_string()));
            } else {
                stack.push(vec![Vec::new()]);
            }
        } else if car == '}' && stack.len() > 1 {
            if escaping {
                push(&mut stack, Token::Text("}".to_string()));
            } else {
                let alternatives = stack.pop().unwrap();
                push(
                    &mut stack,
                    Token::Group {
                        alternatives,
                        closed: true,
                    },
                );
            }
        } else if car == ',' && stack.len() > 1 {
            if escaping {
                push(&mut stack, Token::Text(",".to_string()));
            } else {
                stack.last_mut().unwrap().push(Vec::new());
            }
        } else if car == '\\' {
            if escaping {
                push(&mut stack, Token::Text("\\\\".to_string()));
                escaping = false;
            } else {
                escaping = true;
            }
            i += 1;
            continue;
        } else {
            push(&mut stack, Token::Text(car.to_string()));
        }
        escaping = false;
        i += 1;
    }

    // PHP leaves an unterminated `(` for an unclosed `{`; keep the broken shape so the
    // resulting pattern fails to compile just as the PCRE one does.
    while stack.len() > 1 {
        let alternatives = stack.pop().unwrap();
        push(
            &mut stack,
            Token::Group {
                alternatives,
                closed: false,
            },
        );
    }
    stack.pop().unwrap().pop().unwrap()
}

fn star(ctx: Ctx) -> &'static str {
    if ctx.strict_wildcard_slash {
        "[^/]*"
    } else {
        ".*"
    }
}

fn question(ctx: Ctx) -> &'static str {
    if ctx.strict_wildcard_slash {
        "[^/]"
    } else {
        "."
    }
}

fn globstar(ctx: Ctx, trailing_slash_optional: bool) -> String {
    // PHP emits `/(?:(?=[^\.])[^/]++/)*`; `(?=[^\.])[^/]+` collapses to `[^/.][^/]*`.
    let step = if ctx.strict_leading_dot {
        "[^/.][^/]*"
    } else {
        "[^/]+"
    };
    let slash = if trailing_slash_optional { "/?" } else { "/" };
    format!("/(?:{step}{slash})*")
}

fn render_seq(tokens: &[Token], ctx: Ctx) -> Out {
    let mut out = String::new();
    for (idx, token) in tokens.iter().enumerate() {
        match token {
            Token::Text(text) => out.push_str(text),
            Token::Star => out.push_str(star(ctx)),
            Token::Question => out.push_str(question(ctx)),
            Token::Globstar {
                trailing_slash_optional,
            } => out.push_str(&globstar(ctx, *trailing_slash_optional)),
            Token::End(Terminal::EndAnchor) => {}
            Token::End(Terminal::EndOrSlash) => out.push_str("(?:$|/)"),
            Token::NoDot => {
                return prefix(out, apply_no_dot(&tokens[idx + 1..], ctx));
            }
            Token::Group {
                alternatives,
                closed,
            } => match render_group_isolated(alternatives, *closed, ctx) {
                Out::Ok(group) => out.push_str(&group),
                Out::Dead => return Out::Dead,
                Out::Escapes => {
                    // A constraint inside the group applies to what follows it; splice
                    // the remainder into each alternative and re-render.
                    let rest = &tokens[idx + 1..];
                    let mut branches = Vec::new();
                    for alternative in alternatives {
                        let mut seq = alternative.clone();
                        seq.extend_from_slice(rest);
                        match render_seq(&seq, ctx) {
                            Out::Ok(branch) => branches.push(branch),
                            Out::Dead => {}
                            Out::Escapes => return Out::Escapes,
                        }
                    }
                    return match join_branches(branches) {
                        Some(joined) => Out::Ok(out + &joined),
                        None => Out::Dead,
                    };
                }
            },
        }
    }
    Out::Ok(out)
}

fn render_group_isolated(alternatives: &[Vec<Token>], closed: bool, ctx: Ctx) -> Out {
    let mut rendered = Vec::new();
    for alternative in alternatives {
        match render_seq(alternative, ctx) {
            Out::Ok(branch) => rendered.push(branch),
            Out::Dead => {}
            Out::Escapes => {
                if !closed {
                    // The unterminated `(` makes the output uncompilable either way;
                    // resolve the dangling constraint as if the pattern ended here.
                    let mut seq = alternative.clone();
                    seq.push(Token::End(Terminal::EndAnchor));
                    if let Out::Ok(branch) = render_seq(&seq, ctx) {
                        rendered.push(branch);
                    }
                } else {
                    return Out::Escapes;
                }
            }
        }
    }
    if rendered.is_empty() {
        return Out::Dead;
    }
    let close = if closed { ")" } else { "" };
    Out::Ok(format!("({}{close}", rendered.join("|")))
}

/// Renders the remainder of the pattern under the constraint that the next matched
/// character must not be a dot.
fn apply_no_dot(tokens: &[Token], ctx: Ctx) -> Out {
    let Some((first, rest)) = tokens.split_first() else {
        return Out::Escapes;
    };
    match first {
        Token::Text(text) if text == "\\." => Out::Dead,
        Token::Text(text) if text == "[" => {
            if let Some((class, after)) = scan_char_class(tokens) {
                prefix(
                    format!("[{class}&&[^.]]"),
                    render_seq(&tokens[after..], ctx),
                )
            } else {
                prefix("[".to_string(), render_seq(rest, ctx))
            }
        }
        Token::Text(text) => prefix(text.clone(), render_seq(rest, ctx)),
        Token::Star => {
            // Either the `*` consumes at least one character, which then carries the
            // constraint, or it consumes none and the constraint moves past it.
            let head = if ctx.strict_wildcard_slash {
                "[^/.][^/]*"
            } else {
                "[^.\\n].*"
            };
            let consumed = prefix(head.to_string(), render_seq(rest, ctx));
            let skipped = apply_no_dot(rest, ctx);
            match (consumed, skipped) {
                (Out::Escapes, _) | (_, Out::Escapes) => Out::Escapes,
                (Out::Ok(a), Out::Ok(b)) => Out::Ok(format!("(?:{a}|{b})")),
                (Out::Ok(a), Out::Dead) => Out::Ok(a),
                (Out::Dead, Out::Ok(b)) => Out::Ok(b),
                (Out::Dead, Out::Dead) => Out::Dead,
            }
        }
        Token::Question => {
            let head = if ctx.strict_wildcard_slash {
                "[^/.]"
            } else {
                "[^.\\n]"
            };
            prefix(head.to_string(), render_seq(rest, ctx))
        }
        // The construct starts with a literal `/`, which satisfies the constraint.
        Token::Globstar { .. } => render_seq(tokens, ctx),
        Token::Group {
            alternatives,
            closed: true,
        } => {
            let mut branches = Vec::new();
            for alternative in alternatives {
                let mut seq = alternative.clone();
                seq.extend_from_slice(rest);
                match apply_no_dot(&seq, ctx) {
                    Out::Ok(branch) => branches.push(branch),
                    Out::Dead => {}
                    Out::Escapes => return Out::Escapes,
                }
            }
            match join_branches(branches) {
                Some(joined) => Out::Ok(joined),
                None => Out::Dead,
            }
        }
        // The unterminated `(` makes the output uncompilable either way; skip the
        // expansion.
        Token::Group { closed: false, .. } => render_seq(tokens, ctx),
        // `(?=[^.])$`: no next character can exist.
        Token::End(Terminal::EndAnchor) => Out::Dead,
        // `(?=[^.])(?=$|/)`: the next character must exist, must not be a dot, and must
        // be either the end (impossible) or a slash — exactly one consumable `/`.
        Token::End(Terminal::EndOrSlash) => Out::Ok("/".to_string()),
        Token::NoDot => unreachable!("PHP never emits two consecutive look-aheads"),
    }
}

fn prefix(head: String, tail: Out) -> Out {
    match tail {
        Out::Ok(tail) => Out::Ok(head + &tail),
        other => other,
    }
}

fn join_branches(mut branches: Vec<String>) -> Option<String> {
    match branches.len() {
        0 => None,
        1 => branches.pop(),
        _ => Some(format!("(?:{})", branches.join("|"))),
    }
}

/// A `[` passes through the PHP loop untouched and opens a character class in the
/// final regex, so a no-dot constraint has to intersect the class rather than be
/// satisfied by the `[` itself. Collects a class spanning plain text tokens; None when
/// no plain closing `]` follows (the `[` is then treated as a literal).
fn scan_char_class(tokens: &[Token]) -> Option<(String, usize)> {
    let mut class = String::from("[");
    let mut idx = 1;
    loop {
        match tokens.get(idx)? {
            Token::Text(text) if text == "]" => {
                class.push(']');
                return Some((class, idx + 1));
            }
            Token::Text(text) => class.push_str(text),
            _ => return None,
        }
        idx += 1;
    }
}
