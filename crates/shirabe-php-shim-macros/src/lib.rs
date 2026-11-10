//! `bformat!` and `bwrite!`, re-exported by `shirabe_php_shim`. The expansion names
//! `::shirabe_php_shim::PhpDisplay` and `::bstr::BString`, so the calling crate depends on both.

#![expect(
    clippy::disallowed_types,
    clippy::disallowed_macros,
    clippy::disallowed_methods,
    reason = "the crate generates Rust source, which rustc reads as text"
)]

use syn::spanned::Spanned as _;

/// PHP's string interpolation and `.` operator: `bformat!("Package {name} not found")` is
/// `"Package $name not found"`, and evaluates to a `BString` holding the bytes of every argument
/// as they are. An argument is anything that implements `PhpDisplay`.
///
/// The syntax is that of `format!`, restricted to the placeholders `{}`, `{<index>}` and
/// `{<identifier>}`, with `{{` and `}}` for literal braces. The format may also be a byte string
/// literal, for bytes that a `str` literal cannot hold. Each argument is evaluated once, in the
/// order it is written. Format specs are rejected: use `sprintf()` or `str_pad()`, which count
/// widths in bytes as PHP does.
///
/// ```
/// # use shirabe_php_shim::bformat;
/// # use bstr::BString;
/// let name = BString::from(b"\xffvendor".to_vec());
/// let count = 2i64;
/// assert_eq!(
///     bformat!("{name}: {} of {total}, {{}}", count, total = 3i64),
///     BString::from(b"\xffvendor: 2 of 3, {}".to_vec()),
/// );
/// ```
///
/// ```compile_fail
/// # use shirabe_php_shim::bformat;
/// bformat!("{:>3}", 1i64);
/// ```
///
/// ```compile_fail
/// # use shirabe_php_shim::bformat;
/// bformat!("{}", 1i64, 2i64);
/// ```
///
/// ```compile_fail
/// # use shirabe_php_shim::bformat;
/// bformat!("{} {}", 1i64);
/// ```
#[proc_macro]
pub fn bformat(input: proc_macro::TokenStream) -> proc_macro::TokenStream {
    let input = syn::parse_macro_input!(input as FormatInput<false>);
    expand(input)
        .unwrap_or_else(|e| e.to_compile_error())
        .into()
}

/// [`bformat!`] that appends to an existing `BString` instead of creating one:
/// `bwrite!(out, "{name}\n")`.
///
/// ```
/// # use shirabe_php_shim::bwrite;
/// # use bstr::BString;
/// let mut out = BString::from("a");
/// bwrite!(out, "{}c", "b");
/// let target = &mut out;
/// bwrite!(target, "d");
/// assert_eq!(out, BString::from("abcd"));
/// ```
#[proc_macro]
pub fn bwrite(input: proc_macro::TokenStream) -> proc_macro::TokenStream {
    let input = syn::parse_macro_input!(input as FormatInput<true>);
    expand(input)
        .unwrap_or_else(|e| e.to_compile_error())
        .into()
}

/// `[<out>,] <format literal> [, <arg>]* [,]`. `WITH_OUT` selects `bwrite!`, whose
/// first operand is the buffer to append to.
struct FormatInput<const WITH_OUT: bool> {
    out: Option<syn::Expr>,
    format: Vec<u8>,
    format_span: proc_macro2::Span,
    positional: Vec<syn::Expr>,
    named: Vec<(syn::Ident, syn::Expr)>,
}

impl<const WITH_OUT: bool> syn::parse::Parse for FormatInput<WITH_OUT> {
    fn parse(input: syn::parse::ParseStream) -> syn::Result<Self> {
        let out = if WITH_OUT {
            let out = input.parse::<syn::Expr>()?;
            input.parse::<syn::Token![,]>()?;
            Some(out)
        } else {
            None
        };

        let lit = input.parse::<syn::Lit>()?;
        let format = match &lit {
            syn::Lit::Str(s) => s.value().into_bytes(),
            syn::Lit::ByteStr(s) => s.value(),
            _ => {
                return Err(syn::Error::new(
                    lit.span(),
                    "format argument must be a string or byte string literal",
                ));
            }
        };

        let mut positional = Vec::new();
        let mut named: Vec<(syn::Ident, syn::Expr)> = Vec::new();
        if !input.is_empty() {
            input.parse::<syn::Token![,]>()?;
            let args = syn::punctuated::Punctuated::<Arg, syn::Token![,]>::parse_terminated(input)?;
            for arg in args {
                match arg {
                    Arg::Positional(expr) => {
                        if !named.is_empty() {
                            return Err(syn::Error::new(
                                expr.span(),
                                "positional arguments cannot follow named arguments",
                            ));
                        }
                        positional.push(expr);
                    }
                    Arg::Named(name, expr) => {
                        if named.iter().any(|(n, _)| *n == name) {
                            return Err(syn::Error::new(
                                name.span(),
                                format!("duplicate argument named `{name}`"),
                            ));
                        }
                        named.push((name, expr));
                    }
                }
            }
        }

        Ok(Self {
            out,
            format,
            format_span: lit.span(),
            positional,
            named,
        })
    }
}

enum Arg {
    Positional(syn::Expr),
    Named(syn::Ident, syn::Expr),
}

impl syn::parse::Parse for Arg {
    fn parse(input: syn::parse::ParseStream) -> syn::Result<Self> {
        if input.peek(syn::Ident) && input.peek2(syn::Token![=]) && !input.peek2(syn::Token![==]) {
            let name = input.parse::<syn::Ident>()?;
            input.parse::<syn::Token![=]>()?;
            Ok(Arg::Named(name, input.parse()?))
        } else {
            Ok(Arg::Positional(input.parse()?))
        }
    }
}

/// One step of the expansion: append literal bytes, or append the argument at an index.
#[derive(Debug)]
enum Step {
    Bytes(Vec<u8>),
    Arg(usize),
}

#[derive(Debug)]
enum Piece {
    Bytes(Vec<u8>),
    Next,
    Index(usize),
    Name(String),
}

/// Splits a format string into literal bytes and placeholders. A placeholder is `{}`, `{<index>}`
/// or `{<identifier>}`; format specs are rejected, since their widths and precisions would have to
/// count either bytes or characters.
fn parse_format(format: &[u8], span: proc_macro2::Span) -> syn::Result<Vec<Piece>> {
    let mut pieces = Vec::new();
    let mut bytes = Vec::new();
    let mut i = 0;
    while i < format.len() {
        match format[i] {
            b'{' if format.get(i + 1) == Some(&b'{') => {
                bytes.push(b'{');
                i += 2;
            }
            b'{' => {
                let Some(len) = format[i + 1..].iter().position(|&b| b == b'}') else {
                    return Err(syn::Error::new(
                        span,
                        "invalid format string: expected `}` but string was terminated; \
                         if you intended to print `{`, escape it as `{{`",
                    ));
                };
                let content = &format[i + 1..i + 1 + len];
                let piece = parse_placeholder(content, span)?;
                if !bytes.is_empty() {
                    pieces.push(Piece::Bytes(std::mem::take(&mut bytes)));
                }
                pieces.push(piece);
                i += len + 2;
            }
            b'}' if format.get(i + 1) == Some(&b'}') => {
                bytes.push(b'}');
                i += 2;
            }
            b'}' => {
                return Err(syn::Error::new(
                    span,
                    "invalid format string: unmatched `}` found; \
                     if you intended to print `}`, escape it as `}}`",
                ));
            }
            b => {
                bytes.push(b);
                i += 1;
            }
        }
    }
    if !bytes.is_empty() {
        pieces.push(Piece::Bytes(bytes));
    }
    Ok(pieces)
}

fn parse_placeholder(content: &[u8], span: proc_macro2::Span) -> syn::Result<Piece> {
    if content.contains(&b':') {
        return Err(syn::Error::new(
            span,
            "format specs are not supported; use sprintf() or str_pad() for widths and precisions",
        ));
    }
    if content.is_empty() {
        return Ok(Piece::Next);
    }
    if content.iter().all(u8::is_ascii_digit) {
        let index = std::str::from_utf8(content)
            .expect("ASCII digits are UTF-8")
            .parse()
            .map_err(|_| {
                syn::Error::new(span, "invalid format string: argument index too large")
            })?;
        return Ok(Piece::Index(index));
    }
    let is_identifier = (content[0] == b'_' || content[0].is_ascii_alphabetic())
        && content[1..]
            .iter()
            .all(|&b| b == b'_' || b.is_ascii_alphanumeric())
        && content != b"_";
    if is_identifier {
        let name = std::str::from_utf8(content).expect("an ASCII identifier is UTF-8");
        return Ok(Piece::Name(name.to_owned()));
    }
    Err(syn::Error::new(
        span,
        format!(
            "invalid format string: expected `}}`, `{{<index>}}` or `{{<identifier>}}`, found `{{{}}}`",
            content.escape_ascii()
        ),
    ))
}

fn expand<const WITH_OUT: bool>(
    input: FormatInput<WITH_OUT>,
) -> syn::Result<proc_macro2::TokenStream> {
    let FormatInput {
        out,
        format,
        format_span,
        positional,
        named,
    } = input;
    let pieces = parse_format(&format, format_span)?;

    // Every argument is evaluated once, in the order it is written, and captured identifiers
    // after them; each placeholder then refers to one of these.
    let mut args: Vec<syn::Expr> = positional;
    let positional_len = args.len();
    let mut names: Vec<String> = Vec::new();
    for (name, expr) in named {
        names.push(name.to_string());
        args.push(expr);
    }
    let mut used = vec![false; args.len()];
    let mut next = 0;
    let mut steps = Vec::new();
    for piece in pieces {
        let index = match piece {
            Piece::Bytes(bytes) => {
                steps.push(Step::Bytes(bytes));
                continue;
            }
            Piece::Next => {
                next += 1;
                check_positional(next - 1, positional_len, format_span)?
            }
            Piece::Index(index) => check_positional(index, positional_len, format_span)?,
            Piece::Name(name) => match names.iter().position(|n| *n == name) {
                Some(n) => positional_len + n,
                None => {
                    let ident = syn::Ident::new(&name, format_span);
                    names.push(name);
                    args.push(syn::parse_quote!(#ident));
                    used.push(false);
                    args.len() - 1
                }
            },
        };
        used[index] = true;
        steps.push(Step::Arg(index));
    }
    if let Some(index) = used.iter().position(|u| !u) {
        let message = if index < positional_len {
            "argument never used"
        } else {
            "named argument never used"
        };
        return Err(syn::Error::new(args[index].span(), message));
    }

    let out_ident = syn::Ident::new("__bformat_out", proc_macro2::Span::mixed_site());
    let binds: Vec<syn::Ident> = (0..args.len())
        .map(|i| {
            syn::Ident::new(
                &format!("__bformat_arg{i}"),
                proc_macro2::Span::mixed_site(),
            )
        })
        .collect();
    let mut capacity = 0;
    let stmts: Vec<proc_macro2::TokenStream> = steps
        .into_iter()
        .map(|step| match step {
            Step::Bytes(bytes) => {
                capacity += bytes.len();
                let lit = proc_macro2::Literal::byte_string(&bytes);
                quote::quote!(#out_ident.extend_from_slice(#lit);)
            }
            Step::Arg(index) => {
                // A call whose tokens all sit at the argument, so that an argument that does not
                // implement `PhpDisplay` is reported there.
                let span = proc_macro2::Span::mixed_site().located_at(args[index].span());
                let mut bind = binds[index].clone();
                bind.set_span(span);
                let mut out_ident = out_ident.clone();
                out_ident.set_span(span);
                quote::quote_spanned!(span=>
                    ::shirabe_php_shim::PhpDisplay::fmt(#bind, #out_ident);
                )
            }
        })
        .collect();
    let body = quote::quote! {
        match (#(&(#args), )*) {
            (#(#binds, )*) => { #(#stmts)* }
        }
    };

    Ok(match out {
        Some(out) => quote::quote! {{
            let #out_ident: &mut ::bstr::BString = {
                use ::core::borrow::BorrowMut as _;
                (#out).borrow_mut()
            };
            #body
        }},
        None => {
            let buffer = syn::Ident::new("__bformat_buffer", proc_macro2::Span::mixed_site());
            quote::quote! {{
                let mut #buffer = ::bstr::BString::from(::std::vec::Vec::with_capacity(#capacity));
                {
                    let #out_ident = &mut #buffer;
                    #body
                }
                #buffer
            }}
        }
    })
}

fn check_positional(index: usize, len: usize, span: proc_macro2::Span) -> syn::Result<usize> {
    if index < len {
        Ok(index)
    } else {
        Err(syn::Error::new(
            span,
            format!(
                "invalid reference to positional argument {index} ({})",
                match len {
                    0 => "no arguments were given".to_owned(),
                    1 => "there is 1 argument".to_owned(),
                    n => format!("there are {n} arguments"),
                }
            ),
        ))
    }
}
