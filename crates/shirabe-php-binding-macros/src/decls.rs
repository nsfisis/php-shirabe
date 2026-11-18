//! The PHP declarations `scripts/plugin-stub-generator/generate-stubs` writes under the calling
//! crate's `php-decls/`, one JSON file per Composer class or interface.

#[derive(Debug, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Decl {
    pub fqcn: String,
    pub rust_module: Vec<String>,
    #[serde(default)]
    pub parent: Option<String>,
    pub methods: Vec<Method>,
    #[serde(default)]
    pub constructor: Option<Constructor>,
}

#[derive(Debug, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Method {
    pub name: String,
    pub rust: String,
    pub declared_in: String,
    #[serde(rename = "static")]
    pub is_static: bool,
    pub params: Vec<Param>,
    pub func_num_args: bool,
    #[serde(default)]
    pub forwarded: bool,
    #[serde(default)]
    pub status: Option<String>,
}

#[derive(Debug, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Constructor {
    pub declared_in: Option<String>,
    pub params: Vec<Param>,
    pub func_num_args: bool,
    #[serde(default)]
    pub forwarded: bool,
    #[serde(default)]
    pub status: Option<String>,
}

#[derive(Debug, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Param {
    pub name: String,
    pub by_ref: bool,
    pub variadic: bool,
    pub nullable: bool,
    #[serde(rename = "type")]
    pub native_type: Option<String>,
}

impl Param {
    /// The parameter's PHP type as a binding compares it: the native type (`mixed` when there is
    /// none), and whether null can arrive.
    pub fn signature(&self) -> String {
        format!(
            "{}{}",
            self.native_type.as_deref().unwrap_or("mixed"),
            if self.nullable { "|null" } else { "" }
        )
    }
}

/// The signatures of a parameter list, as a `&[&[u8]]` expression.
pub fn signatures(params: &[Param]) -> proc_macro2::TokenStream {
    let signatures = params.iter().map(|param| {
        syn::LitByteStr::new(param.signature().as_bytes(), proc_macro2::Span::call_site())
    });
    quote::quote! { &[#(#signatures),*] }
}

/// A loaded declaration, with the item that makes the calling crate rebuild when it changes.
#[derive(Debug)]
pub struct Loaded {
    pub decl: Decl,
    pub tracking: proc_macro2::TokenStream,
}

/// Reads the declaration of `fqcn` from the calling crate's `php-decls/`.
pub fn load(fqcn: &str, span: proc_macro2::Span) -> syn::Result<Loaded> {
    let relative = format!("/php-decls/{}.json", fqcn.replace('\\', "/"));
    let manifest_dir = std::env::var("CARGO_MANIFEST_DIR")
        .map_err(|_| syn::Error::new(span, "CARGO_MANIFEST_DIR is not set"))?;
    let path = format!("{manifest_dir}{relative}");
    let bytes = std::fs::read(&path).map_err(|error| {
        syn::Error::new(
            span,
            format!("no PHP declaration for {fqcn} ({path}: {error}); run scripts/plugin-stub-generator/generate-stubs"),
        )
    })?;
    let decl: Decl = serde_json::from_slice(&bytes)
        .map_err(|error| syn::Error::new(span, format!("{path}: {error}")))?;
    if decl.fqcn != fqcn {
        return Err(syn::Error::new(
            span,
            format!("{path} declares {}, not {fqcn}", decl.fqcn),
        ));
    }
    let tracking = quote::quote! {
        const _: &[u8] = include_bytes!(concat!(env!("CARGO_MANIFEST_DIR"), #relative));
    };

    Ok(Loaded { decl, tracking })
}

/// A Rust name the declaration spells, raw (`r#type`) when it is a keyword.
pub fn ident(name: &str, span: proc_macro2::Span) -> syn::Result<syn::Ident> {
    let mut ident: syn::Ident = syn::parse_str(name)
        .map_err(|_| syn::Error::new(span, format!("`{name}` is not a Rust identifier")))?;
    ident.set_span(span);

    Ok(ident)
}
