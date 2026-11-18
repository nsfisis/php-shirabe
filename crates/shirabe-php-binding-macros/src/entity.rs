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
                while !content.is_empty() {
                    let key: syn::Ident = syn::ext::IdentExt::parse_any(&content)?;
                    content.parse::<syn::Token![:]>()?;
                    match key.to_string().as_str() {
                        "via" => via = Some(content.parse()?),
                        "mut" => mutable = Some(content.parse()?),
                        "deref" => deref = Some(content.parse()?),
                        _ => {
                            return Err(syn::Error::new(
                                key.span(),
                                "expected `via`, `mut` or `deref`",
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
    types: Vec<Entry>,
    constructors: Vec<ConstructEntry>,
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
        let mut types = None;
        let mut constructors = Vec::new();
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
                        "expected `handle`, `expected`, `address`, `class`, `classes`, `clone`, `property`, `types` or `constructors`",
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
            types: types.ok_or_else(|| missing("types"))?,
            constructors,
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
            ..
        } = &entry.source
        else {
            continue;
        };
        let route = quote::format_ident!("__PhpRoute{}", index);
        routes.push(quote::quote! {
            struct #route<'h>(&'h #handle);
        });
        if let Some(via) = via {
            let body = via.apply(quote::quote! { self.0 });
            routes.push(quote::quote! {
                impl<'h> crate::php_worker::rpc::binding::Receiver<#target> for #route<'h> {
                    type Guard<'a> = ::std::cell::Ref<'a, #target> where Self: 'a;

                    fn get(&self) -> Self::Guard<'_> #body
                }
            });
        }
        if let Some(deref) = deref {
            let body = deref.apply(quote::quote! { self.0 });
            routes.push(quote::quote! {
                impl<'h> crate::php_worker::rpc::binding::Receiver<#target> for #route<'h> {
                    type Guard<'a> = &'a #target where Self: 'a;

                    fn get(&self) -> Self::Guard<'_> #body
                }
            });
        }
        if let Some(mutable) = mutable {
            let body = mutable.apply(quote::quote! { self.0 });
            routes.push(quote::quote! {
                impl<'h> crate::php_worker::rpc::binding::ReceiverMut<#target> for #route<'h> {
                    type Guard<'a> = ::std::cell::RefMut<'a, #target> where Self: 'a;

                    fn get_mut(&self) -> Self::Guard<'_> #body
                }
            });
        }
    }

    let mut impls = Vec::new();
    let mut checks = Vec::new();
    for class in &classes {
        impls.push(forwarding_impl(&input, &classes, class, &mut checks)?);
    }

    let address = match &input.address {
        Some(address) => address.apply(quote::quote! { self }),
        None => quote::quote! {{ ::std::rc::Rc::as_ptr(self) as *const () }},
    };
    let class_body = match &input.class {
        Class::Fixed(_) => {
            let forwarding = &classes[0].forwarding;
            quote::quote! { ::core::result::Result::Ok(&#forwarding::BINDINGS) }
        }
        Class::Chosen(choose) => {
            let chosen = choose.apply(quote::quote! { self });
            let arms = classes.iter().map(|class| {
                let literal = syn::LitByteStr::new(class.fqcn.as_bytes(), class.span);
                let forwarding = &class.forwarding;
                quote::quote! { #literal => ::core::result::Result::Ok(&#forwarding::BINDINGS), }
            });
            quote::quote! {
                let class: &'static [u8] = #chosen;
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
            }

            #(#routes)*

            #(#impls)*

            #(#checks)*
        };
    })
}

/// The impl of a class's `Forwarding` trait: the members the class forwards and its parent does
/// not, each routed to the adapter of the type declaring it.
fn forwarding_impl(
    input: &Input,
    classes: &[ClassDecl],
    class: &ClassDecl,
    checks: &mut Vec<proc_macro2::TokenStream>,
) -> syn::Result<proc_macro2::TokenStream> {
    let handle = &input.handle;
    let decl = &class.loaded.decl;
    let span = class.span;
    let fqcn = &class.fqcn;
    let parent = decl
        .parent
        .as_ref()
        .and_then(|parent| classes.iter().find(|other| &other.fqcn == parent));
    let inherited: Vec<&str> = parent
        .map(|parent| {
            parent
                .loaded
                .decl
                .methods
                .iter()
                .filter(|method| method.forwarded() && !method.is_static)
                .map(|method| method.name.as_str())
                .collect()
        })
        .unwrap_or_default();

    let mut members = Vec::new();
    for method in &decl.methods {
        if !method.forwarded()
            || method.status.as_deref() != Some("implemented")
            || (!method.is_static && inherited.contains(&method.name.as_str()))
        {
            continue;
        }
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
                    "{fqcn}::{} is declared by {}, whose {} methods no entry of `types` answers",
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
        let answer = quote::quote! {
            ::core::result::Result<::shirabe_php_rpc::RpcValue, ::shirabe_php_rpc::PhpThrow>
        };
        let call = quote::quote! { call: &mut crate::php_worker::rpc::entity::Call<'_> };
        members.push(match &entry.source {
            Source::Instance { module, .. } => {
                let route = quote::format_ident!("__PhpRoute{}", index);
                checks.push(params_check(
                    module,
                    &name,
                    &method.params,
                    fqcn,
                    &method.name,
                ));
                quote::quote! {
                    fn #name(this: &#handle, #call) -> #answer {
                        #module::#name(&#route(this), call)
                    }
                }
            }
            Source::Custom { module } => quote::quote! {
                fn #name(this: &#handle, #call) -> #answer {
                    #module::#name(this, call)
                }
            },
            Source::Static { module } => {
                checks.push(params_check(
                    module,
                    &name,
                    &method.params,
                    fqcn,
                    &method.name,
                ));
                quote::quote! {
                    fn #name(#call) -> #answer {
                        #module::#name(call)
                    }
                }
            }
        });
    }

    if let Some(constructor) = &decl.constructor
        && constructor.forwarded
        && constructor.status.as_deref() == Some("implemented")
    {
        let Some(entry) = input
            .constructors
            .iter()
            .find(|entry| entry.fqcn.value() == *fqcn)
        else {
            return Err(syn::Error::new(
                span,
                format!(
                    "{fqcn} forwards its constructor, which no entry of `constructors` answers"
                ),
            ));
        };
        let body = match &entry.construct {
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
                quote::quote! {
                    let value = #module::__construct(call)?;
                    ::core::result::Result::Ok(#into)
                }
            }
            Construct::Custom { function } => quote::quote! { #function(call) },
        };
        members.push(quote::quote! {
            fn __construct(
                call: &mut crate::php_worker::rpc::entity::Call<'_>,
            ) -> ::core::result::Result<#handle, ::shirabe_php_rpc::PhpThrow> {
                #body
            }
        });
    }

    let forwarding = &class.forwarding;
    let handle_type = parent
        .is_none()
        .then(|| quote::quote! { type Handle = #handle; });

    Ok(quote::quote! {
        impl #forwarding::Forwarding for #forwarding::PhpClass {
            #handle_type

            #(#members)*
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
