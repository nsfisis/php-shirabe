//! `#[php_binding]`: the adapters through which the members of a PHP type reach the Rust trait or
//! inherent impl standing for it.

use crate::decls::{self, Param};
use syn::spanned::Spanned as _;

/// The kinds of member of a PHP type an item answers.
#[derive(Debug, Clone, Copy)]
struct Members {
    instance: bool,
    statics: bool,
    constructor: bool,
}

struct Args {
    fqcn: syn::LitStr,
    members: Members,
    proxy: Option<syn::Path>,
}

impl syn::parse::Parse for Args {
    fn parse(input: syn::parse::ParseStream<'_>) -> syn::Result<Self> {
        let fqcn: syn::LitStr = input.parse()?;
        let mut proxy = None;
        let mut members = Members {
            instance: true,
            statics: true,
            constructor: true,
        };
        while !input.is_empty() {
            input.parse::<syn::Token![,]>()?;
            if input.is_empty() {
                break;
            }
            let key: syn::Ident = syn::ext::IdentExt::parse_any(input)?;
            input.parse::<syn::Token![=]>()?;
            match key.to_string().as_str() {
                "members" => {
                    let kinds: Vec<syn::Ident> = if input.peek(syn::token::Paren) {
                        let content;
                        syn::parenthesized!(content in input);
                        content
                            .parse_terminated(syn::ext::IdentExt::parse_any, syn::Token![,])?
                            .into_iter()
                            .collect()
                    } else {
                        vec![syn::ext::IdentExt::parse_any(input)?]
                    };
                    members = Members {
                        instance: false,
                        statics: false,
                        constructor: false,
                    };
                    for kind in kinds {
                        match kind.to_string().as_str() {
                            "instance" => members.instance = true,
                            "static" => members.statics = true,
                            "constructor" => members.constructor = true,
                            _ => {
                                return Err(syn::Error::new(
                                    kind.span(),
                                    "expected `instance`, `static` or `constructor`",
                                ));
                            }
                        }
                    }
                }
                "proxy" => proxy = Some(input.parse()?),
                _ => {
                    return Err(syn::Error::new(key.span(), "expected `members` or `proxy`"));
                }
            }
        }

        Ok(Args {
            fqcn,
            members,
            proxy,
        })
    }
}

/// The `#[php(...)]` annotations of a method.
#[derive(Default)]
struct MethodAttrs {
    name: Option<syn::LitStr>,
    custom: Option<syn::Path>,
    skip: bool,
    rust_only: bool,
}

/// Removes the `#[php(...)]` annotations of a method and its parameters, and reads them.
fn take_attrs(
    sig: &mut syn::Signature,
    attrs: &mut Vec<syn::Attribute>,
) -> syn::Result<(MethodAttrs, Vec<Option<syn::Type>>)> {
    let mut method = MethodAttrs::default();
    for attr in take_php(attrs) {
        attr.parse_nested_meta(|meta| {
            if meta.path.is_ident("name") {
                method.name = Some(meta.value()?.parse()?);
            } else if meta.path.is_ident("custom") {
                method.custom = Some(meta.value()?.parse()?);
            } else if meta.path.is_ident("skip") {
                method.skip = true;
            } else if meta.path.is_ident("rust_only") {
                method.rust_only = true;
            } else {
                return Err(meta.error("expected `name`, `custom`, `skip` or `rust_only`"));
            }
            Ok(())
        })?;
    }
    let mut decodes = Vec::new();
    for input in &mut sig.inputs {
        let syn::FnArg::Typed(pat_type) = input else {
            continue;
        };
        let mut decode = None;
        for attr in take_php(&mut pat_type.attrs) {
            attr.parse_nested_meta(|meta| {
                if meta.path.is_ident("decode") {
                    decode = Some(meta.value()?.parse()?);
                    Ok(())
                } else {
                    Err(meta.error("expected `decode`"))
                }
            })?;
        }
        decodes.push(decode);
    }

    Ok((method, decodes))
}

fn take_php(attrs: &mut Vec<syn::Attribute>) -> Vec<syn::Attribute> {
    let (php, rest) = std::mem::take(attrs)
        .into_iter()
        .partition(|attr| attr.path().is_ident("php"));
    *attrs = rest;
    php
}

/// A Rust method of the annotated item.
struct RustMethod {
    sig: syn::Signature,
    attrs: MethodAttrs,
    decodes: Vec<Option<syn::Type>>,
    matched: bool,
    /// Whether a trait method has a default body.
    has_default: bool,
}

pub fn expand(
    attr: proc_macro2::TokenStream,
    item: proc_macro2::TokenStream,
) -> proc_macro2::TokenStream {
    let mut item: syn::Item = match syn::parse2(item.clone()) {
        Ok(item) => item,
        Err(error) => return error.to_compile_error(),
    };
    match expand_item(attr, &mut item) {
        Ok(generated) => quote::quote! { #item #generated },
        Err(error) => {
            let error = error.to_compile_error();
            quote::quote! { #item #error }
        }
    }
}

fn expand_item(
    attr: proc_macro2::TokenStream,
    item: &mut syn::Item,
) -> syn::Result<proc_macro2::TokenStream> {
    let args: Args = syn::parse2(attr)?;
    let mut methods = Vec::new();
    let mut self_ty = None;
    let mut proxy_trait = None;
    let (target, call_base, module_name) = match item {
        syn::Item::Trait(item) => {
            if !item.generics.params.is_empty() {
                return Err(syn::Error::new(
                    item.generics.span(),
                    "#[php_binding] does not support a generic trait",
                ));
            }
            for trait_item in &mut item.items {
                if let syn::TraitItem::Fn(function) = trait_item {
                    let (attrs, decodes) = take_attrs(&mut function.sig, &mut function.attrs)?;
                    methods.push(RustMethod {
                        sig: function.sig.clone(),
                        attrs,
                        decodes,
                        matched: false,
                        has_default: function.default.is_some(),
                    });
                }
            }
            proxy_trait = Some((
                item.ident.clone(),
                item.attrs
                    .iter()
                    .filter(|attr| {
                        attr.path()
                            .segments
                            .last()
                            .is_some_and(|segment| segment.ident == "async_trait")
                    })
                    .cloned()
                    .collect::<Vec<_>>(),
            ));
            let ident = &item.ident;
            (
                quote::quote! { dyn #ident + 'static },
                quote::quote! { <dyn #ident as #ident> },
                snake_case(&ident.to_string()),
            )
        }
        syn::Item::Impl(item) if item.trait_.is_none() => {
            if !item.generics.params.is_empty() {
                return Err(syn::Error::new(
                    item.generics.span(),
                    "#[php_binding] does not support a generic impl",
                ));
            }
            for impl_item in &mut item.items {
                if let syn::ImplItem::Fn(function) = impl_item {
                    let (attrs, decodes) = take_attrs(&mut function.sig, &mut function.attrs)?;
                    methods.push(RustMethod {
                        sig: function.sig.clone(),
                        attrs,
                        decodes,
                        matched: false,
                        has_default: false,
                    });
                }
            }
            self_ty = Some((*item.self_ty).clone());
            let self_ty = &item.self_ty;
            let syn::Type::Path(path) = &**self_ty else {
                return Err(syn::Error::new(
                    self_ty.span(),
                    "#[php_binding] expects an impl of a named type",
                ));
            };
            let name = path
                .path
                .segments
                .last()
                .expect("a path has a segment")
                .ident
                .to_string();
            (
                quote::quote! { #self_ty },
                quote::quote! { <#self_ty> },
                snake_case(&name),
            )
        }
        _ => {
            return Err(syn::Error::new(
                proc_macro2::Span::call_site(),
                "#[php_binding] goes on a trait or an inherent impl",
            ));
        }
    };

    let fqcn = args.fqcn.value();
    let span = args.fqcn.span();
    let loaded = decls::load(&fqcn, span)?;
    let mut adapters = Vec::new();
    let mut checked = Vec::new();
    let mut kinds = Vec::new();
    for method in &loaded.decl.methods {
        let answers = if method.is_static {
            args.members.statics
        } else {
            args.members.instance
        };
        if !answers
            || method.declared_in != fqcn
            || method.answered_in_worker()
            || args.proxy.is_some()
        {
            continue;
        }
        let Some(rust) = find(&mut methods, &method.name, &method.rust) else {
            continue;
        };
        if rust.attrs.skip {
            continue;
        }
        let adapter_name = decls::ident(&method.rust, rust.sig.ident.span())?;
        if let Some(custom) = &rust.attrs.custom {
            adapters.push(quote::quote! { pub(crate) use #custom as #adapter_name; });
            checked.push(quote::quote! { pub(crate) const #adapter_name: ::core::option::Option<&[&[u8]]> = ::core::option::Option::None; });
            kinds.push(
                quote::quote! { pub(crate) const #adapter_name: (bool, bool) = (false, false); },
            );
            continue;
        }
        if method.func_num_args {
            return Err(syn::Error::new(
                rust.sig.span(),
                format!(
                    "{fqcn}::{} branches on func_num_args(); answer it with #[php(custom = ...)]",
                    method.name
                ),
            ));
        }
        adapters.push(adapter(
            &fqcn,
            &method.name,
            method.is_static,
            &method.params,
            rust,
            &adapter_name,
            &target,
            &call_base,
        )?);
        let signatures = decls::signatures(&method.params);
        checked.push(quote::quote! { pub(crate) const #adapter_name: ::core::option::Option<&[&[u8]]> = ::core::option::Option::Some(#signatures); });
        let is_async = rust.sig.asyncness.is_some();
        let is_mut = rust.sig.inputs.iter().any(|input| {
            matches!(input, syn::FnArg::Receiver(r) if matches!(r.kind, syn::ReceiverKind::Reference(_, _, Some(_))))
        });
        kinds.push(
            quote::quote! { pub(crate) const #adapter_name: (bool, bool) = (#is_async, #is_mut); },
        );
    }
    if let Some(index) = methods.iter().position(|method| {
        method
            .attrs
            .name
            .as_ref()
            .is_some_and(|name| name.value() == "__construct")
    }) {
        let constructor = &mut methods[index];
        constructor.matched = true;
        let span = constructor.sig.span();
        let (Some(self_ty), true, Some(declared)) =
            (&self_ty, args.members.constructor, &loaded.decl.constructor)
        else {
            return Err(syn::Error::new(
                span,
                format!("{fqcn} has no constructor this item answers"),
            ));
        };
        if declared.func_num_args {
            return Err(syn::Error::new(
                span,
                format!(
                    "{fqcn}::__construct branches on func_num_args(); answer it with #[php(custom = ...)]"
                ),
            ));
        }
        let adapter_name = quote::format_ident!("__construct");
        if let Some(custom) = &constructor.attrs.custom {
            adapters.push(quote::quote! { pub(crate) use #custom as #adapter_name; });
            checked.push(quote::quote! { pub(crate) const #adapter_name: ::core::option::Option<&[&[u8]]> = ::core::option::Option::None; });
        } else {
            adapters.push(constructor_adapter(
                &fqcn,
                &declared.params,
                constructor,
                self_ty,
            )?);
            let signatures = decls::signatures(&declared.params);
            checked.push(quote::quote! { pub(crate) const #adapter_name: ::core::option::Option<&[&[u8]]> = ::core::option::Option::Some(#signatures); });
        }
    }
    for method in &methods {
        if let (Some(name), false) = (&method.attrs.name, method.matched) {
            return Err(syn::Error::new(
                name.span(),
                format!("{fqcn} has no member `{}` this item answers", name.value()),
            ));
        }
    }

    let proxy_impl = match (&args.proxy, &proxy_trait) {
        (None, _) => None,
        (Some(proxy), Some((trait_ident, async_attrs))) => Some(proxy_impl(
            &fqcn,
            &loaded.decl,
            &methods,
            proxy,
            trait_ident,
            async_attrs,
        )?),
        (Some(proxy), None) => {
            return Err(syn::Error::new_spanned(proxy, "`proxy` goes on a trait"));
        }
    };

    let module = quote::format_ident!("__php_binding_{}", module_name);
    let tracking = loaded.tracking;

    Ok(quote::quote! {
        #[doc(hidden)]
        pub(crate) mod #module {
            #[allow(unused_imports)]
            use super::*;

            #tracking

            #(#adapters)*

            #proxy_impl

            /// The PHP parameter types each adapter decodes its arguments against, or `None`
            /// for a hand-written one.
            #[allow(non_upper_case_globals)]
            pub(crate) mod __php_params {
                #(#checked)*
            }

            /// For each adapter, whether the Rust method is an `async fn` and whether it takes
            /// `&mut self`, which `php_entity!` checks one handle does not mix.
            #[allow(non_upper_case_globals)]
            pub(crate) mod __php_kinds {
                #(#kinds)*
            }
        }
    })
}

/// The impl of a trait standing for a PHP interface for the adapter of an entity living in the
/// worker: each method calls the PHP method on the entity and decodes its answer, a promise's
/// settlement for an `async fn`. A method only the Rust side has delegates to the adapter's
/// inherent method of the same name (`#[php(rust_only)]`) or keeps the trait's default body.
fn proxy_impl(
    fqcn: &str,
    decl: &decls::Decl,
    methods: &[RustMethod],
    proxy: &syn::Path,
    trait_ident: &syn::Ident,
    async_attrs: &[syn::Attribute],
) -> syn::Result<proc_macro2::TokenStream> {
    let mut items = Vec::new();
    for rust in methods {
        let sig = &rust.sig;
        let span = sig.span();
        let fn_ident = &sig.ident;
        let mut names = Vec::new();
        for input in &sig.inputs {
            match input {
                syn::FnArg::Receiver(_) => {}
                syn::FnArg::Typed(pat_type) => match &*pat_type.pat {
                    syn::Pat::Ident(ident) => names.push(ident.ident.clone()),
                    other => {
                        return Err(syn::Error::new_spanned(
                            other,
                            "a proxied method names each of its parameters",
                        ));
                    }
                },
            }
        }
        if rust.attrs.rust_only {
            items.push(quote::quote! {
                #sig {
                    #proxy::#fn_ident(self, #(#names),*)
                }
            });
            continue;
        }
        if let Some(custom) = &rust.attrs.custom {
            items.push(quote::quote! {
                #sig {
                    #custom(self, #(#names),*)
                }
            });
            continue;
        }
        let php = match &rust.attrs.name {
            Some(name) => decl
                .methods
                .iter()
                .find(|method| method.name == name.value()),
            None => decl
                .methods
                .iter()
                .find(|method| *fn_ident == method.rust.trim_start_matches("r#")),
        };
        let Some(php) = php else {
            if rust.has_default {
                continue;
            }
            return Err(syn::Error::new(
                span,
                format!(
                    "{fqcn} declares no method `{fn_ident}`; mark a method only the Rust side has #[php(rust_only)]"
                ),
            ));
        };
        let member = format!("{fqcn}::{}", php.name);
        if names.len() != php.params.len() {
            return Err(syn::Error::new(
                span,
                format!(
                    "{member} takes {} parameters in PHP, {} here",
                    php.params.len(),
                    names.len()
                ),
            ));
        }
        let syn::ReturnType::Type(_, output) = &sig.output else {
            return Err(syn::Error::new(
                span,
                format!(
                    "{member}: a proxied method answers with an `anyhow::Result`, since the call can fail"
                ),
            ));
        };
        let answer = result_ok_type(output).ok_or_else(|| {
            syn::Error::new_spanned(
                output,
                format!("{member}: a proxied method answers with an `anyhow::Result`, since the call can fail"),
            )
        })?;
        // A promise is often only documented: `@return PromiseInterface|null`.
        let returns_promise = php
            .return_type
            .iter()
            .chain(&php.doc_return_type)
            .any(|ty| ty.contains("PromiseInterface"));
        if sig.asyncness.is_some() != returns_promise {
            return Err(syn::Error::new(
                span,
                format!(
                    "{member}: an `async fn` stands for a PHP method answering with a promise, and only that"
                ),
            ));
        }
        let ignores_answer = matches!(answer, syn::Type::Tuple(tuple) if tuple.elems.is_empty());
        if !returns_promise && !ignores_answer {
            let php_admits_null = php
                .return_type
                .as_deref()
                .is_none_or(|ty| ty.starts_with('?') || ty.contains("null") || ty == "mixed");
            if php_admits_null && !admits_null(answer) {
                return Err(syn::Error::new_spanned(
                    answer,
                    format!(
                        "{member} may answer with null or an untyped value in PHP, which this type does not hold; answer it with #[php(custom = ...)]"
                    ),
                ));
            }
        }
        let php_name = &php.name;
        let settle = returns_promise.then(|| {
            quote::quote! {
                let __value = crate::php_worker::rpc::binding::promise_value(__call, __value).await?;
            }
        });
        // A method the Rust side answers with `()` leaves PHP's answer alone, as a caller of a
        // `void` method does.
        let decode = if ignores_answer {
            quote::quote! {
                let _ = __value;
                ::core::result::Result::Ok(())
            }
        } else {
            quote::quote! { crate::php_worker::rpc::binding::proxy_answer(__call, &__value) }
        };
        items.push(quote::quote! {
            #sig {
                let __call = crate::php_worker::call::PhpCall::Method(
                    crate::php_worker::proxy::PhpProxy::php_handle(self),
                    ::bstr::BStr::new(#php_name),
                );
                let __args = vec![#(crate::php_worker::rpc::binding::proxy_arg(#names)?),*];
                let __value: ::shirabe_php_rpc::RpcValue = __call.run(__args)?;
                #settle
                #decode
            }
        });
    }

    Ok(quote::quote! {
        #(#async_attrs)*
        impl #trait_ident for #proxy {
            #(#items)*
        }
    })
}

/// `T` of a `Result<T>` return type.
fn result_ok_type(ty: &syn::Type) -> Option<&syn::Type> {
    let syn::Type::Path(path) = ty else {
        return None;
    };
    let last = path.path.segments.last()?;
    if last.ident != "Result" {
        return None;
    }
    let syn::PathArguments::AngleBracketed(args) = &last.arguments else {
        return None;
    };
    match args.args.first()? {
        syn::GenericArgument::Type(ty) => Some(ty),
        _ => None,
    }
}

/// The Rust method answering a PHP member: the one naming it with `#[php(name = ...)]`, or else
/// the one with its Rust name that names no member itself.
fn find<'a>(
    methods: &'a mut [RustMethod],
    php_name: &str,
    rust_name: &str,
) -> Option<&'a mut RustMethod> {
    let index = methods
        .iter()
        .position(|method| {
            method
                .attrs
                .name
                .as_ref()
                .is_some_and(|name| name.value() == php_name)
        })
        .or_else(|| {
            methods.iter().position(|method| {
                method.attrs.name.is_none()
                    && method.sig.ident == rust_name.trim_start_matches("r#")
            })
        })?;
    let method = &mut methods[index];
    method.matched = true;

    Some(method)
}

/// How an adapter decodes one argument and hands it to the Rust method.
enum ArgShape {
    /// The parameter's own type.
    Owned(syn::Type),
    /// `&T`, decoded as `T::Owned` and lent.
    Borrowed(syn::Type),
    /// `Option<&T>`, decoded as `Option<T::Owned>` and lent.
    OptionBorrowed(syn::Type),
}

#[allow(clippy::too_many_arguments, reason = "the parts of one adapter")]
fn adapter(
    fqcn: &str,
    php_name: &str,
    is_static: bool,
    params: &[Param],
    rust: &RustMethod,
    adapter_name: &syn::Ident,
    target: &proc_macro2::TokenStream,
    call_base: &proc_macro2::TokenStream,
) -> syn::Result<proc_macro2::TokenStream> {
    let sig = &rust.sig;
    let span = sig.span();
    let member = format!("{fqcn}::{php_name}");
    let is_async = sig.asyncness.is_some();
    let mut receiver = None;
    let mut typed = Vec::new();
    for input in &sig.inputs {
        match input {
            syn::FnArg::Receiver(r) => receiver = Some(r),
            syn::FnArg::Typed(pat_type) => typed.push(pat_type),
        }
    }
    let receiver_mut = match (is_static, receiver) {
        (true, None) => None,
        (true, Some(r)) => {
            return Err(syn::Error::new(
                r.span(),
                format!("{member} is static in PHP, so it takes no receiver"),
            ));
        }
        (false, None) => {
            return Err(syn::Error::new(
                span,
                format!(
                    "{member} is an instance method in PHP, so it takes `&self` or `&mut self`"
                ),
            ));
        }
        (false, Some(r)) => match &r.kind {
            syn::ReceiverKind::Reference(_, _, mutability) => Some(mutability.is_some()),
            _ => {
                return Err(syn::Error::new(
                    r.span(),
                    format!(
                        "{member}: an adapter borrows its receiver, so it takes `&self` or `&mut self`"
                    ),
                ));
            }
        },
    };
    let (decodes, passes) = decode_args(&member, span, params, &typed, &rust.decodes)?;

    let fn_ident = &sig.ident;
    let lends = match &sig.output {
        syn::ReturnType::Default => false,
        syn::ReturnType::Type(_, ty) => has_reference(ty),
    };
    let answer = quote::quote! { crate::php_worker::rpc::binding::IntoRpcAnswer::into_rpc_answer };
    if is_async {
        if receiver_mut != Some(false) {
            return Err(syn::Error::new(
                span,
                format!(
                    "{member}: an async adapter holds its borrow of the receiver across `.await`, so it takes `&self`"
                ),
            ));
        }
        if lends {
            return Err(syn::Error::new(
                span,
                format!("{member}: an async adapter cannot answer with a borrowed value"),
            ));
        }
        return Ok(quote::quote! {
            pub(crate) fn #adapter_name(
                this: &(impl crate::php_worker::rpc::binding::Receiver<#target>
                      + crate::php_worker::rpc::binding::JobReceiver
                      + ::core::clone::Clone
                      + 'static),
                call: &mut crate::php_worker::rpc::entity::Call<'_>,
            ) -> ::core::result::Result<::shirabe_php_rpc::RpcValue, ::shirabe_php_rpc::PhpThrow> {
                #(#decodes)*
                let __job = crate::php_worker::rpc::binding::JobReceiver::job(this, call)?;
                let __failed = call.failed();
                let __this = ::core::clone::Clone::clone(this);
                ::core::result::Result::Ok(crate::php_worker::rpc::binding::register_job(
                    __job,
                    move |__started| ::std::boxed::Box::pin(async move {
                        __started.set(true);
                        let __answer = {
                            let __guard = crate::php_worker::rpc::binding::Receiver::get(&__this);
                            #call_base::#fn_ident(&*__guard, #(#passes),*).await
                        };
                        #answer(__answer, &__failed)
                    }),
                ))
            }
        });
    }
    let (this, invoke) = match receiver_mut {
        None => (
            quote::quote! {},
            quote::quote! { #call_base::#fn_ident(#(#passes),*) },
        ),
        Some(true) => (
            quote::quote! { this: &impl crate::php_worker::rpc::binding::ReceiverMut<#target>, },
            quote::quote! { #call_base::#fn_ident(&mut *crate::php_worker::rpc::binding::ReceiverMut::get_mut(this), #(#passes),*) },
        ),
        Some(false) => (
            quote::quote! { this: &impl crate::php_worker::rpc::binding::Receiver<#target>, },
            quote::quote! { #call_base::#fn_ident(&*crate::php_worker::rpc::binding::Receiver::get(this), #(#passes),*) },
        ),
    };
    let body = if lends {
        quote::quote! {
            let __failed = call.failed();
            #answer(#invoke, &__failed)
        }
    } else {
        quote::quote! {
            let __answer = #invoke;
            #answer(__answer, &call.failed())
        }
    };

    Ok(quote::quote! {
        pub(crate) fn #adapter_name(
            #this
            call: &mut crate::php_worker::rpc::entity::Call<'_>,
        ) -> ::core::result::Result<::shirabe_php_rpc::RpcValue, ::shirabe_php_rpc::PhpThrow> {
            #(#decodes)*
            #body
        }
    })
}

/// The statements decoding a forwarded call's arguments, and the expressions passing them on.
fn decode_args(
    member: &str,
    span: proc_macro2::Span,
    params: &[Param],
    typed: &[&syn::PatType],
    decode_types: &[Option<syn::Type>],
) -> syn::Result<(Vec<proc_macro2::TokenStream>, Vec<proc_macro2::TokenStream>)> {
    if typed.len() != params.len() {
        return Err(syn::Error::new(
            span,
            format!(
                "{member} takes {} parameters in PHP, {} here",
                params.len(),
                typed.len()
            ),
        ));
    }

    let mut decodes = Vec::new();
    let mut passes = Vec::new();
    for (position, ((pat_type, param), decode)) in
        typed.iter().zip(params).zip(decode_types).enumerate()
    {
        let ty_span = pat_type.ty.span();
        let what = format!("{member} parameter ${}", param.name);
        if param.variadic {
            return Err(syn::Error::new(
                ty_span,
                format!("{what} is variadic, which an adapter does not decode yet"),
            ));
        }
        let shape = match decode {
            Some(decode) => ArgShape::Owned(decode.clone()),
            None => shape_of(&pat_type.ty, &what)?,
        };
        if param.by_ref {
            return Err(syn::Error::new(
                ty_span,
                format!("{what} is passed by reference, which an adapter does not decode yet"),
            ));
        }
        let admits_null = match &shape {
            ArgShape::Owned(ty) => admits_null(ty),
            ArgShape::Borrowed(_) => false,
            ArgShape::OptionBorrowed(_) => true,
        };
        if param.nullable && !admits_null {
            return Err(syn::Error::new(
                ty_span,
                format!(
                    "null can arrive in {what}, which this type does not hold; give a #[php(decode = ...)] type or answer it with #[php(custom = ...)]"
                ),
            ));
        }
        let var = quote::format_ident!("__arg{}", position);
        let (decoded, pass) = match &shape {
            ArgShape::Owned(ty) => (quote::quote! { #ty }, quote::quote! { #var }),
            ArgShape::Borrowed(ty) => (
                quote::quote! { <#ty as ::std::borrow::ToOwned>::Owned },
                quote::quote! { ::std::borrow::Borrow::<#ty>::borrow(&#var) },
            ),
            ArgShape::OptionBorrowed(ty) => (
                quote::quote! { ::core::option::Option<<#ty as ::std::borrow::ToOwned>::Owned> },
                quote::quote! { #var.as_ref().map(::std::borrow::Borrow::<#ty>::borrow) },
            ),
        };
        decodes.push(quote::quote_spanned! {ty_span=>
            let #var: #decoded = crate::php_worker::rpc::binding::decode_arg(call, #position)?;
        });
        passes.push(pass);
    }

    Ok((decodes, passes))
}

/// The adapter of a constructor: decodes the arguments and builds the Rust value, which the
/// entity declaration turns into a handle.
fn constructor_adapter(
    fqcn: &str,
    params: &[Param],
    rust: &RustMethod,
    self_ty: &syn::Type,
) -> syn::Result<proc_macro2::TokenStream> {
    let sig = &rust.sig;
    let span = sig.span();
    let member = format!("{fqcn}::__construct");
    let mut typed = Vec::new();
    for input in &sig.inputs {
        match input {
            syn::FnArg::Receiver(r) => {
                return Err(syn::Error::new(
                    r.span(),
                    format!("{member} builds a value, so it takes no receiver"),
                ));
            }
            syn::FnArg::Typed(pat_type) => typed.push(pat_type),
        }
    }
    let fallible = match &sig.output {
        syn::ReturnType::Type(_, ty) => match &**ty {
            syn::Type::Path(path) => {
                let last = path.path.segments.last().expect("a path has a segment");
                last.ident == "Result"
            }
            _ => false,
        },
        syn::ReturnType::Default => {
            return Err(syn::Error::new(
                span,
                format!("{member} has to return the value it builds"),
            ));
        }
    };
    let (decodes, passes) = decode_args(&member, span, params, &typed, &rust.decodes)?;
    let fn_ident = &sig.ident;
    let built = if fallible {
        quote::quote! { ::core::result::Result::map_err(<#self_ty>::#fn_ident(#(#passes),*), call.failed()) }
    } else {
        quote::quote! { ::core::result::Result::Ok(<#self_ty>::#fn_ident(#(#passes),*)) }
    };

    Ok(quote::quote! {
        pub(crate) fn __construct(
            call: &mut crate::php_worker::rpc::entity::Call<'_>,
        ) -> ::core::result::Result<#self_ty, ::shirabe_php_rpc::PhpThrow> {
            #(#decodes)*
            #built
        }
    })
}

fn shape_of(ty: &syn::Type, what: &str) -> syn::Result<ArgShape> {
    match ty {
        syn::Type::Reference(reference) => {
            if reference.mutability.is_some() {
                return Err(syn::Error::new(
                    ty.span(),
                    format!("{what} is `&mut`, which an adapter does not decode yet"),
                ));
            }
            Ok(ArgShape::Borrowed((*reference.elem).clone()))
        }
        syn::Type::ImplTrait(_) => Err(syn::Error::new(
            ty.span(),
            format!("{what} is generic; give the type to decode with #[php(decode = ...)]"),
        )),
        syn::Type::Path(path) => {
            let last = path.path.segments.last().expect("a path has a segment");
            if last.ident == "Option"
                && let syn::PathArguments::AngleBracketed(args) = &last.arguments
                && let Some(syn::GenericArgument::Type(syn::Type::Reference(reference))) =
                    args.args.first()
            {
                if reference.mutability.is_some() {
                    return Err(syn::Error::new(
                        ty.span(),
                        format!("{what} is `&mut`, which an adapter does not decode yet"),
                    ));
                }
                return Ok(ArgShape::OptionBorrowed((*reference.elem).clone()));
            }
            Ok(ArgShape::Owned(ty.clone()))
        }
        _ => Ok(ArgShape::Owned(ty.clone())),
    }
}

/// Whether a decoded type holds PHP's null: an `Option`, or a type standing for any PHP value.
fn admits_null(ty: &syn::Type) -> bool {
    let syn::Type::Path(path) = ty else {
        return false;
    };
    let last = path.path.segments.last().expect("a path has a segment");

    ["Option", "PhpMixed", "RpcValue"]
        .iter()
        .any(|name| last.ident == name)
}

/// Whether a type borrows: it holds a `&` or a lifetime.
fn has_reference(ty: &syn::Type) -> bool {
    fn scan(tokens: proc_macro2::TokenStream) -> bool {
        tokens.into_iter().any(|token| match token {
            proc_macro2::TokenTree::Punct(punct) => {
                punct.as_char() == '&' || punct.as_char() == '\''
            }
            proc_macro2::TokenTree::Group(group) => scan(group.stream()),
            _ => false,
        })
    }

    scan(quote::quote! { #ty })
}

pub fn snake_case(name: &str) -> String {
    let mut out = String::new();
    let chars: Vec<char> = name.chars().collect();
    for (index, &c) in chars.iter().enumerate() {
        if c.is_ascii_uppercase() {
            let after_lower = index > 0
                && (chars[index - 1].is_ascii_lowercase() || chars[index - 1].is_ascii_digit());
            let before_lower = index > 0
                && chars[index - 1].is_ascii_uppercase()
                && chars.get(index + 1).is_some_and(char::is_ascii_lowercase);
            if after_lower || before_lower {
                out.push('_');
            }
            out.push(c.to_ascii_lowercase());
        } else {
            out.push(c);
        }
    }

    out
}
