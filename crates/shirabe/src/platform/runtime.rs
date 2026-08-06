//! ref: composer/src/Composer/Platform/Runtime.php

use indexmap::IndexMap;
use shirabe_external_packages::composer::pcre::{CaptureKey, Preg};
use shirabe_php_rpc::{PhpThrow, PluginValue};
use shirabe_php_shim::{
    PhpMixed, RuntimeException, function_exists, html_entity_decode, implode, ltrim, php_regex,
    strip_tags, trim,
};

/// Seam over the PHP runtime so PlatformRepository can be tested against mocked
/// extension/constant/function probes. PHP has no such interface (the test mocks the
/// concrete `Composer\Platform\Runtime` directly); it is introduced here to keep the
/// consumer dependent only on trait methods.
pub trait RuntimeInterface: std::fmt::Debug {
    fn has_constant(&self, constant_name: &str, class: Option<String>) -> bool;
    fn get_constant(&self, constant_name: &str, class: Option<String>) -> PhpMixed;
    /// `callable` carries the PHP callable spec (a function name string or a
    /// `[class, method]` list), matching PHP `invoke($callable, $arguments)`.
    fn invoke(&self, callable: PhpMixed, arguments: Vec<PhpMixed>) -> PhpMixed;
    fn has_class(&self, class: &str) -> bool;
    fn construct(&self, class: &str, arguments: Vec<PhpMixed>) -> anyhow::Result<PhpMixed>;
    fn get_extensions(&self) -> Vec<String>;
    fn get_extension_version(&self, extension: &str) -> String;
    fn get_extension_info(&self, extension: &str) -> anyhow::Result<String>;
}

#[derive(Debug)]
pub struct Runtime;

impl RuntimeInterface for Runtime {
    fn has_constant(&self, constant_name: &str, class: Option<String>) -> bool {
        shirabe_php_rpc::has_constant(&ltrim(
            &format!("{}::{}", class.as_deref().unwrap_or(""), constant_name),
            Some(":"),
        ))
    }

    fn get_constant(&self, constant_name: &str, class: Option<String>) -> PhpMixed {
        shirabe_php_rpc::get_constant(&ltrim(
            &format!("{}::{}", class.as_deref().unwrap_or(""), constant_name),
            Some(":"),
        ))
    }

    fn invoke(&self, callable: PhpMixed, arguments: Vec<PhpMixed>) -> PhpMixed {
        // PHP: return $callable(...$arguments);
        // Only the specific dynamic callables PlatformRepository actually reaches are
        // wired through php-rpc; arbitrary PHP callables are still unsupported.
        match (&callable, arguments.as_slice()) {
            (PhpMixed::String(name), [PhpMixed::String(arg)]) if name == "inet_pton" => {
                shirabe_php_rpc::inet_pton(arg)
            }
            (PhpMixed::String(name), []) if name == "curl_version" => {
                let mut version = IndexMap::new();
                if let Some(v) = shirabe_php_rpc::curl_version() {
                    version.insert("version".to_string(), PhpMixed::String(v));
                }
                PhpMixed::Array(version)
            }
            (PhpMixed::List(spec), _) => match class_callable(spec) {
                ("ResourceBundle", "create") => resource_bundle_create(arguments),
                ("IntlChar", "getUnicodeVersion") => {
                    php_value(shirabe_php_rpc::call_static_method(
                        "IntlChar",
                        "getUnicodeVersion",
                        Vec::new(),
                        None,
                    ))
                }
                (class, method) => panic!(
                    "the PHP callable `{class}::{method}` is not wired through the runtime seam"
                ),
            },
            _ => panic!("the PHP callable {callable:?} is not wired through the runtime seam"),
        }
    }

    fn has_class(&self, class: &str) -> bool {
        shirabe_php_rpc::class_exists(class)
    }

    fn construct(&self, class: &str, arguments: Vec<PhpMixed>) -> anyhow::Result<PhpMixed> {
        match class {
            "Imagick" => imagick_version(arguments),
            other => Err(anyhow::anyhow!(RuntimeException {
                message: format!("the PHP class `{other}` is not wired through the runtime seam"),
                code: 0,
            })),
        }
    }

    fn get_extensions(&self) -> Vec<String> {
        shirabe_php_rpc::get_loaded_extensions()
    }

    fn get_extension_version(&self, extension: &str) -> String {
        shirabe_php_rpc::phpversion(extension).unwrap_or_else(|| "0".to_string())
    }

    fn get_extension_info(&self, extension: &str) -> anyhow::Result<String> {
        Ok(shirabe_php_rpc::get_extension_info(extension))
    }
}

/// The `[class, method]` pair of a PHP callable given in array form.
fn class_callable(spec: &[PhpMixed]) -> (&str, &str) {
    match spec {
        [PhpMixed::String(class), PhpMixed::String(method)] => (class, method),
        other => panic!("a PHP callable given as an array must be [class, method], got {other:?}"),
    }
}

/// Unwraps an RPC outcome whose failure means the runtime probe itself is broken, not that the
/// probed extension is absent.
fn php_value(outcome: anyhow::Result<Result<PluginValue, PhpThrow>>) -> PhpMixed {
    match outcome {
        Ok(Ok(value)) => value
            .to_php_mixed()
            .expect("a runtime probe answers with plain values"),
        Ok(Err(throw)) => panic!("the PHP runtime probe failed: {}", throw.message),
        Err(e) => panic!("the PHP runtime probe could not be sent: {e:#}"),
    }
}

/// PHP `ResourceBundle::create(...)`, whose result the caller reads `->get('Version')` off.
/// A live PHP object has no `PhpMixed` counterpart, so that entry crosses in its place.
fn resource_bundle_create(arguments: Vec<PhpMixed>) -> PhpMixed {
    let bundle = match php_handle(shirabe_php_rpc::call_static_method(
        "ResourceBundle",
        "create",
        arguments.iter().map(PluginValue::from_php_mixed).collect(),
        None,
    )) {
        Some(phandle) => phandle,
        // PHP returns null when the bundle cannot be opened.
        None => return PhpMixed::Null,
    };
    let version = php_value(shirabe_php_rpc::call_php_method(
        bundle,
        "get",
        vec![PluginValue::string("Version")],
        None,
    ));
    let _ = shirabe_php_rpc::release_php_handle(bundle);
    PhpMixed::Object(IndexMap::from([("Version".to_string(), version)]))
}

/// PHP `(new Imagick())->getVersion()`, reported as the entries the caller reads.
fn imagick_version(arguments: Vec<PhpMixed>) -> anyhow::Result<PhpMixed> {
    let imagick = php_handle(shirabe_php_rpc::new_object(
        "Imagick",
        arguments.iter().map(PluginValue::from_php_mixed).collect(),
        None,
    ))
    .ok_or_else(|| {
        anyhow::anyhow!(RuntimeException {
            message: "`new Imagick` did not answer with an object".to_string(),
            code: 0,
        })
    })?;
    let version = php_value(shirabe_php_rpc::call_php_method(
        imagick,
        "getVersion",
        Vec::new(),
        None,
    ));
    let _ = shirabe_php_rpc::release_php_handle(imagick);
    Ok(version)
}

/// The handle of a PHP-side object an RPC answered with, or `None` when it answered with null.
fn php_handle(outcome: anyhow::Result<Result<PluginValue, PhpThrow>>) -> Option<u64> {
    match outcome {
        Ok(Ok(PluginValue::PhpHandle(handle))) => Some(handle.phandle),
        Ok(Ok(PluginValue::Null)) => None,
        Ok(Ok(other)) => panic!("the PHP runtime probe answered with {other:?}, not an object"),
        Ok(Err(throw)) => panic!("the PHP runtime probe failed: {}", throw.message),
        Err(e) => panic!("the PHP runtime probe could not be sent: {e:#}"),
    }
}

impl Runtime {
    pub fn has_function(&self, f: &str) -> bool {
        function_exists(f)
    }

    pub fn parse_html_extension_info(html: &str) -> String {
        let mut result: Vec<String> = vec![];

        let mut matches: IndexMap<CaptureKey, String> = IndexMap::new();
        if Preg::match3(
            php_regex!(r"~<h2>\s*<a[^>]*>([^<]+)</a>\s*</h2>~i"),
            html,
            Some(&mut matches),
        ) {
            result.push(trim(
                &html_entity_decode(
                    matches
                        .get(&CaptureKey::ByIndex(1))
                        .map(|s| s.as_str())
                        .unwrap_or(""),
                ),
                None,
            ));
            result.push(String::new());
        }

        let mut matches: IndexMap<CaptureKey, Vec<String>> = IndexMap::new();
        if Preg::match_all3(
            php_regex!(
                r#"~<tr>\s*<td class="e">\s*(.*?)\s*</td>\s*<td class="v">\s*(.*?)\s*</td>\s*</tr>~is"#
            ),
            html,
            Some(&mut matches),
        ) > 0
        {
            let group1 = matches
                .get(&CaptureKey::ByIndex(1))
                .cloned()
                .unwrap_or_default();
            let group2 = matches
                .get(&CaptureKey::ByIndex(2))
                .cloned()
                .unwrap_or_default();
            let count = std::cmp::min(group1.len(), group2.len());

            for i in 0..count {
                let key = trim(&html_entity_decode(&strip_tags(&group1[i])), None);
                let value = trim(&html_entity_decode(&strip_tags(&group2[i])), None);
                result.push(format!("{} => {}", key, value));
            }
        }

        implode("\n", &result)
    }
}
