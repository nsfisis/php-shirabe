//! `php_entity!`: the `Entity` impl of a handle type, and the routing of the members its class
//! forwards to the adapters `#[php_binding]` generated.

use crate::decls;

/// `|handle| expr`: how a route reaches the receiver from a handle.
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

enum Source {
    /// The adapters of an item answering instance methods, reached through a route.
    Instance {
        module: syn::Path,
        target: syn::Type,
        via: Option<Access>,
        mutable: Option<Access>,
    },
    /// The adapters of an item answering static methods.
    Static { module: syn::Path },
}

/// What answers the members a PHP type declares.
struct Entry {
    fqcn: syn::LitStr,
    source: Source,
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
                while !content.is_empty() {
                    let key: syn::Ident = syn::ext::IdentExt::parse_any(&content)?;
                    content.parse::<syn::Token![:]>()?;
                    match key.to_string().as_str() {
                        "via" => via = Some(content.parse()?),
                        "mut" => mutable = Some(content.parse()?),
                        _ => return Err(syn::Error::new(key.span(), "expected `via` or `mut`")),
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
                }
            }
            "static" => Source::Static { module },
            _ => {
                return Err(syn::Error::new(
                    kind.span(),
                    "expected `instance` or `static`",
                ));
            }
        };

        Ok(Entry { fqcn, source })
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
    expected: syn::LitByteStr,
    class: syn::LitStr,
    types: Vec<Entry>,
}

impl syn::parse::Parse for Input {
    fn parse(input: syn::parse::ParseStream<'_>) -> syn::Result<Self> {
        let mut handle = None;
        let mut expected = None;
        let mut class = None;
        let mut types = None;
        while !input.is_empty() {
            let key: syn::Ident = input.parse()?;
            input.parse::<syn::Token![:]>()?;
            match key.to_string().as_str() {
                "handle" => handle = Some(input.parse()?),
                "expected" => {
                    let text: syn::LitStr = input.parse()?;
                    expected = Some(syn::LitByteStr::new(text.value().as_bytes(), text.span()));
                }
                "class" => class = Some(input.parse()?),
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
                        "expected `handle`, `expected`, `class` or `types`",
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

        Ok(Input {
            handle: handle.ok_or_else(|| missing("handle"))?,
            expected: expected.ok_or_else(|| missing("expected"))?,
            class: class.ok_or_else(|| missing("class"))?,
            types: types.ok_or_else(|| missing("types"))?,
        })
    }
}

pub fn expand(input: proc_macro2::TokenStream) -> proc_macro2::TokenStream {
    match syn::parse2(input).and_then(expand_input) {
        Ok(tokens) => tokens,
        Err(error) => error.to_compile_error(),
    }
}

fn expand_input(input: Input) -> syn::Result<proc_macro2::TokenStream> {
    let class = input.class.value();
    let span = input.class.span();
    let loaded = decls::load(&class, span)?;
    let decl = &loaded.decl;
    let handle = &input.handle;
    let expected = &input.expected;
    let mut forwarding = quote::quote! { crate::php_worker::rpc::forwarding };
    for segment in &decl.rust_module {
        let segment = decls::ident(segment, span)?;
        forwarding = quote::quote! { #forwarding::#segment };
    }

    let mut routes = Vec::new();
    for (index, entry) in input.types.iter().enumerate() {
        let Source::Instance {
            target,
            via,
            mutable,
            ..
        } = &entry.source
        else {
            continue;
        };
        let route = quote::format_ident!("__PhpRoute{}", index);
        routes.push(quote::quote! {
            struct #route<'h>(&'h #handle);
        });
        if let Some(Access { pat, body }) = via {
            routes.push(quote::quote! {
                impl<'h> crate::php_worker::rpc::binding::Receiver<#target> for #route<'h> {
                    type Guard<'a> = ::std::cell::Ref<'a, #target> where Self: 'a;

                    fn get(&self) -> Self::Guard<'_> {
                        let #pat = self.0;
                        #body
                    }
                }
            });
        }
        if let Some(Access { pat, body }) = mutable {
            routes.push(quote::quote! {
                impl<'h> crate::php_worker::rpc::binding::ReceiverMut<#target> for #route<'h> {
                    type Guard<'a> = ::std::cell::RefMut<'a, #target> where Self: 'a;

                    fn get_mut(&self) -> Self::Guard<'_> {
                        let #pat = self.0;
                        #body
                    }
                }
            });
        }
    }

    let mut members = Vec::new();
    for method in &decl.methods {
        if !method.forwarded || method.status.as_deref() != Some("implemented") {
            continue;
        }
        let name = decls::ident(&method.rust, span)?;
        let found = input.types.iter().enumerate().find(|(_, entry)| {
            entry.fqcn.value() == method.declared_in
                && matches!(
                    (&entry.source, method.is_static),
                    (Source::Instance { .. }, false) | (Source::Static { .. }, true)
                )
        });
        let Some((index, entry)) = found else {
            return Err(syn::Error::new(
                span,
                format!(
                    "{class}::{} is declared by {}, whose {} methods no entry of `types` answers",
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
        members.push(match &entry.source {
            Source::Instance { module, .. } => {
                let route = quote::format_ident!("__PhpRoute{}", index);
                quote::quote! {
                    fn #name(
                        this: &Self::Handle,
                        call: &mut crate::php_worker::rpc::entity::Call<'_>,
                    ) -> ::core::result::Result<::shirabe_php_rpc::RpcValue, ::shirabe_php_rpc::PhpThrow> {
                        #module::#name(&#route(this), call)
                    }
                }
            }
            Source::Static { module } => quote::quote! {
                fn #name(
                    call: &mut crate::php_worker::rpc::entity::Call<'_>,
                ) -> ::core::result::Result<::shirabe_php_rpc::RpcValue, ::shirabe_php_rpc::PhpThrow> {
                    #module::#name(call)
                }
            },
        });
    }
    if let Some(constructor) = &decl.constructor
        && constructor.forwarded
        && constructor.status.as_deref() == Some("implemented")
    {
        return Err(syn::Error::new(
            span,
            format!("{class}: php_entity! does not route constructors yet"),
        ));
    }

    let tracking = loaded.tracking;

    Ok(quote::quote! {
        const _: () = {
            #tracking

            impl crate::php_worker::rpc::entity::Entity for #handle {
                const EXPECTED: &'static [u8] = #expected;

                fn address(&self) -> *const () {
                    ::std::rc::Rc::as_ptr(self) as *const ()
                }

                fn class(
                    &self,
                ) -> ::core::result::Result<
                    &'static crate::php_worker::rpc::entity::Bindings<Self>,
                    ::shirabe_php_rpc::PhpThrow,
                > {
                    ::core::result::Result::Ok(&#forwarding::BINDINGS)
                }
            }

            #(#routes)*

            impl #forwarding::Forwarding for #forwarding::PhpClass {
                type Handle = #handle;

                #(#members)*
            }
        };
    })
}
