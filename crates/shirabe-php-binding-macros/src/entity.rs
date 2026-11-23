//! `php_entity!`: the `Entity` impl of a handle type, and the routing of the members its classes
//! forward to the adapters `#[php_binding]` generated.

use crate::decls;

/// `|handle| expr`: what an entity declaration computes from a handle or a built value.
struct Access {
    pat: syn::Pat,
    body: syn::Expr,
}

impl syn::parse::Parse for Access {
    fn parse(input: syn::parse::ParseStream<'_>) -> syn::Result<Self> {
        let closure: syn::ExprClosure = input.parse()?;
        if closure.inputs.len() != 1 {
            return Err(syn::Error::new_spanned(&closure, "expected `|handle| ...`"));
        }
        Ok(Access {
            pat: closure.inputs.first().expect("one input").clone(),
            body: *closure.body,
        })
    }
}

impl Access {
    /// The body, with the handle bound to the closure's parameter.
    fn apply(&self, handle: proc_macro2::TokenStream) -> proc_macro2::TokenStream {
        let Access { pat, body } = self;
        quote::quote! {{
            let #pat = #handle;
            #body
        }}
    }
}

/// Parses `key: value, ...` up to the end of `input`, each key one of `keys` and given at most
/// once.
fn parse_fields(
    input: syn::parse::ParseStream<'_>,
    keys: &[&str],
    mut field: impl FnMut(&syn::Ident, syn::parse::ParseStream<'_>) -> syn::Result<()>,
) -> syn::Result<()> {
    let mut seen = Vec::new();
    while !input.is_empty() {
        let key: syn::Ident = syn::ext::IdentExt::parse_any(input)?;
        let name = key.to_string();
        if !keys.contains(&name.as_str()) {
            let expected: Vec<String> = keys.iter().map(|key| format!("`{key}`")).collect();
            return Err(syn::Error::new(
                key.span(),
                format!("expected one of {}", expected.join(", ")),
            ));
        }
        if seen.contains(&name) {
            return Err(syn::Error::new(
                key.span(),
                format!("`{name}` is given twice"),
            ));
        }
        seen.push(name);
        input.parse::<syn::Token![:]>()?;
        field(&key, input)?;
        if !input.is_empty() {
            input.parse::<syn::Token![,]>()?;
        }
    }
    Ok(())
}

/// `{ key: value, ... }`, as `parse_fields` parses its content.
fn parse_braced_fields(
    input: syn::parse::ParseStream<'_>,
    keys: &[&str],
    field: impl FnMut(&syn::Ident, syn::parse::ParseStream<'_>) -> syn::Result<()>,
) -> syn::Result<()> {
    let content;
    syn::braced!(content in input);
    parse_fields(&content, keys, field)
}

fn missing(what: &str, span: proc_macro2::Span) -> syn::Error {
    syn::Error::new(span, format!("missing `{what}`"))
}

/// The key of a map in a declaration: a PHP class name, or a PHP member name.
trait Key: Sized {
    fn parse(input: syn::parse::ParseStream<'_>) -> syn::Result<Self>;
    fn name(&self) -> String;
    fn span(&self) -> proc_macro2::Span;
}

impl Key for syn::LitStr {
    fn parse(input: syn::parse::ParseStream<'_>) -> syn::Result<Self> {
        input.parse()
    }

    fn name(&self) -> String {
        self.value()
    }

    fn span(&self) -> proc_macro2::Span {
        syn::LitStr::span(self)
    }
}

impl Key for syn::Ident {
    fn parse(input: syn::parse::ParseStream<'_>) -> syn::Result<Self> {
        syn::ext::IdentExt::parse_any(input)
    }

    fn name(&self) -> String {
        self.to_string()
    }

    fn span(&self) -> proc_macro2::Span {
        syn::Ident::span(self)
    }
}

/// `{ key: value, ... }` with distinct keys.
fn parse_map<K: Key, V>(
    input: syn::parse::ParseStream<'_>,
    mut value: impl FnMut(&K, syn::parse::ParseStream<'_>) -> syn::Result<V>,
) -> syn::Result<Vec<(K, V)>> {
    let content;
    syn::braced!(content in input);
    let mut entries: Vec<(K, V)> = Vec::new();
    while !content.is_empty() {
        let key = K::parse(&content)?;
        if entries.iter().any(|(other, _)| other.name() == key.name()) {
            return Err(syn::Error::new(
                key.span(),
                format!("`{}` is given twice", key.name()),
            ));
        }
        content.parse::<syn::Token![:]>()?;
        let parsed = value(&key, &content)?;
        entries.push((key, parsed));
        if !content.is_empty() {
            content.parse::<syn::Token![,]>()?;
        }
    }
    Ok(entries)
}

/// A PHP member named in a declaration, with whether a forwarded member matched it.
struct Listed<T> {
    name: syn::Ident,
    value: T,
    used: std::cell::Cell<bool>,
}

impl<T> Listed<T> {
    fn new(name: syn::Ident, value: T) -> Self {
        Listed {
            name,
            value,
            used: std::cell::Cell::new(false),
        }
    }
}

/// `[member, ...]` with distinct members.
fn parse_members(input: syn::parse::ParseStream<'_>) -> syn::Result<Vec<Listed<()>>> {
    let content;
    syn::bracketed!(content in input);
    let mut members: Vec<Listed<()>> = Vec::new();
    for name in content.parse_terminated(
        <syn::Ident as syn::ext::IdentExt>::parse_any,
        syn::Token![,],
    )? {
        if members.iter().any(|other| other.name == name) {
            return Err(syn::Error::new(
                name.span(),
                format!("`{name}` is given twice"),
            ));
        }
        members.push(Listed::new(name, ()));
    }
    Ok(members)
}

/// The adapters of an item answering instance methods, and how a handle reaches the type they
/// call.
struct Instance {
    key: syn::Ident,
    adapters: syn::Path,
    target: syn::Type,
    borrow: Option<Access>,
    borrow_mut: Option<Access>,
    deref: Option<Access>,
    used: std::cell::Cell<bool>,
}

impl Instance {
    fn parse(key: &syn::Ident, input: syn::parse::ParseStream<'_>) -> syn::Result<Self> {
        let mut adapters = None;
        let mut target = None;
        let mut borrow = None;
        let mut borrow_mut = None;
        let mut deref = None;
        parse_braced_fields(
            input,
            &["adapters", "target", "borrow", "borrow_mut", "deref"],
            |field, input| {
                match field.to_string().as_str() {
                    "adapters" => adapters = Some(input.parse()?),
                    "target" => target = Some(with_static_bound(input.parse()?)),
                    "borrow" => borrow = Some(input.parse()?),
                    "borrow_mut" => borrow_mut = Some(input.parse()?),
                    "deref" => deref = Some(input.parse()?),
                    _ => unreachable!("parse_fields checks the keys"),
                }
                Ok(())
            },
        )?;
        if borrow.is_some() && deref.is_some() {
            return Err(syn::Error::new(
                key.span(),
                "`borrow` and `deref` both answer the `&self` methods; give one of them",
            ));
        }
        if borrow.is_none() && borrow_mut.is_none() && deref.is_none() {
            return Err(syn::Error::new(
                key.span(),
                "expected at least one of `borrow`, `borrow_mut` or `deref`",
            ));
        }
        Ok(Instance {
            key: key.clone(),
            adapters: adapters.ok_or_else(|| missing("adapters", key.span()))?,
            target: target.ok_or_else(|| missing("target", key.span()))?,
            borrow,
            borrow_mut,
            deref,
            used: std::cell::Cell::new(false),
        })
    }
}

/// The adapters of an item answering static methods.
struct Static {
    key: syn::Ident,
    adapters: syn::Path,
    used: std::cell::Cell<bool>,
}

impl Static {
    fn parse(key: &syn::Ident, input: syn::parse::ParseStream<'_>) -> syn::Result<Self> {
        let mut adapters = None;
        parse_braced_fields(input, &["adapters"], |_, input| {
            adapters = Some(input.parse()?);
            Ok(())
        })?;
        Ok(Static {
            key: key.clone(),
            adapters: adapters.ok_or_else(|| missing("adapters", key.span()))?,
            used: std::cell::Cell::new(false),
        })
    }
}

/// A module of hand-written functions taking the handle itself, answering the instance methods
/// listed, or every one.
struct Handwritten {
    key: syn::Ident,
    module: syn::Path,
    members: Option<Vec<Listed<()>>>,
    used: std::cell::Cell<bool>,
}

impl Handwritten {
    fn parse(key: &syn::Ident, input: syn::parse::ParseStream<'_>) -> syn::Result<Self> {
        let mut module = None;
        let mut members = None;
        parse_braced_fields(input, &["module", "members"], |field, input| {
            match field.to_string().as_str() {
                "module" => module = Some(input.parse()?),
                "members" => members = Some(parse_members(input)?),
                _ => unreachable!("parse_fields checks the keys"),
            }
            Ok(())
        })?;
        Ok(Handwritten {
            key: key.clone(),
            module: module.ok_or_else(|| missing("module", key.span()))?,
            members,
            used: std::cell::Cell::new(false),
        })
    }
}

/// What answers the forwarded members a PHP type declares.
struct Methods {
    fqcn: syn::LitStr,
    instance: Option<Instance>,
    r#static: Option<Static>,
    handwritten: Option<Handwritten>,
    unimplemented: Vec<Listed<()>>,
    refused: Vec<Listed<syn::LitStr>>,
}

impl Methods {
    /// `{ ... }` of the PHP type `fqcn`; `keys` are the kinds of entries allowed.
    fn parse(
        fqcn: &syn::LitStr,
        input: syn::parse::ParseStream<'_>,
        keys: &[&str],
    ) -> syn::Result<Self> {
        let mut methods = Methods {
            fqcn: fqcn.clone(),
            instance: None,
            r#static: None,
            handwritten: None,
            unimplemented: Vec::new(),
            refused: Vec::new(),
        };
        parse_braced_fields(input, keys, |key, input| {
            match key.to_string().as_str() {
                "instance" => methods.instance = Some(Instance::parse(key, input)?),
                "static" => methods.r#static = Some(Static::parse(key, input)?),
                "handwritten" => methods.handwritten = Some(Handwritten::parse(key, input)?),
                "unimplemented" => methods.unimplemented = parse_members(input)?,
                "refused" => {
                    methods.refused = parse_map(input, |_: &syn::Ident, input| input.parse())?
                        .into_iter()
                        .map(|(name, reason)| Listed::new(name, reason))
                        .collect();
                }
                _ => unreachable!("parse_fields checks the keys"),
            }
            Ok(())
        })?;
        if let (Some(handwritten), Some(instance)) = (&methods.handwritten, &methods.instance)
            && handwritten.members.is_none()
        {
            return Err(syn::Error::new(
                instance.key.span(),
                "`handwritten` answers every instance method of this type; list its `members`, or remove `instance`",
            ));
        }
        let mut names: Vec<&syn::Ident> = methods
            .unimplemented
            .iter()
            .map(|listed| &listed.name)
            .chain(methods.refused.iter().map(|listed| &listed.name))
            .collect();
        if let Some(members) = methods
            .handwritten
            .as_ref()
            .and_then(|handwritten| handwritten.members.as_ref())
        {
            names.extend(members.iter().map(|listed| &listed.name));
        }
        for (index, name) in names.iter().enumerate() {
            if names[..index].contains(name) {
                return Err(syn::Error::new(
                    name.span(),
                    format!("`{name}` is listed twice"),
                ));
            }
        }
        Ok(methods)
    }

    /// Fails on anything given that no forwarded member reached, so that a declaration cannot go
    /// stale.
    fn check_used(&self) -> syn::Result<()> {
        let fqcn = self.fqcn.value();
        let unused = |span: proc_macro2::Span, what: String| {
            Err(syn::Error::new(
                span,
                format!("{what}, which no member these classes forward reaches"),
            ))
        };
        if let Some(instance) = &self.instance
            && !instance.used.get()
        {
            return unused(
                instance.key.span(),
                format!("`instance` answers the instance methods {fqcn} declares"),
            );
        }
        if let Some(r#static) = &self.r#static
            && !r#static.used.get()
        {
            return unused(
                r#static.key.span(),
                format!("`static` answers the static methods {fqcn} declares"),
            );
        }
        if let Some(handwritten) = &self.handwritten {
            if !handwritten.used.get() {
                return unused(
                    handwritten.key.span(),
                    format!("`handwritten` answers instance methods {fqcn} declares"),
                );
            }
            for listed in handwritten.members.iter().flatten() {
                if !listed.used.get() {
                    return unused(
                        listed.name.span(),
                        format!("`{}` names an instance method of {fqcn}", listed.name),
                    );
                }
            }
        }
        let statuses = self
            .unimplemented
            .iter()
            .map(|listed| (&listed.name, listed.used.get()))
            .chain(
                self.refused
                    .iter()
                    .map(|listed| (&listed.name, listed.used.get())),
            );
        for (name, used) in statuses {
            if !used {
                return unused(name.span(), format!("`{name}` names a method of {fqcn}"));
            }
        }
        Ok(())
    }
}

fn parse_methods(input: syn::parse::ParseStream<'_>, keys: &[&str]) -> syn::Result<Vec<Methods>> {
    Ok(parse_map(input, |fqcn: &syn::LitStr, input| {
        Methods::parse(fqcn, input, keys)
    })?
    .into_iter()
    .map(|(_, methods)| methods)
    .collect())
}

/// How a class's forwarded constructor is answered.
enum Constructor {
    /// The adapter of the inherent impl answering the constructor, and how the value it builds
    /// becomes a handle.
    Adapters {
        module: syn::Path,
        into: Access,
    },
    /// A hand-written function answering the call with the handle.
    Handwritten {
        function: syn::Path,
    },
    Unimplemented,
    Refused(syn::LitStr),
}

impl syn::parse::Parse for Constructor {
    fn parse(input: syn::parse::ParseStream<'_>) -> syn::Result<Self> {
        if !input.peek(syn::token::Brace) {
            let kind: syn::Ident = input.parse()?;
            return match kind.to_string().as_str() {
                "unimplemented" => Ok(Constructor::Unimplemented),
                "refused" => {
                    let content;
                    syn::parenthesized!(content in input);
                    Ok(Constructor::Refused(content.parse()?))
                }
                _ => Err(syn::Error::new(
                    kind.span(),
                    "expected `unimplemented`, `refused(\"reason\")`, `{ adapters: ..., into: ... }` or `{ handwritten: ... }`",
                )),
            };
        }
        let span = input.span();
        let mut adapters = None;
        let mut into = None;
        let mut handwritten = None;
        parse_braced_fields(input, &["adapters", "into", "handwritten"], |key, input| {
            match key.to_string().as_str() {
                "adapters" => adapters = Some(input.parse()?),
                "into" => into = Some(input.parse()?),
                "handwritten" => handwritten = Some(input.parse()?),
                _ => unreachable!("parse_fields checks the keys"),
            }
            Ok(())
        })?;
        match (adapters, into, handwritten) {
            (Some(module), Some(into), None) => Ok(Constructor::Adapters { module, into }),
            (None, None, Some(function)) => Ok(Constructor::Handwritten { function }),
            _ => Err(syn::Error::new(
                span,
                "expected `{ adapters: ..., into: ... }` or `{ handwritten: ... }`",
            )),
        }
    }
}

/// A class the entities cross as, or only one of their ancestors.
struct ClassEntry {
    fqcn: syn::LitStr,
    ancestor: bool,
    constructor: Option<Constructor>,
}

impl ClassEntry {
    fn parse(fqcn: &syn::LitStr, input: syn::parse::ParseStream<'_>) -> syn::Result<Self> {
        let mut ancestor = false;
        let mut constructor = None;
        parse_braced_fields(input, &["ancestor", "constructor"], |key, input| {
            match key.to_string().as_str() {
                "ancestor" => ancestor = input.parse::<syn::LitBool>()?.value,
                "constructor" => constructor = Some(input.parse()?),
                _ => unreachable!("parse_fields checks the keys"),
            }
            Ok(())
        })?;
        Ok(ClassEntry {
            fqcn: fqcn.clone(),
            ancestor,
            constructor,
        })
    }
}

/// The hand-written functions answering what only some entities need.
#[derive(Default)]
struct Handlers {
    clone: Option<syn::Path>,
    worker_method: Option<syn::Path>,
    job: Option<syn::Path>,
    properties: Vec<(syn::Ident, syn::Path)>,
}

impl Handlers {
    fn parse(input: syn::parse::ParseStream<'_>) -> syn::Result<Self> {
        let mut handlers = Handlers::default();
        parse_braced_fields(
            input,
            &["clone", "worker_method", "job", "properties"],
            |key, input| {
                match key.to_string().as_str() {
                    "clone" => handlers.clone = Some(input.parse()?),
                    "worker_method" => handlers.worker_method = Some(input.parse()?),
                    "job" => handlers.job = Some(input.parse()?),
                    "properties" => {
                        handlers.properties =
                            parse_map(input, |_: &syn::Ident, input| input.parse())?;
                    }
                    _ => unreachable!("parse_fields checks the keys"),
                }
                Ok(())
            },
        )?;
        Ok(handlers)
    }
}

/// A trait object without a lifetime bound stands for one that is `'static`, in a guard's type
/// too, where the default would be the guard's lifetime.
fn with_static_bound(ty: syn::Type) -> syn::Type {
    match ty {
        syn::Type::TraitObject(mut object) => {
            if !object
                .bounds
                .iter()
                .any(|bound| matches!(bound, syn::TypeParamBound::Lifetime(_)))
            {
                object
                    .bounds
                    .push(syn::TypeParamBound::Lifetime(syn::Lifetime::new(
                        "'static",
                        proc_macro2::Span::call_site(),
                    )));
            }
            syn::Type::TraitObject(object)
        }
        ty => ty,
    }
}

struct Input {
    handle: syn::Type,
    type_error: syn::LitByteStr,
    identity: Option<Access>,
    class_of: Option<Access>,
    classes: Vec<ClassEntry>,
    methods: Vec<Methods>,
    handlers: Handlers,
}

impl syn::parse::Parse for Input {
    fn parse(input: syn::parse::ParseStream<'_>) -> syn::Result<Self> {
        let mut handle = None;
        let mut type_error = None;
        let mut identity = None;
        let mut class_of = None;
        let mut classes = None;
        let mut methods = None;
        let mut handlers = Handlers::default();
        parse_fields(
            input,
            &[
                "handle",
                "type_error",
                "identity",
                "class_of",
                "classes",
                "methods",
                "handlers",
            ],
            |key, input| {
                match key.to_string().as_str() {
                    "handle" => handle = Some(input.parse()?),
                    "type_error" => {
                        let text: syn::LitStr = input.parse()?;
                        type_error =
                            Some(syn::LitByteStr::new(text.value().as_bytes(), text.span()));
                    }
                    "identity" => identity = Some(input.parse()?),
                    "class_of" => class_of = Some(input.parse()?),
                    "classes" => {
                        classes = Some(
                            parse_map(input, ClassEntry::parse)?
                                .into_iter()
                                .map(|(_, entry)| entry)
                                .collect(),
                        );
                    }
                    "methods" => {
                        methods = Some(parse_methods(
                            input,
                            &[
                                "instance",
                                "static",
                                "handwritten",
                                "unimplemented",
                                "refused",
                            ],
                        )?);
                    }
                    "handlers" => handlers = Handlers::parse(input)?,
                    _ => unreachable!("parse_fields checks the keys"),
                }
                Ok(())
            },
        )?;
        let span = proc_macro2::Span::call_site();
        Ok(Input {
            handle: handle.ok_or_else(|| missing("handle", span))?,
            type_error: type_error.ok_or_else(|| missing("type_error", span))?,
            identity,
            class_of,
            classes: classes.ok_or_else(|| missing("classes", span))?,
            methods: methods.ok_or_else(|| missing("methods", span))?,
            handlers,
        })
    }
}

pub fn expand(input: proc_macro2::TokenStream) -> proc_macro2::TokenStream {
    match syn::parse2(input).and_then(expand_input) {
        Ok(tokens) => tokens,
        Err(error) => error.to_compile_error(),
    }
}

/// One class the entities cross as, with its declaration and its forwarding module.
struct ClassDecl<'a> {
    entry: &'a ClassEntry,
    fqcn: String,
    span: proc_macro2::Span,
    loaded: decls::Loaded,
    forwarding: proc_macro2::TokenStream,
}

fn load_class(fqcn: &syn::LitStr) -> syn::Result<(decls::Loaded, proc_macro2::TokenStream)> {
    let loaded = decls::load(&fqcn.value(), fqcn.span())?;
    let mut forwarding = quote::quote! { crate::php_worker::rpc::forwarding };
    for segment in &loaded.decl.rust_module {
        let segment = decls::ident(segment, fqcn.span())?;
        forwarding = quote::quote! { #forwarding::#segment };
    }
    Ok((loaded, forwarding))
}

/// How a forwarded method is answered.
enum Answer<'a> {
    Unimplemented,
    Refused(&'a syn::LitStr),
    Handwritten(&'a syn::Path),
    Instance {
        route: usize,
        adapters: &'a syn::Path,
    },
    Static(&'a syn::Path),
}

/// The answer the entry of `methods` for the PHP type declaring `method` gives it.
fn resolve<'a>(
    methods: &'a [Methods],
    class: &str,
    span: proc_macro2::Span,
    method: &decls::Method,
) -> syn::Result<Answer<'a>> {
    let unrouted = |what: &str| {
        syn::Error::new(
            span,
            format!(
                "{class}::{} is declared by {}, {what}; route it, or list it in `unimplemented` or `refused`",
                method.name, method.declared_in,
            ),
        )
    };
    let Some((index, entry)) = methods
        .iter()
        .enumerate()
        .find(|(_, entry)| entry.fqcn.value() == method.declared_in)
    else {
        return Err(unrouted("which no entry of `methods` covers"));
    };
    let listed = |list: &'a [Listed<()>]| list.iter().find(|listed| listed.name == method.name);
    if let Some(listed) = listed(&entry.unimplemented) {
        listed.used.set(true);
        return Ok(Answer::Unimplemented);
    }
    if let Some(listed) = entry
        .refused
        .iter()
        .find(|listed| listed.name == method.name)
    {
        listed.used.set(true);
        return Ok(Answer::Refused(&listed.value));
    }
    if let Some(handwritten) = &entry.handwritten {
        let answers = match &handwritten.members {
            None => !method.is_static,
            Some(members) => match listed(members) {
                Some(listed) => {
                    if method.is_static {
                        return Err(syn::Error::new(
                            listed.name.span(),
                            format!(
                                "`{}` is a static method; `handwritten` answers instance methods",
                                listed.name
                            ),
                        ));
                    }
                    listed.used.set(true);
                    true
                }
                None => false,
            },
        };
        if answers {
            handwritten.used.set(true);
            return Ok(Answer::Handwritten(&handwritten.module));
        }
    }
    if method.is_static {
        let Some(r#static) = &entry.r#static else {
            return Err(unrouted(
                "whose entry of `methods` answers no static method",
            ));
        };
        r#static.used.set(true);
        return Ok(Answer::Static(&r#static.adapters));
    }
    let Some(instance) = &entry.instance else {
        return Err(unrouted(
            "whose entry of `methods` answers no instance method",
        ));
    };
    instance.used.set(true);
    Ok(Answer::Instance {
        route: index,
        adapters: &instance.adapters,
    })
}

/// The tokens of a forwarded method's answer, with the checks of the adapter it calls.
fn answer_tokens(
    answer: &Answer<'_>,
    fqcn: &str,
    method: &decls::Method,
    span: proc_macro2::Span,
    checks: &mut Vec<proc_macro2::TokenStream>,
    kinds: &mut Vec<proc_macro2::TokenStream>,
) -> syn::Result<proc_macro2::TokenStream> {
    let name = decls::ident(&method.rust, span)?;
    Ok(match answer {
        Answer::Unimplemented => quote::quote! { ::core::result::Result::Err(call.unsupported()) },
        Answer::Refused(reason) => {
            quote::quote! { ::core::result::Result::Err(call.unavailable(#reason)) }
        }
        Answer::Handwritten(module) => quote::quote! { #module::#name(this, call) },
        Answer::Instance { route, adapters } => {
            let route = quote::format_ident!("__PhpRoute{}", route);
            checks.push(params_check(
                adapters,
                &name,
                &method.params,
                fqcn,
                &method.name,
            ));
            kinds.push(quote::quote! { #adapters::__php_kinds::#name });
            quote::quote! { #adapters::#name(&#route(::core::clone::Clone::clone(this)), call) }
        }
        Answer::Static(adapters) => {
            checks.push(params_check(
                adapters,
                &name,
                &method.params,
                fqcn,
                &method.name,
            ));
            quote::quote! { #adapters::#name(call) }
        }
    })
}

fn expand_input(input: Input) -> syn::Result<proc_macro2::TokenStream> {
    let handle = &input.handle;
    let type_error = &input.type_error;

    let mut classes = Vec::new();
    for entry in &input.classes {
        let (loaded, forwarding) = load_class(&entry.fqcn)?;
        classes.push(ClassDecl {
            entry,
            fqcn: entry.fqcn.value(),
            span: entry.fqcn.span(),
            loaded,
            forwarding,
        });
    }
    let crossed: Vec<usize> = (0..classes.len())
        .filter(|&index| !classes[index].entry.ancestor)
        .collect();
    let call_site = proc_macro2::Span::call_site();
    match (&input.class_of, crossed.len()) {
        (_, 0) => {
            return Err(syn::Error::new(
                call_site,
                "every class of `classes` is an ancestor; the entities cross as none",
            ));
        }
        (None, 1) | (Some(_), 2..) => {}
        (Some(class_of), 1) => {
            return Err(syn::Error::new_spanned(
                &class_of.body,
                "the entities cross as one class; `class_of` goes with several",
            ));
        }
        (None, _) => return Err(missing("class_of", call_site)),
    }

    let mut routes = Vec::new();
    for (index, methods) in input.methods.iter().enumerate() {
        let Some(Instance {
            target,
            borrow,
            borrow_mut,
            deref,
            ..
        }) = &methods.instance
        else {
            continue;
        };
        let route = quote::format_ident!("__PhpRoute{}", index);
        routes.push(quote::quote! {
            #[derive(Clone)]
            struct #route(#handle);
        });
        if let Some(job) = &input.handlers.job {
            routes.push(quote::quote! {
                impl crate::php_worker::rpc::binding::JobReceiver for #route {
                    fn job(
                        &self,
                        call: &crate::php_worker::rpc::entity::Call<'_>,
                    ) -> ::core::result::Result<
                        crate::php_worker::rpc::binding::JobContext,
                        ::shirabe_php_rpc::PhpThrow,
                    > {
                        #job(&self.0, call)
                    }
                }
            });
        }
        if let Some(borrow) = borrow {
            let body = borrow.apply(quote::quote! { &self.0 });
            routes.push(quote::quote! {
                impl crate::php_worker::rpc::binding::Receiver<#target> for #route {
                    type Guard<'a> = ::std::cell::Ref<'a, #target> where Self: 'a;

                    fn get(&self) -> Self::Guard<'_> #body
                }
            });
        }
        if let Some(deref) = deref {
            let body = deref.apply(quote::quote! { &self.0 });
            routes.push(quote::quote! {
                impl crate::php_worker::rpc::binding::Receiver<#target> for #route {
                    type Guard<'a> = &'a (#target) where Self: 'a;

                    fn get(&self) -> Self::Guard<'_> #body
                }
            });
        }
        if let Some(borrow_mut) = borrow_mut {
            let body = borrow_mut.apply(quote::quote! { &self.0 });
            routes.push(quote::quote! {
                impl crate::php_worker::rpc::binding::ReceiverMut<#target> for #route {
                    type Guard<'a> = ::std::cell::RefMut<'a, #target> where Self: 'a;

                    fn get_mut(&self) -> Self::Guard<'_> #body
                }
            });
        }
    }

    let mut impls = Vec::new();
    let mut checks = Vec::new();
    let mut kinds = Vec::new();
    for (index, class) in classes.iter().enumerate() {
        impls.push(class_items(&input, class, index, &mut checks, &mut kinds)?);
    }
    for methods in &input.methods {
        methods.check_used()?;
    }
    checks.push(quote::quote! {
        const _: () = assert!(
            !crate::php_worker::rpc::binding::mixes_async_and_mut(&[#(#kinds),*]),
            "this handle forwards an async fn and a `&mut self` method; answer one of them with a hand-written function",
        );
    });

    let identity = match &input.identity {
        Some(identity) => identity.apply(quote::quote! { self }),
        None => quote::quote! {{ ::std::rc::Rc::as_ptr(self) as *const () }},
    };
    let class_body = match &input.class_of {
        None => {
            let bindings = quote::format_ident!("__PHP_BINDINGS_{}", crossed[0]);
            quote::quote! { ::core::result::Result::Ok(&#bindings) }
        }
        Some(class_of) => {
            let chosen = class_of.apply(quote::quote! { self });
            let arms = crossed.iter().map(|&index| {
                let class = &classes[index];
                let literal = syn::LitByteStr::new(class.fqcn.as_bytes(), class.span);
                let bindings = quote::format_ident!("__PHP_BINDINGS_{}", index);
                quote::quote! { #literal => ::core::result::Result::Ok(&#bindings), }
            });
            quote::quote! {
                let class: ::core::result::Result<&'static [u8], ::shirabe_php_rpc::PhpThrow> = #chosen;
                let class = class?;
                match class {
                    #(#arms)*
                    other => panic!(
                        "an entity crosses as {:?}, which its entity declaration does not list",
                        ::bstr::BStr::new(other)
                    ),
                }
            }
        }
    };
    let clone = input.handlers.clone.as_ref().map(|function| {
        quote::quote! {
            fn clone_entity(&self) -> ::core::result::Result<Self, ::shirabe_php_rpc::PhpThrow> {
                #function(self)
            }
        }
    });
    for (name, _) in &input.handlers.properties {
        let declared = classes.iter().any(|class| {
            class
                .loaded
                .decl
                .properties
                .iter()
                .any(|property| !property.is_static && *name == property.name)
        });
        if !declared {
            return Err(syn::Error::new(
                name.span(),
                format!("these classes declare no instance property `{name}`"),
            ));
        }
    }
    let property = (!input.handlers.properties.is_empty()).then(|| {
        let arms = input.handlers.properties.iter().map(|(name, function)| {
            let literal = syn::LitByteStr::new(name.to_string().as_bytes(), name.span());
            quote::quote! { #literal => #function(self, access), }
        });
        quote::quote! {
            fn property(
                &self,
                access: crate::php_worker::rpc::entity::PropertyAccess<'_>,
            ) -> ::core::result::Result<::shirabe_php_rpc::RpcValue, ::shirabe_php_rpc::PhpThrow> {
                match &**access.name() {
                    #(#arms)*
                    _ => crate::php_worker::rpc::entity::unavailable_property(&access),
                }
            }
        }
    });
    let worker_method = input.handlers.worker_method.as_ref().map(|function| {
        quote::quote! {
            fn worker_method(
                &self,
                call: &mut crate::php_worker::rpc::entity::Call<'_>,
            ) -> ::core::option::Option<
                ::core::result::Result<::shirabe_php_rpc::RpcValue, ::shirabe_php_rpc::PhpThrow>,
            > {
                #function(self, call)
            }
        }
    });
    let tracking = classes.iter().map(|class| &class.loaded.tracking);

    Ok(quote::quote! {
        const _: () = {
            #(#tracking)*

            impl crate::php_worker::rpc::entity::Entity for #handle {
                const EXPECTED: &'static [u8] = #type_error;

                fn address(&self) -> *const () #identity

                fn class(
                    &self,
                ) -> ::core::result::Result<
                    &'static crate::php_worker::rpc::entity::Bindings<Self>,
                    ::shirabe_php_rpc::PhpThrow,
                > {
                    #class_body
                }

                #clone

                #property

                #worker_method
            }

            #(#routes)*

            #(#impls)*

            #(#checks)*
        };
    })
}

/// The answer of a `CallStaticMethod` sent as a class: its forwarded static methods.
fn static_router(
    methods: &[Methods],
    fqcn: &str,
    span: proc_macro2::Span,
    decl: &decls::Decl,
    checks: &mut Vec<proc_macro2::TokenStream>,
    kinds: &mut Vec<proc_macro2::TokenStream>,
) -> syn::Result<proc_macro2::TokenStream> {
    let mut arms = Vec::new();
    for method in &decl.methods {
        if !method.is_static || !method.forwarded() {
            continue;
        }
        let literal = syn::LitByteStr::new(method.name.as_bytes(), span);
        let answer = resolve(methods, fqcn, span, method)?;
        let answer = answer_tokens(&answer, fqcn, method, span, checks, kinds)?;
        arms.push(quote::quote! { #literal => #answer, });
    }

    Ok(quote::quote! {
        fn call_static(
            call: &mut crate::php_worker::rpc::entity::Call<'_>,
        ) -> ::core::result::Result<::shirabe_php_rpc::RpcValue, ::shirabe_php_rpc::PhpThrow> {
            match &**call.method() {
                #(#arms)*
                _ => crate::php_worker::rpc::entity::protocol_violation(call),
            }
        }
    })
}

/// A class's dispatch of the calls an entity crossing as it receives, its `Bindings`, and the
/// answers of its constructor and static methods.
fn class_items(
    input: &Input,
    class: &ClassDecl<'_>,
    index: usize,
    checks: &mut Vec<proc_macro2::TokenStream>,
    kinds: &mut Vec<proc_macro2::TokenStream>,
) -> syn::Result<proc_macro2::TokenStream> {
    let handle = &input.handle;
    let decl = &class.loaded.decl;
    let span = class.span;
    let fqcn = &class.fqcn;

    let mut arms = Vec::new();
    for method in &decl.methods {
        if method.is_static || !method.forwarded() {
            continue;
        }
        let literal = syn::LitByteStr::new(method.name.as_bytes(), span);
        let answer = resolve(&input.methods, fqcn, span, method)?;
        let answer = answer_tokens(&answer, fqcn, method, span, checks, kinds)?;
        arms.push(quote::quote! { #literal => #answer, });
    }

    let forwarded = decl
        .constructor
        .as_ref()
        .filter(|constructor| constructor.forwarded);
    let construct = match (forwarded, &class.entry.constructor) {
        (None, None) => quote::quote! { crate::php_worker::rpc::entity::protocol_violation(call) },
        (None, Some(_)) => {
            return Err(syn::Error::new(
                span,
                format!("{fqcn} does not forward its constructor; remove `constructor`"),
            ));
        }
        (Some(_), None) => {
            return Err(syn::Error::new(
                span,
                format!(
                    "{fqcn} forwards its constructor; give `constructor`, as `unimplemented` or `refused(...)` if the Rust side does not answer it"
                ),
            ));
        }
        (Some(_), Some(Constructor::Unimplemented)) => {
            quote::quote! { ::core::result::Result::Err(call.unconstructible()) }
        }
        (Some(_), Some(Constructor::Refused(reason))) => {
            quote::quote! { ::core::result::Result::Err(call.unavailable(#reason)) }
        }
        (Some(constructor), Some(answered)) => {
            let built = match answered {
                Constructor::Adapters { module, into } => {
                    let construct = quote::format_ident!("__construct");
                    checks.push(params_check(
                        module,
                        &construct,
                        &constructor.params,
                        fqcn,
                        "__construct",
                    ));
                    let into = into.apply(quote::quote! { value });
                    quote::quote! {{
                        let value = #module::__construct(call)?;
                        #into
                    }}
                }
                Constructor::Handwritten { function } => quote::quote! { #function(call)? },
                Constructor::Unimplemented | Constructor::Refused(_) => {
                    unreachable!("matched above")
                }
            };
            quote::quote! {{
                let entity: #handle = #built;
                crate::php_worker::rpc::entity::constructed(call.class(), entity)
            }}
        }
    };
    let call_static = static_router(&input.methods, fqcn, span, decl, checks, kinds)?;

    let forwarding = &class.forwarding;
    let dispatch = quote::format_ident!("__php_dispatch_{}", index);
    let bindings = quote::format_ident!("__PHP_BINDINGS_{}", index);
    let name = syn::LitByteStr::new(fqcn.as_bytes(), span);

    Ok(quote::quote! {
        fn #dispatch(
            this: &#handle,
            call: &mut crate::php_worker::rpc::entity::Call<'_>,
        ) -> ::core::result::Result<::shirabe_php_rpc::RpcValue, ::shirabe_php_rpc::PhpThrow> {
            match &**call.method() {
                #(#arms)*
                _ => crate::php_worker::rpc::entity::protocol_violation(call),
            }
        }

        static #bindings: crate::php_worker::rpc::entity::Bindings<#handle> =
            crate::php_worker::rpc::entity::Bindings {
                name: #name,
                dispatch: #dispatch,
            };

        impl crate::php_worker::rpc::entity::ForwardedClass for #forwarding::PhpClass {
            fn construct(
                call: &mut crate::php_worker::rpc::entity::Call<'_>,
            ) -> ::core::result::Result<u64, ::shirabe_php_rpc::PhpThrow> {
                #construct
            }

            #call_static
        }
    })
}

/// A compile-time check that the stub of a class sends the arguments the adapter decodes: the
/// parameter types the adapter was generated against are the ones the class's stub declares.
fn params_check(
    module: &syn::Path,
    name: &syn::Ident,
    params: &[decls::Param],
    fqcn: &str,
    member: &str,
) -> proc_macro2::TokenStream {
    let signatures = decls::signatures(params);
    let message = format!(
        "the stub of {fqcn} declares the parameters of {member} with other types than the adapter decodes; answer it with #[php(custom = ...)]"
    );
    quote::quote! {
        const _: () = assert!(
            crate::php_worker::rpc::binding::params_match(#module::__php_params::#name, #signatures),
            #message,
        );
    }
}

/// The input of `php_statics!`: a class with no entity, whose static methods are forwarded.
struct StaticsInput {
    class: syn::LitStr,
    methods: Vec<Methods>,
}

impl syn::parse::Parse for StaticsInput {
    fn parse(input: syn::parse::ParseStream<'_>) -> syn::Result<Self> {
        let mut class = None;
        let mut methods = None;
        parse_fields(input, &["class", "methods"], |key, input| {
            match key.to_string().as_str() {
                "class" => class = Some(input.parse()?),
                "methods" => {
                    methods = Some(parse_methods(
                        input,
                        &["static", "unimplemented", "refused"],
                    )?);
                }
                _ => unreachable!("parse_fields checks the keys"),
            }
            Ok(())
        })?;
        let span = proc_macro2::Span::call_site();
        Ok(StaticsInput {
            class: class.ok_or_else(|| missing("class", span))?,
            methods: methods.ok_or_else(|| missing("methods", span))?,
        })
    }
}

pub fn expand_statics(input: proc_macro2::TokenStream) -> proc_macro2::TokenStream {
    match syn::parse2(input).and_then(expand_statics_input) {
        Ok(tokens) => tokens,
        Err(error) => error.to_compile_error(),
    }
}

fn expand_statics_input(input: StaticsInput) -> syn::Result<proc_macro2::TokenStream> {
    let fqcn = input.class.value();
    let span = input.class.span();
    let (loaded, forwarding) = load_class(&input.class)?;
    let mut checks = Vec::new();
    let mut kinds = Vec::new();
    let call_static = static_router(
        &input.methods,
        &fqcn,
        span,
        &loaded.decl,
        &mut checks,
        &mut kinds,
    )?;
    for methods in &input.methods {
        methods.check_used()?;
    }
    let tracking = loaded.tracking;

    Ok(quote::quote! {
        const _: () = {
            #tracking

            impl crate::php_worker::rpc::entity::ForwardedClass for #forwarding::PhpClass {
                fn construct(
                    call: &mut crate::php_worker::rpc::entity::Call<'_>,
                ) -> ::core::result::Result<u64, ::shirabe_php_rpc::PhpThrow> {
                    crate::php_worker::rpc::entity::protocol_violation(call)
                }

                #call_static
            }

            #(#checks)*
        };
    })
}
