//! `php_entity!`: the `Entity` impl of a handle type, and the routing of the members its classes
//! forward to the adapters `#[php_binding]` generated.

use crate::decls;

/// `|handle| expr`: what an entity declaration computes from a handle.
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

enum Source {
    /// The adapters of an item answering instance methods, reached through a route.
    Instance {
        module: syn::Path,
        target: syn::Type,
        via: Option<Access>,
        mutable: Option<Access>,
        deref: Option<Access>,
        job: Option<syn::Path>,
    },
    /// The adapters of an item answering static methods.
    Static { module: syn::Path },
    /// Hand-written functions taking the handle itself, for a PHP type the Rust side has no
    /// trait or type for.
    Custom { module: syn::Path },
}

/// What answers the members a PHP type declares, or only the ones listed.
struct Entry {
    fqcn: syn::LitStr,
    source: Source,
    only: Option<Vec<String>>,
}

impl syn::parse::Parse for Entry {
    fn parse(input: syn::parse::ParseStream<'_>) -> syn::Result<Self> {
        let fqcn: syn::LitStr = input.parse()?;
        input.parse::<syn::Token![:]>()?;
        let kind: syn::Ident = syn::ext::IdentExt::parse_any(input)?;
        let module: syn::Path = input.parse()?;
        let source = match kind.to_string().as_str() {
            "instance" => {
                input.parse::<syn::Token![for]>()?;
                let target: syn::Type = input.parse()?;
                let content;
                syn::braced!(content in input);
                let mut via = None;
                let mut mutable = None;
                let mut deref = None;
                let mut job = None;
                while !content.is_empty() {
                    let key: syn::Ident = syn::ext::IdentExt::parse_any(&content)?;
                    content.parse::<syn::Token![:]>()?;
                    match key.to_string().as_str() {
                        "via" => via = Some(content.parse()?),
                        "mut" => mutable = Some(content.parse()?),
                        "deref" => deref = Some(content.parse()?),
                        "job" => job = Some(content.parse()?),
                        _ => {
                            return Err(syn::Error::new(
                                key.span(),
                                "expected `via`, `mut`, `deref` or `job`",
                            ));
                        }
                    }
                    if !content.is_empty() {
                        content.parse::<syn::Token![,]>()?;
                    }
                }
                Source::Instance {
                    module,
                    target: with_static_bound(target),
                    via,
                    mutable,
                    deref,
                    job,
                }
            }
            "static" => Source::Static { module },
            "custom" => Source::Custom { module },
            _ => {
                return Err(syn::Error::new(
                    kind.span(),
                    "expected `instance`, `static` or `custom`",
                ));
            }
        };
        let mut only = None;
        if input.peek(syn::Ident) && input.fork().parse::<syn::Ident>()? == "only" {
            input.parse::<syn::Ident>()?;
            let content;
            syn::bracketed!(content in input);
            only = Some(
                content
                    .parse_terminated(syn::ext::IdentExt::parse_any, syn::Token![,])?
                    .into_iter()
                    .map(|ident: syn::Ident| ident.to_string())
                    .collect(),
            );
        }

        Ok(Entry { fqcn, source, only })
    }
}

/// How a class's forwarded constructor builds the entity.
enum Construct {
    /// The adapter of the inherent impl answering the constructor, and how the value it builds
    /// becomes a handle.
    Adapter { module: syn::Path, into: Access },
    /// A hand-written function answering the call with the handle.
    Custom { function: syn::Path },
}

struct ConstructEntry {
    fqcn: syn::LitStr,
    construct: Construct,
}

impl syn::parse::Parse for ConstructEntry {
    fn parse(input: syn::parse::ParseStream<'_>) -> syn::Result<Self> {
        let fqcn: syn::LitStr = input.parse()?;
        input.parse::<syn::Token![:]>()?;
        let custom = input.peek(syn::Ident) && input.fork().parse::<syn::Ident>()? == "custom";
        let construct = if custom {
            input.parse::<syn::Ident>()?;
            Construct::Custom {
                function: input.parse()?,
            }
        } else {
            let module: syn::Path = input.parse()?;
            input.parse::<syn::Token![=>]>()?;
            Construct::Adapter {
                module,
                into: input.parse()?,
            }
        };

        Ok(ConstructEntry { fqcn, construct })
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

/// The members the Rust side does not answer: `unimplemented: [...]` and `refused: { ... }`,
/// each naming a member (`"getFoo"`, for every class) or a member of a class and its subclasses
/// (`"Composer\\Foo::getFoo"`; a constructor only for that class).
#[derive(Default)]
struct Statuses {
    unimplemented: Vec<syn::LitStr>,
    refused: Vec<(syn::LitStr, syn::LitStr)>,
}

enum Status<'a> {
    Unimplemented,
    Refused(&'a syn::LitStr),
}

impl Statuses {
    fn parse_unimplemented(&mut self, input: syn::parse::ParseStream<'_>) -> syn::Result<()> {
        let content;
        syn::bracketed!(content in input);
        self.unimplemented.extend(
            content.parse_terminated(<syn::LitStr as syn::parse::Parse>::parse, syn::Token![,])?,
        );
        Ok(())
    }

    fn parse_refused(&mut self, input: syn::parse::ParseStream<'_>) -> syn::Result<()> {
        let content;
        syn::braced!(content in input);
        while !content.is_empty() {
            let member: syn::LitStr = content.parse()?;
            content.parse::<syn::Token![:]>()?;
            let reason: syn::LitStr = content.parse()?;
            self.refused.push((member, reason));
            if !content.is_empty() {
                content.parse::<syn::Token![,]>()?;
            }
        }
        Ok(())
    }

    /// Whether `entry` names `member` of a class whose ancestors (itself first) are `chain`.
    fn names(entry: &str, chain: &[&str], member: &str) -> bool {
        match entry.rsplit_once("::") {
            None => entry == member,
            Some((class, name)) => {
                name == member
                    && if member == "__construct" {
                        chain.first() == Some(&class)
                    } else {
                        chain.contains(&class)
                    }
            }
        }
    }

    fn of(&self, chain: &[&str], member: &str) -> Option<Status<'_>> {
        if self
            .unimplemented
            .iter()
            .any(|entry| Self::names(&entry.value(), chain, member))
        {
            return Some(Status::Unimplemented);
        }
        self.refused
            .iter()
            .find(|(entry, _)| Self::names(&entry.value(), chain, member))
            .map(|(_, reason)| Status::Refused(reason))
    }

    /// Fails on an entry naming no member a class forwards, so that the lists cannot go stale.
    fn check_used(&self, forwarded: &[(Vec<&str>, Vec<&str>)]) -> syn::Result<()> {
        let entries = self
            .unimplemented
            .iter()
            .chain(self.refused.iter().map(|(entry, _)| entry));
        for entry in entries {
            let used = forwarded.iter().any(|(chain, members)| {
                members
                    .iter()
                    .any(|member| Self::names(&entry.value(), chain, member))
            });
            if !used {
                return Err(syn::Error::new(
                    entry.span(),
                    format!("`{}` names no member these classes forward", entry.value()),
                ));
            }
        }
        Ok(())
    }
}

fn status_answer(status: &Status<'_>) -> proc_macro2::TokenStream {
    match status {
        Status::Unimplemented => quote::quote! { ::core::result::Result::Err(call.unsupported()) },
        Status::Refused(reason) => {
            quote::quote! { ::core::result::Result::Err(call.unavailable(#reason)) }
        }
    }
}

/// The class an entity crosses as: always one, or one chosen from the handle.
enum Class {
    Fixed(syn::LitStr),
    Chosen(Access),
}

struct Input {
    handle: syn::Type,
    expected: syn::LitByteStr,
    address: Option<Access>,
    class: Class,
    classes: Vec<syn::LitStr>,
    clone: Option<syn::Path>,
    property: Option<syn::Path>,
    worker_method: Option<syn::Path>,
    types: Vec<Entry>,
    constructors: Vec<ConstructEntry>,
    statuses: Statuses,
}

impl syn::parse::Parse for Input {
    fn parse(input: syn::parse::ParseStream<'_>) -> syn::Result<Self> {
        let mut handle = None;
        let mut expected = None;
        let mut address = None;
        let mut class = None;
        let mut classes = None;
        let mut clone = None;
        let mut property = None;
        let mut worker_method = None;
        let mut types = None;
        let mut constructors = Vec::new();
        let mut statuses = Statuses::default();
        while !input.is_empty() {
            let key: syn::Ident = input.parse()?;
            input.parse::<syn::Token![:]>()?;
            match key.to_string().as_str() {
                "handle" => handle = Some(input.parse()?),
                "expected" => {
                    let text: syn::LitStr = input.parse()?;
                    expected = Some(syn::LitByteStr::new(text.value().as_bytes(), text.span()));
                }
                "address" => address = Some(input.parse()?),
                "class" => {
                    class = Some(if input.peek(syn::LitStr) {
                        Class::Fixed(input.parse()?)
                    } else {
                        Class::Chosen(input.parse()?)
                    });
                }
                "classes" => {
                    let content;
                    syn::bracketed!(content in input);
                    classes = Some(
                        content
                            .parse_terminated(
                                <syn::LitStr as syn::parse::Parse>::parse,
                                syn::Token![,],
                            )?
                            .into_iter()
                            .collect(),
                    );
                }
                "clone" => clone = Some(input.parse()?),
                "property" => property = Some(input.parse()?),
                "worker_method" => worker_method = Some(input.parse()?),
                "unimplemented" => statuses.parse_unimplemented(input)?,
                "refused" => statuses.parse_refused(input)?,
                "types" => {
                    let content;
                    syn::braced!(content in input);
                    types = Some(
                        content
                            .parse_terminated(<Entry as syn::parse::Parse>::parse, syn::Token![,])?
                            .into_iter()
                            .collect(),
                    );
                }
                "constructors" => {
                    let content;
                    syn::braced!(content in input);
                    constructors = content
                        .parse_terminated(
                            <ConstructEntry as syn::parse::Parse>::parse,
                            syn::Token![,],
                        )?
                        .into_iter()
                        .collect();
                }
                _ => {
                    return Err(syn::Error::new(
                        key.span(),
                        "expected `handle`, `expected`, `address`, `class`, `classes`, `clone`, `property`, `worker_method`, `types`, `constructors`, `unimplemented` or `refused`",
                    ));
                }
            }
            if !input.is_empty() {
                input.parse::<syn::Token![,]>()?;
            }
        }
        let missing = |what: &str| {
            syn::Error::new(proc_macro2::Span::call_site(), format!("missing `{what}`"))
        };
        let class = class.ok_or_else(|| missing("class"))?;
        let classes = match (&class, classes) {
            (Class::Fixed(fqcn), None) => vec![fqcn.clone()],
            (Class::Chosen(_), Some(classes)) => classes,
            (Class::Fixed(fqcn), Some(_)) => {
                return Err(syn::Error::new(
                    fqcn.span(),
                    "`classes` goes with a `class` chosen from the handle",
                ));
            }
            (Class::Chosen(_), None) => return Err(missing("classes")),
        };

        Ok(Input {
            handle: handle.ok_or_else(|| missing("handle"))?,
            expected: expected.ok_or_else(|| missing("expected"))?,
            address,
            class,
            classes,
            clone,
            property,
            worker_method,
            types: types.ok_or_else(|| missing("types"))?,
            constructors,
            statuses,
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
struct ClassDecl {
    fqcn: String,
    span: proc_macro2::Span,
    loaded: decls::Loaded,
    forwarding: proc_macro2::TokenStream,
}

fn expand_input(input: Input) -> syn::Result<proc_macro2::TokenStream> {
    let handle = &input.handle;
    let expected = &input.expected;

    let mut classes = Vec::new();
    for lit in &input.classes {
        let fqcn = lit.value();
        let loaded = decls::load(&fqcn, lit.span())?;
        let mut forwarding = quote::quote! { crate::php_worker::rpc::forwarding };
        for segment in &loaded.decl.rust_module {
            let segment = decls::ident(segment, lit.span())?;
            forwarding = quote::quote! { #forwarding::#segment };
        }
        classes.push(ClassDecl {
            fqcn,
            span: lit.span(),
            loaded,
            forwarding,
        });
    }

    let mut routes = Vec::new();
    for (index, entry) in input.types.iter().enumerate() {
        let Source::Instance {
            target,
            via,
            mutable,
            deref,
            job,
            ..
        } = &entry.source
        else {
            continue;
        };
        let route = quote::format_ident!("__PhpRoute{}", index);
        routes.push(quote::quote! {
            #[derive(Clone)]
            struct #route(#handle);
        });
        if let Some(job) = job {
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
        if let Some(via) = via {
            let body = via.apply(quote::quote! { &self.0 });
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
                    type Guard<'a> = &'a #target where Self: 'a;

                    fn get(&self) -> Self::Guard<'_> #body
                }
            });
        }
        if let Some(mutable) = mutable {
            let body = mutable.apply(quote::quote! { &self.0 });
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
    let mut forwarded = Vec::new();
    for (index, class) in classes.iter().enumerate() {
        impls.push(class_items(
            &input,
            &classes,
            index,
            &mut checks,
            &mut kinds,
        )?);
        forwarded.push((chain(&classes, class), forwarded_members(class)));
    }
    input.statuses.check_used(&forwarded)?;
    checks.push(quote::quote! {
        const _: () = assert!(
            !crate::php_worker::rpc::binding::mixes_async_and_mut(&[#(#kinds),*]),
            "this handle forwards an async fn and a `&mut self` method; answer one of them with a hand-written function",
        );
    });

    let address = match &input.address {
        Some(address) => address.apply(quote::quote! { self }),
        None => quote::quote! {{ ::std::rc::Rc::as_ptr(self) as *const () }},
    };
    let class_body = match &input.class {
        Class::Fixed(_) => quote::quote! { ::core::result::Result::Ok(&__PHP_BINDINGS_0) },
        Class::Chosen(choose) => {
            let chosen = choose.apply(quote::quote! { self });
            let arms = classes.iter().enumerate().map(|(index, class)| {
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
    let clone = input.clone.as_ref().map(|function| {
        quote::quote! {
            fn clone_entity(&self) -> ::core::result::Result<Self, ::shirabe_php_rpc::PhpThrow> {
                #function(self)
            }
        }
    });
    let property = input.property.as_ref().map(|function| {
        quote::quote! {
            fn property(
                &self,
                access: crate::php_worker::rpc::entity::PropertyAccess<'_>,
            ) -> ::core::result::Result<::shirabe_php_rpc::RpcValue, ::shirabe_php_rpc::PhpThrow> {
                #function(self, access)
            }
        }
    });
    let worker_method = input.worker_method.as_ref().map(|function| {
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
                const EXPECTED: &'static [u8] = #expected;

                fn address(&self) -> *const () #address

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

/// A class and its ancestors among `classes`, the class first.
fn chain<'a>(classes: &'a [ClassDecl], class: &'a ClassDecl) -> Vec<&'a str> {
    let mut chain = vec![class.fqcn.as_str()];
    let mut current = class;
    while let Some(parent) = current
        .loaded
        .decl
        .parent
        .as_ref()
        .and_then(|parent| classes.iter().find(|other| &other.fqcn == parent))
    {
        chain.push(parent.fqcn.as_str());
        current = parent;
    }
    chain
}

/// The members a class's stub sends to the Rust side.
fn forwarded_members(class: &ClassDecl) -> Vec<&str> {
    let decl = &class.loaded.decl;
    let mut members: Vec<&str> = decl
        .methods
        .iter()
        .filter(|method| method.forwarded())
        .map(|method| method.name.as_str())
        .collect();
    if decl
        .constructor
        .as_ref()
        .is_some_and(|constructor| constructor.forwarded)
    {
        members.push("__construct");
    }
    members
}

/// The answer of a method routed to `types`, with the checks of the adapter it calls.
fn route(
    input: &Input,
    class: &ClassDecl,
    method: &decls::Method,
    checks: &mut Vec<proc_macro2::TokenStream>,
    kinds: &mut Vec<proc_macro2::TokenStream>,
) -> syn::Result<proc_macro2::TokenStream> {
    let fqcn = &class.fqcn;
    let span = class.span;
    let name = decls::ident(&method.rust, span)?;
    let found = input.types.iter().enumerate().find(|(_, entry)| {
        entry.fqcn.value() == method.declared_in
            && entry
                .only
                .as_ref()
                .is_none_or(|only| only.contains(&method.name))
            && matches!(
                (&entry.source, method.is_static),
                (Source::Instance { .. } | Source::Custom { .. }, false)
                    | (Source::Static { .. }, true)
            )
    });
    let Some((index, entry)) = found else {
        return Err(syn::Error::new(
            span,
            format!(
                "{fqcn}::{} is declared by {}, whose {} methods no entry of `types` answers; route it, or list it in `unimplemented` or `refused`",
                method.name,
                method.declared_in,
                if method.is_static {
                    "static"
                } else {
                    "instance"
                },
            ),
        ));
    };
    Ok(match &entry.source {
        Source::Instance { module, .. } => {
            let route = quote::format_ident!("__PhpRoute{}", index);
            checks.push(params_check(
                module,
                &name,
                &method.params,
                fqcn,
                &method.name,
            ));
            kinds.push(quote::quote! { #module::__php_kinds::#name });
            quote::quote! { #module::#name(&#route(::core::clone::Clone::clone(this)), call) }
        }
        Source::Custom { module } => quote::quote! { #module::#name(this, call) },
        Source::Static { module } => {
            checks.push(params_check(
                module,
                &name,
                &method.params,
                fqcn,
                &method.name,
            ));
            quote::quote! { #module::#name(call) }
        }
    })
}

/// The answer of a `CallStaticMethod` sent as a class: its forwarded static methods.
fn static_router(
    input: &Input,
    class: &ClassDecl,
    chain: &[&str],
    checks: &mut Vec<proc_macro2::TokenStream>,
    kinds: &mut Vec<proc_macro2::TokenStream>,
) -> syn::Result<proc_macro2::TokenStream> {
    let mut arms = Vec::new();
    for method in &class.loaded.decl.methods {
        if !method.is_static || !method.forwarded() {
            continue;
        }
        let literal = syn::LitByteStr::new(method.name.as_bytes(), class.span);
        let answer = match input.statuses.of(chain, &method.name) {
            Some(status) => status_answer(&status),
            None => route(input, class, method, checks, kinds)?,
        };
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
    classes: &[ClassDecl],
    index: usize,
    checks: &mut Vec<proc_macro2::TokenStream>,
    kinds: &mut Vec<proc_macro2::TokenStream>,
) -> syn::Result<proc_macro2::TokenStream> {
    let handle = &input.handle;
    let class = &classes[index];
    let decl = &class.loaded.decl;
    let span = class.span;
    let fqcn = &class.fqcn;
    let chain = chain(classes, class);

    let mut arms = Vec::new();
    for method in &decl.methods {
        if method.is_static || !method.forwarded() {
            continue;
        }
        let literal = syn::LitByteStr::new(method.name.as_bytes(), span);
        let answer = match input.statuses.of(&chain, &method.name) {
            Some(status) => status_answer(&status),
            None => route(input, class, method, checks, kinds)?,
        };
        arms.push(quote::quote! { #literal => #answer, });
    }

    let construct = match &decl.constructor {
        Some(constructor) if constructor.forwarded => {
            match input.statuses.of(&chain, "__construct") {
                Some(Status::Unimplemented) => {
                    quote::quote! { ::core::result::Result::Err(call.unconstructible()) }
                }
                Some(Status::Refused(reason)) => {
                    quote::quote! { ::core::result::Result::Err(call.unavailable(#reason)) }
                }
                None => {
                    let Some(entry) = input
                        .constructors
                        .iter()
                        .find(|entry| entry.fqcn.value() == *fqcn)
                    else {
                        return Err(syn::Error::new(
                            span,
                            format!(
                                "{fqcn} forwards its constructor, which no entry of `constructors` answers; answer it, or list it in `unimplemented` or `refused`"
                            ),
                        ));
                    };
                    let built = match &entry.construct {
                        Construct::Adapter { module, into } => {
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
                        Construct::Custom { function } => quote::quote! { #function(call)? },
                    };
                    quote::quote! {{
                        let entity: #handle = #built;
                        crate::php_worker::rpc::entity::constructed(call.class(), entity)
                    }}
                }
            }
        }
        _ => quote::quote! { crate::php_worker::rpc::entity::protocol_violation(call) },
    };
    let call_static = static_router(input, class, &chain, checks, kinds)?;

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
    types: Vec<Entry>,
    statuses: Statuses,
}

impl syn::parse::Parse for StaticsInput {
    fn parse(input: syn::parse::ParseStream<'_>) -> syn::Result<Self> {
        let mut class = None;
        let mut types = None;
        let mut statuses = Statuses::default();
        while !input.is_empty() {
            let key: syn::Ident = input.parse()?;
            input.parse::<syn::Token![:]>()?;
            match key.to_string().as_str() {
                "class" => class = Some(input.parse()?),
                "unimplemented" => statuses.parse_unimplemented(input)?,
                "refused" => statuses.parse_refused(input)?,
                "types" => {
                    let content;
                    syn::braced!(content in input);
                    types = Some(
                        content
                            .parse_terminated(<Entry as syn::parse::Parse>::parse, syn::Token![,])?
                            .into_iter()
                            .collect(),
                    );
                }
                _ => {
                    return Err(syn::Error::new(
                        key.span(),
                        "expected `class`, `types`, `unimplemented` or `refused`",
                    ));
                }
            }
            if !input.is_empty() {
                input.parse::<syn::Token![,]>()?;
            }
        }
        let missing = |what: &str| {
            syn::Error::new(proc_macro2::Span::call_site(), format!("missing `{what}`"))
        };

        Ok(StaticsInput {
            class: class.ok_or_else(|| missing("class"))?,
            types: types.ok_or_else(|| missing("types"))?,
            statuses,
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
    let loaded = decls::load(&fqcn, span)?;
    let mut forwarding = quote::quote! { crate::php_worker::rpc::forwarding };
    for segment in &loaded.decl.rust_module {
        let segment = decls::ident(segment, span)?;
        forwarding = quote::quote! { #forwarding::#segment };
    }
    let chain = [fqcn.as_str()];
    let mut arms = Vec::new();
    let mut checks = Vec::new();
    let mut forwarded = Vec::new();
    for method in &loaded.decl.methods {
        if !method.is_static || !method.forwarded() {
            continue;
        }
        forwarded.push(method.name.as_str());
        let literal = syn::LitByteStr::new(method.name.as_bytes(), span);
        if let Some(status) = input.statuses.of(&chain, &method.name) {
            let answer = status_answer(&status);
            arms.push(quote::quote! { #literal => #answer, });
            continue;
        }
        let name = decls::ident(&method.rust, span)?;
        let Some(module) = input.types.iter().find_map(|entry| match &entry.source {
            Source::Static { module }
                if entry.fqcn.value() == method.declared_in
                    && entry
                        .only
                        .as_ref()
                        .is_none_or(|only| only.contains(&method.name)) =>
            {
                Some(module)
            }
            _ => None,
        }) else {
            return Err(syn::Error::new(
                span,
                format!(
                    "{fqcn}::{} is declared by {}, whose static methods no entry of `types` answers; route it, or list it in `unimplemented` or `refused`",
                    method.name, method.declared_in,
                ),
            ));
        };
        checks.push(params_check(
            module,
            &name,
            &method.params,
            &fqcn,
            &method.name,
        ));
        arms.push(quote::quote! { #literal => #module::#name(call), });
    }
    input.statuses.check_used(&[(chain.to_vec(), forwarded)])?;
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

                fn call_static(
                    call: &mut crate::php_worker::rpc::entity::Call<'_>,
                ) -> ::core::result::Result<::shirabe_php_rpc::RpcValue, ::shirabe_php_rpc::PhpThrow> {
                    match &**call.method() {
                        #(#arms)*
                        _ => crate::php_worker::rpc::entity::protocol_violation(call),
                    }
                }
            }

            #(#checks)*
        };
    })
}
