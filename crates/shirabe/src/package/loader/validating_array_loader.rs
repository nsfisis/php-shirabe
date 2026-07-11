//! ref: composer/src/Composer/Package/Loader/ValidatingArrayLoader.php

use crate::package::PackageInterfaceHandle;
use crate::package::loader::InvalidPackageException;
use crate::package::loader::LoaderInterface;
use crate::package::version::VersionParser;
use crate::package::{STABILITIES, SUPPORTED_LINK_TYPES};
use crate::repository::PlatformRepository;
use indexmap::IndexMap;
use shirabe_external_packages::composer::pcre::Preg;
use shirabe_php_shim::{
    E_USER_DEPRECATED, PHP_EOL, PhpMixed, array_intersect_key, array_values, filter_var_email,
    get_debug_type, is_array, is_bool, is_int, is_numeric, is_scalar, is_string, json_encode,
    parse_url_all, php_to_string, str_replace, strcasecmp, strtolower, strtotime, substr,
    trigger_error, trim, var_export,
};
use shirabe_semver::Intervals;
use shirabe_semver::constraint::AnyConstraint;
use shirabe_semver::constraint::SimpleConstraint;
use shirabe_spdx_licenses::SpdxLicenses;

#[derive(Debug)]
pub struct ValidatingArrayLoader {
    loader: Box<dyn LoaderInterface>,
    version_parser: VersionParser,
    // RefCell: `load` implements `LoaderInterface`, whose signature takes `&self`, but PHP's
    // implementation freely mutates these as scratch state for the duration of a single call.
    errors: std::cell::RefCell<Vec<String>>,
    warnings: std::cell::RefCell<Vec<String>>,
    config: std::cell::RefCell<IndexMap<String, PhpMixed>>,
    flags: i64,
}

impl ValidatingArrayLoader {
    pub const CHECK_ALL: i64 = 3;
    pub const CHECK_UNBOUND_CONSTRAINTS: i64 = 1;
    pub const CHECK_STRICT_CONSTRAINTS: i64 = 2;

    pub fn new(
        loader: Box<dyn LoaderInterface>,
        strict_name: bool,
        parser: Option<VersionParser>,
        flags: i64,
    ) -> Self {
        let version_parser = parser.unwrap_or_default();

        if !strict_name {
            trigger_error(
                "$strictName must be set to true in ValidatingArrayLoader's constructor as of 2.2, and it will be removed in 3.0",
                E_USER_DEPRECATED,
            );
        }

        Self {
            loader,
            version_parser,
            errors: std::cell::RefCell::new(Vec::new()),
            warnings: std::cell::RefCell::new(Vec::new()),
            config: std::cell::RefCell::new(IndexMap::new()),
            flags,
        }
    }
}

impl LoaderInterface for ValidatingArrayLoader {
    fn as_any(&self) -> &dyn std::any::Any {
        self
    }

    fn load(
        &self,
        config: IndexMap<String, PhpMixed>,
        class: Option<String>,
    ) -> anyhow::Result<PackageInterfaceHandle> {
        let class = class.unwrap_or_else(|| "Composer\\Package\\CompletePackage".to_string());

        *self.errors.borrow_mut() = Vec::new();
        *self.warnings.borrow_mut() = Vec::new();
        *self.config.borrow_mut() = config.clone();

        self.validate_string("name", true);
        if let Some(name_val) = config.get("name").and_then(|v| v.as_string())
            && let Some(err) = Self::has_package_naming_error(name_val, false)
        {
            self.errors.borrow_mut().push(format!("name : {}", err));
        }

        if self.config.borrow().contains_key("version") {
            let version_val = self.config.borrow()["version"].clone();
            if !is_scalar(&version_val) {
                self.validate_string("version", false);
            } else {
                if !is_string(&version_val) {
                    self.config.borrow_mut().insert(
                        "version".to_string(),
                        PhpMixed::String(php_to_string(&version_val)),
                    );
                }
                let version_str = self
                    .config
                    .borrow()
                    .get("version")
                    .and_then(|v| v.as_string())
                    .unwrap_or("")
                    .to_string();
                match self.version_parser.normalize(&version_str, None) {
                    Ok(_) => {}
                    Err(e) => {
                        self.errors
                            .borrow_mut()
                            .push(format!("version : invalid value ({}): {}", version_str, e));
                        self.config.borrow_mut().shift_remove("version");
                    }
                }
            }
        }

        if let Some(config_section) = self
            .config
            .borrow()
            .get("config")
            .and_then(|v| v.as_array())
            .cloned()
            && let Some(platform_val) = config_section.get("platform")
        {
            let platform_array: IndexMap<String, PhpMixed> = match platform_val {
                PhpMixed::Array(m) => m.clone(),
                other => {
                    let mut m = IndexMap::new();
                    m.insert("0".to_string(), other.clone());
                    m
                }
            };
            for (key, platform) in &platform_array {
                if let PhpMixed::Bool(false) = platform {
                    continue;
                }
                if !is_string(platform) {
                    self.errors.borrow_mut().push(format!(
                        "config.platform.{} : invalid value ({} {}): expected string or false",
                        key,
                        get_debug_type(platform),
                        var_export(platform, true)
                    ));
                    continue;
                }
                let platform_str = platform.as_string().unwrap_or("").to_string();
                if let Err(e) = self.version_parser.normalize(&platform_str, None) {
                    self.errors.borrow_mut().push(format!(
                        "config.platform.{} : invalid value ({}): {}",
                        key, platform_str, e
                    ));
                }
            }
        }

        self.validate_regex("type", "[A-Za-z0-9-]+", false);
        self.validate_string("target-dir", false);
        self.validate_array("extra", false);

        if self.config.borrow().contains_key("bin") {
            if is_string(&self.config.borrow()["bin"]) {
                self.validate_string("bin", false);
            } else {
                self.validate_flat_array("bin", None, false);
            }
        }

        self.validate_array("scripts", false); // TODO validate event names & listener syntax
        self.validate_string("description", false);
        self.validate_url("homepage", false);
        self.validate_flat_array("keywords", Some("[\\p{N}\\p{L} ._-]+"), false);

        let mut release_date: Option<chrono::DateTime<chrono::Utc>> = None;
        self.validate_string("time", false);
        if self.config.borrow().contains_key("time") {
            let time_str = self.config.borrow()["time"]
                .as_string()
                .unwrap_or("")
                .to_string();
            match shirabe_php_shim::date_create::<chrono::Utc>(&time_str) {
                Ok(dt) => {
                    release_date = Some(dt);
                }
                Err(e) => {
                    self.errors
                        .borrow_mut()
                        .push(format!("time : invalid value ({}): {}", time_str, e));
                    self.config.borrow_mut().shift_remove("time");
                }
            }
        }

        if self.config.borrow().contains_key("license") {
            let license_val = self.config.borrow()["license"].clone();
            // validate main data types
            if is_array(&license_val) || is_string(&license_val) {
                let mut licenses: IndexMap<String, PhpMixed> = match &license_val {
                    PhpMixed::Array(m) => m.clone(),
                    other => {
                        let mut m = IndexMap::new();
                        m.insert("0".to_string(), other.clone());
                        m
                    }
                };

                let license_keys: Vec<String> = licenses.keys().cloned().collect();
                for index in &license_keys {
                    let license = licenses[index].clone();
                    if !is_string(&license) {
                        self.warnings.borrow_mut().push(format!(
                            "License {} should be a string.",
                            json_encode(&license).unwrap_or_default(),
                        ));
                        licenses.shift_remove(index);
                    }
                }

                // check for license validity on newly updated branches/tags
                let cutoff = strtotime("-8days").unwrap_or(0);
                if release_date.is_none() || release_date.unwrap().timestamp() >= cutoff {
                    let license_validator = SpdxLicenses::new();
                    for license in licenses.values() {
                        let license_str = license.as_string().unwrap_or("").to_string();
                        // replace proprietary by MIT for validation purposes since it's not a valid SPDX identifier, but is accepted by composer
                        if license_str == "proprietary" {
                            continue;
                        }
                        let license_to_validate = str_replace("proprietary", "MIT", &license_str);
                        if !license_validator.validate(&license_to_validate) {
                            if license_validator
                                .validate(&trim(&license_to_validate, Some(" \t\n\r\0\u{0B}")))
                            {
                                self.warnings.borrow_mut().push(format!(
                                    "License {} must not contain extra spaces, make sure to trim it.",
                                        json_encode(&PhpMixed::String(license_str.clone()))
                                            .unwrap_or_default(),
                                ));
                            } else {
                                self.warnings.borrow_mut().push(format!(
                                    "License {} is not a valid SPDX license identifier, see https://spdx.org/licenses/ if you use an open license.{}If the software is closed-source, you may use \"proprietary\" as license.",

                                        json_encode(&PhpMixed::String(license_str.clone()))
                                            .unwrap_or_default(),
                                    PHP_EOL
                                ));
                            }
                        }
                    }
                }

                let reindexed: Vec<PhpMixed> = array_values(&licenses);
                let mut reindexed_map: IndexMap<String, PhpMixed> = IndexMap::new();
                for (i, v) in reindexed.into_iter().enumerate() {
                    reindexed_map.insert(i.to_string(), v);
                }
                self.config
                    .borrow_mut()
                    .insert("license".to_string(), PhpMixed::Array(reindexed_map));
            } else {
                self.warnings.borrow_mut().push(format!(
                    "License must be a string or array of strings, got {}.",
                    json_encode(&license_val).unwrap_or_default(),
                ));
                self.config.borrow_mut().shift_remove("license");
            }
        }

        if self.validate_array("authors", false) {
            let author_keys: Vec<String> = self.config.borrow()["authors"]
                .as_array()
                .map(|a| a.keys().cloned().collect())
                .unwrap_or_default();
            for key in &author_keys {
                let author = self.config.borrow()["authors"].as_array().unwrap()[key].clone();
                if !is_array(&author) {
                    self.errors.borrow_mut().push(format!(
                        "authors.{} : should be an array, {} given",
                        key,
                        get_debug_type(&author)
                    ));
                    if let Some(PhpMixed::Array(m)) = self.config.borrow_mut().get_mut("authors") {
                        m.shift_remove(key);
                    }
                    continue;
                }
                for author_data in ["homepage", "email", "name", "role"] {
                    let val_opt = author.as_array().and_then(|m| m.get(author_data)).cloned();
                    if let Some(val) = val_opt
                        && !is_string(&val)
                    {
                        self.errors.borrow_mut().push(format!(
                            "authors.{}.{} : invalid value, must be a string",
                            key, author_data
                        ));
                        if let Some(PhpMixed::Array(authors)) =
                            self.config.borrow_mut().get_mut("authors")
                            && let Some(author_entry) = authors.get_mut(key)
                            && let PhpMixed::Array(am) = author_entry
                        {
                            am.shift_remove(author_data);
                        }
                    }
                }
                let homepage = author
                    .as_array()
                    .and_then(|m| m.get("homepage"))
                    .and_then(|v| v.as_string())
                    .map(|s| s.to_string());
                if let Some(homepage_str) = homepage
                    && !self.filter_url(&homepage_str, &["http", "https"])
                {
                    self.warnings.borrow_mut().push(format!(
                        "authors.{}.homepage : invalid value ({}), must be an http/https URL",
                        key, homepage_str
                    ));
                    if let Some(PhpMixed::Array(authors)) =
                        self.config.borrow_mut().get_mut("authors")
                        && let Some(author_entry) = authors.get_mut(key)
                        && let PhpMixed::Array(am) = author_entry
                    {
                        am.shift_remove("homepage");
                    }
                }
                let email = author
                    .as_array()
                    .and_then(|m| m.get("email"))
                    .and_then(|v| v.as_string())
                    .map(|s| s.to_string());
                if let Some(email_str) = email
                    && !filter_var_email(&email_str)
                {
                    self.warnings.borrow_mut().push(format!(
                        "authors.{}.email : invalid value ({}), must be a valid email address",
                        key, email_str
                    ));
                    if let Some(PhpMixed::Array(authors)) =
                        self.config.borrow_mut().get_mut("authors")
                        && let Some(author_entry) = authors.get_mut(key)
                        && let PhpMixed::Array(am) = author_entry
                    {
                        am.shift_remove("email");
                    }
                }
                let current_author_len = self
                    .config
                    .borrow()
                    .get("authors")
                    .and_then(|v| v.as_array())
                    .and_then(|m| m.get(key))
                    .and_then(|v| v.as_array())
                    .map(|m| m.len())
                    .unwrap_or(0);
                if current_author_len == 0
                    && let Some(PhpMixed::Array(authors)) =
                        self.config.borrow_mut().get_mut("authors")
                {
                    authors.shift_remove(key);
                }
            }
            let authors_len = self
                .config
                .borrow()
                .get("authors")
                .and_then(|v| v.as_array())
                .map(|m| m.len())
                .unwrap_or(0);
            if authors_len == 0 {
                self.config.borrow_mut().shift_remove("authors");
            }
        }

        if self.validate_array("support", false)
            && !Self::is_empty_array(self.config.borrow().get("support"))
        {
            for key in [
                "issues", "forum", "wiki", "source", "email", "irc", "docs", "rss", "chat",
                "security",
            ] {
                let val_opt = self
                    .config
                    .borrow()
                    .get("support")
                    .and_then(|v| v.as_array())
                    .and_then(|m| m.get(key))
                    .cloned();
                if let Some(val) = val_opt
                    && !is_string(&val)
                {
                    self.errors
                        .borrow_mut()
                        .push(format!("support.{} : invalid value, must be a string", key));
                    if let Some(PhpMixed::Array(support)) =
                        self.config.borrow_mut().get_mut("support")
                    {
                        support.shift_remove(key);
                    }
                }
            }

            let support_email = self
                .config
                .borrow()
                .get("support")
                .and_then(|v| v.as_array())
                .and_then(|m| m.get("email"))
                .and_then(|v| v.as_string())
                .map(|s| s.to_string());
            if let Some(email_str) = support_email
                && !filter_var_email(&email_str)
            {
                self.warnings.borrow_mut().push(format!(
                    "support.email : invalid value ({}), must be a valid email address",
                    email_str
                ));
                if let Some(PhpMixed::Array(support)) = self.config.borrow_mut().get_mut("support")
                {
                    support.shift_remove("email");
                }
            }

            let support_irc = self
                .config
                .borrow()
                .get("support")
                .and_then(|v| v.as_array())
                .and_then(|m| m.get("irc"))
                .and_then(|v| v.as_string())
                .map(|s| s.to_string());
            if let Some(irc_str) = support_irc
                && !self.filter_url(&irc_str, &["irc", "ircs"])
            {
                self.warnings.borrow_mut().push(format!(
                        "support.irc : invalid value ({}), must be a irc://<server>/<channel> or ircs:// URL",
                        irc_str
                    ));
                if let Some(PhpMixed::Array(support)) = self.config.borrow_mut().get_mut("support")
                {
                    support.shift_remove("irc");
                }
            }

            for key in [
                "issues", "forum", "wiki", "source", "docs", "chat", "security",
            ] {
                let url_opt = self
                    .config
                    .borrow()
                    .get("support")
                    .and_then(|v| v.as_array())
                    .and_then(|m| m.get(key))
                    .and_then(|v| v.as_string())
                    .map(|s| s.to_string());
                if let Some(url_str) = url_opt
                    && !self.filter_url(&url_str, &["http", "https"])
                {
                    self.warnings.borrow_mut().push(format!(
                        "support.{} : invalid value ({}), must be an http/https URL",
                        key, url_str
                    ));
                    if let Some(PhpMixed::Array(support)) =
                        self.config.borrow_mut().get_mut("support")
                    {
                        support.shift_remove(key);
                    }
                }
            }
            if Self::is_empty_array(self.config.borrow().get("support")) {
                self.config.borrow_mut().shift_remove("support");
            }
        }

        if self.validate_array("funding", false)
            && !Self::is_empty_array(self.config.borrow().get("funding"))
        {
            let funding_keys: Vec<String> = self
                .config
                .borrow()
                .get("funding")
                .and_then(|v| v.as_array())
                .map(|m| m.keys().cloned().collect())
                .unwrap_or_default();
            for key in &funding_keys {
                let funding_option =
                    self.config.borrow()["funding"].as_array().unwrap()[key].clone();
                if !is_array(&funding_option) {
                    self.errors.borrow_mut().push(format!(
                        "funding.{} : should be an array, {} given",
                        key,
                        get_debug_type(&funding_option)
                    ));
                    if let Some(PhpMixed::Array(funding)) =
                        self.config.borrow_mut().get_mut("funding")
                    {
                        funding.shift_remove(key);
                    }
                    continue;
                }
                for funding_data in ["type", "url"] {
                    let val_opt = funding_option
                        .as_array()
                        .and_then(|m| m.get(funding_data))
                        .cloned();
                    if let Some(val) = val_opt
                        && !is_string(&val)
                    {
                        self.errors.borrow_mut().push(format!(
                            "funding.{}.{} : invalid value, must be a string",
                            key, funding_data
                        ));
                        if let Some(PhpMixed::Array(funding)) =
                            self.config.borrow_mut().get_mut("funding")
                            && let Some(entry) = funding.get_mut(key)
                            && let PhpMixed::Array(em) = entry
                        {
                            em.shift_remove(funding_data);
                        }
                    }
                }
                let url = funding_option
                    .as_array()
                    .and_then(|m| m.get("url"))
                    .and_then(|v| v.as_string())
                    .map(|s| s.to_string());
                if let Some(url_str) = url
                    && !self.filter_url(&url_str, &["http", "https"])
                {
                    self.warnings.borrow_mut().push(format!(
                        "funding.{}.url : invalid value ({}), must be an http/https URL",
                        key, url_str
                    ));
                    if let Some(PhpMixed::Array(funding)) =
                        self.config.borrow_mut().get_mut("funding")
                        && let Some(entry) = funding.get_mut(key)
                        && let PhpMixed::Array(em) = entry
                    {
                        em.shift_remove("url");
                    }
                }
                let entry_empty = self
                    .config
                    .borrow()
                    .get("funding")
                    .and_then(|v| v.as_array())
                    .and_then(|m| m.get(key))
                    .and_then(|v| v.as_array())
                    .map(|m| m.is_empty())
                    .unwrap_or(true);
                if entry_empty
                    && let Some(PhpMixed::Array(funding)) =
                        self.config.borrow_mut().get_mut("funding")
                {
                    funding.shift_remove(key);
                }
            }
            if Self::is_empty_array(self.config.borrow().get("funding")) {
                self.config.borrow_mut().shift_remove("funding");
            }
        }

        if self.config.borrow().contains_key("php-ext") && self.validate_array("php-ext", false) {
            let pkg_type = self
                .config
                .borrow()
                .get("type")
                .and_then(|v| v.as_string())
                .unwrap_or("")
                .to_string();
            if !["php-ext", "php-ext-zend"].contains(&pkg_type.as_str()) {
                self.errors.borrow_mut().push(
                    "php-ext can only be set by packages of type \"php-ext\" or \"php-ext-zend\" which must be C extensions".to_string()
                );
                self.config.borrow_mut().shift_remove("php-ext");
            }

            if self.config.borrow().contains_key("php-ext") {
                let mut php_ext: IndexMap<String, PhpMixed> =
                    match self.config.borrow_mut().shift_remove("php-ext").unwrap() {
                        PhpMixed::Array(m) => m,
                        _ => IndexMap::new(),
                    };

                if let Some(v) = php_ext.get("extension-name").cloned()
                    && !is_string(&v)
                {
                    self.errors.borrow_mut().push(format!(
                        "php-ext.extension-name : should be a string, {} given",
                        get_debug_type(&v)
                    ));
                    php_ext.shift_remove("extension-name");
                }

                if let Some(v) = php_ext.get("priority").cloned()
                    && !is_int(&v)
                {
                    self.errors.borrow_mut().push(format!(
                        "php-ext.priority : should be an integer, {} given",
                        get_debug_type(&v)
                    ));
                    php_ext.shift_remove("priority");
                }

                if let Some(v) = php_ext.get("support-zts").cloned()
                    && !is_bool(&v)
                {
                    self.errors.borrow_mut().push(format!(
                        "php-ext.support-zts : should be a boolean, {} given",
                        get_debug_type(&v)
                    ));
                    php_ext.shift_remove("support-zts");
                }

                if let Some(v) = php_ext.get("support-nts").cloned()
                    && !is_bool(&v)
                {
                    self.errors.borrow_mut().push(format!(
                        "php-ext.support-nts : should be a boolean, {} given",
                        get_debug_type(&v)
                    ));
                    php_ext.shift_remove("support-nts");
                }

                if let Some(v) = php_ext.get("build-path").cloned()
                    && !is_string(&v)
                    && !matches!(v, PhpMixed::Null)
                {
                    self.errors.borrow_mut().push(format!(
                        "php-ext.build-path : should be a string or null, {} given",
                        get_debug_type(&v)
                    ));
                    php_ext.shift_remove("build-path");
                }

                if php_ext.contains_key("download-url-method") {
                    let v = php_ext["download-url-method"].clone();
                    if !is_array(&v) && !is_string(&v) {
                        self.errors.borrow_mut().push(format!(
                            "php-ext.download-url-method : should be an array or a string, {} given",
                            get_debug_type(&v)
                        ));
                        php_ext.shift_remove("download-url-method");
                    } else {
                        let valid_download_url_methods = [
                            "composer-default",
                            "pre-packaged-source",
                            "pre-packaged-binary",
                        ];
                        let defined_download_url_methods: IndexMap<String, PhpMixed> =
                            if is_array(&v) {
                                v.as_array().unwrap().clone()
                            } else {
                                let mut m = IndexMap::new();
                                m.insert("0".to_string(), v.clone());
                                m
                            };

                        if defined_download_url_methods.is_empty() {
                            self.errors.borrow_mut().push(
                                "php-ext.download-url-method : must contain at least one element"
                                    .to_string(),
                            );
                            php_ext.shift_remove("download-url-method");
                        } else {
                            for (key, download_url_method) in &defined_download_url_methods {
                                if !is_string(download_url_method) {
                                    self.errors.borrow_mut().push(format!(
                                        "php-ext.download-url-method.{} : should be a string, {} given",
                                        key,
                                        get_debug_type(download_url_method)
                                    ));
                                    php_ext.shift_remove("download-url-method");
                                } else if !valid_download_url_methods
                                    .contains(&download_url_method.as_string().unwrap_or(""))
                                {
                                    self.errors.borrow_mut().push(format!(
                                        "php-ext.download-url-method.{} : invalid value ({}), must be one of {}",
                                        key,
                                        download_url_method.as_string().unwrap_or(""),
                                        valid_download_url_methods.join(", ")
                                    ));
                                    php_ext.shift_remove("download-url-method");
                                }
                            }
                        }
                    }
                }

                if php_ext.contains_key("os-families")
                    && php_ext.contains_key("os-families-exclude")
                {
                    self.errors.borrow_mut().push(
                        "php-ext : os-families and os-families-exclude cannot both be specified"
                            .to_string(),
                    );
                    php_ext.shift_remove("os-families");
                    php_ext.shift_remove("os-families-exclude");
                } else {
                    let valid_os_families =
                        ["windows", "bsd", "darwin", "solaris", "linux", "unknown"];

                    for field_name in ["os-families", "os-families-exclude"] {
                        if let Some(field_val) = php_ext.get(field_name).cloned() {
                            if !is_array(&field_val) {
                                self.errors.borrow_mut().push(format!(
                                    "php-ext.{} : should be an array, {} given",
                                    field_name,
                                    get_debug_type(&field_val)
                                ));
                                php_ext.shift_remove(field_name);
                            } else if field_val.as_array().unwrap().is_empty() {
                                self.errors.borrow_mut().push(format!(
                                    "php-ext.{} : must contain at least one element",
                                    field_name
                                ));
                                php_ext.shift_remove(field_name);
                            } else {
                                let field_keys: Vec<String> =
                                    field_val.as_array().unwrap().keys().cloned().collect();
                                for key in &field_keys {
                                    let os_family = field_val.as_array().unwrap()[key].clone();
                                    if !is_string(&os_family) {
                                        self.errors.borrow_mut().push(format!(
                                            "php-ext.{}.{} : should be a string, {} given",
                                            field_name,
                                            key,
                                            get_debug_type(&os_family)
                                        ));
                                        if let Some(PhpMixed::Array(arr)) =
                                            php_ext.get_mut(field_name)
                                        {
                                            arr.shift_remove(key);
                                        }
                                    } else if !valid_os_families
                                        .contains(&os_family.as_string().unwrap_or(""))
                                    {
                                        self.errors.borrow_mut().push(format!(
                                            "php-ext.{}.{} : invalid value ({}), must be one of {}",
                                            field_name,
                                            key,
                                            os_family.as_string().unwrap_or(""),
                                            valid_os_families.join(", ")
                                        ));
                                        if let Some(PhpMixed::Array(arr)) =
                                            php_ext.get_mut(field_name)
                                        {
                                            arr.shift_remove(key);
                                        }
                                    }
                                }
                                let field_empty = php_ext
                                    .get(field_name)
                                    .and_then(|v| v.as_array())
                                    .map(|m| m.is_empty())
                                    .unwrap_or(true);
                                if field_empty {
                                    php_ext.shift_remove(field_name);
                                }
                            }
                        }
                    }
                }

                if php_ext.contains_key("configure-options") {
                    let configure_options = php_ext["configure-options"].clone();
                    if !is_array(&configure_options) {
                        self.errors.borrow_mut().push(format!(
                            "php-ext.configure-options : should be an array, {} given",
                            get_debug_type(&configure_options)
                        ));
                        php_ext.shift_remove("configure-options");
                    } else {
                        let configure_keys: Vec<String> = configure_options
                            .as_array()
                            .unwrap()
                            .keys()
                            .cloned()
                            .collect();
                        for key in &configure_keys {
                            let option = configure_options.as_array().unwrap()[key].clone();
                            if !is_array(&option) {
                                self.errors.borrow_mut().push(format!(
                                    "php-ext.configure-options.{} : should be an array, {} given",
                                    key,
                                    get_debug_type(&option)
                                ));
                                if let Some(PhpMixed::Array(arr)) =
                                    php_ext.get_mut("configure-options")
                                {
                                    arr.shift_remove(key);
                                }
                                continue;
                            }

                            let option_map = option.as_array().unwrap();
                            if !option_map.contains_key("name") {
                                self.errors.borrow_mut().push(format!(
                                    "php-ext.configure-options.{}.name : must be present",
                                    key
                                ));
                                if let Some(PhpMixed::Array(arr)) =
                                    php_ext.get_mut("configure-options")
                                {
                                    arr.shift_remove(key);
                                }
                                continue;
                            }

                            let name_val = option_map["name"].clone();
                            if !is_string(&name_val) {
                                self.errors.borrow_mut().push(format!(
                                    "php-ext.configure-options.{}.name : should be a string, {} given",
                                    key,
                                    get_debug_type(&name_val)
                                ));
                                if let Some(PhpMixed::Array(arr)) =
                                    php_ext.get_mut("configure-options")
                                {
                                    arr.shift_remove(key);
                                }
                                continue;
                            }

                            if let Some(needs_value) = option_map.get("needs-value").cloned()
                                && !is_bool(&needs_value)
                            {
                                self.errors.borrow_mut().push(format!(
                                        "php-ext.configure-options.{}.needs-value : should be a boolean, {} given",
                                        key,
                                        get_debug_type(&needs_value)
                                    ));
                                if let Some(PhpMixed::Array(co)) =
                                    php_ext.get_mut("configure-options")
                                    && let Some(entry) = co.get_mut(key)
                                    && let PhpMixed::Array(em) = entry
                                {
                                    em.shift_remove("needs-value");
                                }
                            }

                            if let Some(description) = option_map.get("description").cloned()
                                && !is_string(&description)
                            {
                                self.errors.borrow_mut().push(format!(
                                        "php-ext.configure-options.{}.description : should be a string, {} given",
                                        key,
                                        get_debug_type(&description)
                                    ));
                                if let Some(PhpMixed::Array(co)) =
                                    php_ext.get_mut("configure-options")
                                    && let Some(entry) = co.get_mut(key)
                                    && let PhpMixed::Array(em) = entry
                                {
                                    em.shift_remove("description");
                                }
                            }
                        }

                        let configure_empty = php_ext
                            .get("configure-options")
                            .and_then(|v| v.as_array())
                            .map(|m| m.is_empty())
                            .unwrap_or(true);
                        if configure_empty {
                            php_ext.shift_remove("configure-options");
                        }
                    }
                }

                // If php-ext is now empty, unset it
                if !php_ext.is_empty() {
                    self.config
                        .borrow_mut()
                        .insert("php-ext".to_string(), PhpMixed::Array(php_ext));
                }
            }
        }

        let unbound_constraint =
            SimpleConstraint::new("=".to_string(), "10000000-dev".to_string(), None).into();

        let link_types: Vec<&'static str> = SUPPORTED_LINK_TYPES.keys().copied().collect();
        for link_type in link_types {
            if self.validate_array(link_type, false) && self.config.borrow().contains_key(link_type)
            {
                let link_section = self.config.borrow()[link_type]
                    .as_array()
                    .cloned()
                    .unwrap_or_default();
                for (package, constraint) in &link_section {
                    let package = package.to_string();
                    let conflicts_with_own_name = self
                        .config
                        .borrow()
                        .get("name")
                        .and_then(|v| v.as_string())
                        .is_some_and(|name_val| strcasecmp(&package, name_val) == 0);
                    if conflicts_with_own_name {
                        self.errors.borrow_mut().push(format!(
                            "{}.{} : a package cannot set a {} on itself",
                            link_type, package, link_type
                        ));
                        if let Some(PhpMixed::Array(arr)) =
                            self.config.borrow_mut().get_mut(link_type)
                        {
                            arr.shift_remove(&package);
                        }
                        continue;
                    }
                    if let Some(err) = Self::has_package_naming_error(&package, true) {
                        self.warnings
                            .borrow_mut()
                            .push(format!("{}.{}", link_type, err));
                    } else if !Preg::is_match("{^[A-Za-z0-9_./-]+$}", &package) {
                        self.errors.borrow_mut().push(format!(
                            "{}.{} : invalid key, package names must be strings containing only [A-Za-z0-9_./-]",
                            link_type, package
                        ));
                    }
                    if !is_string(constraint) {
                        self.errors.borrow_mut().push(format!(
                            "{}.{} : invalid value, must be a string containing a version constraint",
                            link_type, package
                        ));
                        if let Some(PhpMixed::Array(arr)) =
                            self.config.borrow_mut().get_mut(link_type)
                        {
                            arr.shift_remove(&package);
                        }
                    } else if constraint.as_string().unwrap_or("") != "self.version" {
                        let constraint_str = constraint.as_string().unwrap_or("").to_string();
                        let link_constraint =
                            match self.version_parser.parse_constraints(&constraint_str) {
                                Ok(c) => c,
                                Err(e) => {
                                    self.errors.borrow_mut().push(format!(
                                        "{}.{} : invalid version constraint ({})",
                                        link_type, package, e
                                    ));
                                    if let Some(PhpMixed::Array(arr)) =
                                        self.config.borrow_mut().get_mut(link_type)
                                    {
                                        arr.shift_remove(&package);
                                    }
                                    continue;
                                }
                            };

                        // check requires for unbound constraints on non-platform packages
                        if (self.flags & Self::CHECK_UNBOUND_CONSTRAINTS) != 0
                            && link_type == "require"
                            && link_constraint.matches(&unbound_constraint)
                            && !PlatformRepository::is_platform_package(&package)
                        {
                            self.warnings.borrow_mut().push(format!(
                                "{}.{} : unbound version constraints ({}) should be avoided",
                                link_type, package, constraint_str
                            ));
                        } else if (self.flags & Self::CHECK_STRICT_CONSTRAINTS) != 0
                            && link_type == "require"
                            && link_constraint
                                .as_constraint()
                                .is_some_and(|c| ["==", "="].contains(&c.get_operator()))
                            && AnyConstraint::from(SimpleConstraint::new(
                                ">=".to_string(),
                                "1.0.0.0-dev".to_string(),
                                None,
                            ))
                            .matches(&link_constraint)
                        {
                            self.warnings.borrow_mut().push(format!(
                                "{}.{} : exact version constraints ({}) should be avoided if the package follows semantic versioning",
                                link_type, package, constraint_str
                            ));
                        }

                        let compacted = Intervals::compact_constraint(&link_constraint)?;
                        if compacted.is_match_none() {
                            self.warnings.borrow_mut().push(format!(
                                "{}.{} : this version constraint cannot possibly match anything ({})",
                                link_type, package, constraint_str
                            ));
                        }
                    }

                    if link_type == "conflict" && self.config.borrow().contains_key("replace") {
                        let replace_map = self
                            .config
                            .borrow()
                            .get("replace")
                            .and_then(|v| v.as_array())
                            .cloned()
                            .unwrap_or_default();
                        let conflict_map = self
                            .config
                            .borrow()
                            .get("conflict")
                            .and_then(|v| v.as_array())
                            .cloned()
                            .unwrap_or_default();
                        let replace_map_flat: IndexMap<String, PhpMixed> = replace_map;
                        let conflict_map_flat: IndexMap<String, PhpMixed> = conflict_map;
                        let keys = array_intersect_key(&replace_map_flat, &conflict_map_flat);
                        if !keys.is_empty() {
                            self.errors.borrow_mut().push(format!(
                                "{}.{} : you cannot conflict with a package that is also replaced, as replace already creates an implicit conflict rule",
                                link_type, package
                            ));
                            if let Some(PhpMixed::Array(arr)) =
                                self.config.borrow_mut().get_mut(link_type)
                            {
                                arr.shift_remove(&package);
                            }
                        }
                    }
                }
            }
        }

        if self.validate_array("suggest", false) && self.config.borrow().contains_key("suggest") {
            let suggest_map = self.config.borrow()["suggest"]
                .as_array()
                .cloned()
                .unwrap_or_default();
            for (package, description) in &suggest_map {
                if !is_string(description) {
                    self.errors.borrow_mut().push(format!(
                        "suggest.{} : invalid value, must be a string describing why the package is suggested",
                        package
                    ));
                    if let Some(PhpMixed::Array(arr)) = self.config.borrow_mut().get_mut("suggest")
                    {
                        arr.shift_remove(package);
                    }
                }
            }
        }

        if self.validate_string("minimum-stability", false)
            && self.config.borrow().contains_key("minimum-stability")
        {
            let min_stability = self.config.borrow()["minimum-stability"]
                .as_string()
                .unwrap_or("")
                .to_string();
            if !STABILITIES.contains_key(strtolower(&min_stability).as_str())
                && min_stability != "RC"
            {
                self.errors.borrow_mut().push(format!(
                    "minimum-stability : invalid value ({}), must be one of {}",
                    min_stability,
                    STABILITIES.keys().copied().collect::<Vec<_>>().join(", ")
                ));
                self.config.borrow_mut().shift_remove("minimum-stability");
            }
        }

        if self.validate_array("autoload", false) && self.config.borrow().contains_key("autoload") {
            let types = [
                "psr-0",
                "psr-4",
                "classmap",
                "files",
                "exclude-from-classmap",
            ];
            let autoload_keys: Vec<String> = self.config.borrow()["autoload"]
                .as_array()
                .map(|m| m.keys().cloned().collect())
                .unwrap_or_default();
            for r#type in &autoload_keys {
                let type_config =
                    self.config.borrow()["autoload"].as_array().unwrap()[r#type].clone();
                if !types.contains(&r#type.as_str()) {
                    self.errors.borrow_mut().push(format!(
                        "autoload : invalid value ({}), must be one of {}",
                        r#type,
                        types.join(", ")
                    ));
                    if let Some(PhpMixed::Array(arr)) = self.config.borrow_mut().get_mut("autoload")
                    {
                        arr.shift_remove(r#type);
                    }
                }
                if r#type == "psr-4"
                    && let Some(type_map) = type_config.as_array()
                {
                    for (namespace, _dirs) in type_map {
                        let ns_str = namespace.as_str();
                        if !ns_str.is_empty() && substr(ns_str, -1, None) != "\\" {
                            self.errors.borrow_mut().push(format!(
                                    "autoload.psr-4 : invalid value ({}), namespaces must end with a namespace separator, should be {}\\\\",
                                    ns_str, ns_str
                                ));
                        }
                    }
                }
            }
        }

        let has_psr4 = self
            .config
            .borrow()
            .get("autoload")
            .and_then(|v| v.as_array())
            .map(|m| m.contains_key("psr-4"))
            .unwrap_or(false);
        if has_psr4 && self.config.borrow().contains_key("target-dir") {
            self.errors.borrow_mut().push(
                "target-dir : this can not be used together with the autoload.psr-4 setting, remove target-dir to upgrade to psr-4".to_string()
            );
            // Unset the psr-4 setting, since unsetting target-dir might
            // interfere with other settings.
            if let Some(PhpMixed::Array(arr)) = self.config.borrow_mut().get_mut("autoload") {
                arr.shift_remove("psr-4");
            }
        }

        for src_type in ["source", "dist"] {
            if self.validate_array(src_type, false)
                && !Self::is_empty_array(self.config.borrow().get(src_type))
            {
                let section = self
                    .config
                    .borrow()
                    .get(src_type)
                    .and_then(|v| v.as_array())
                    .cloned()
                    .unwrap_or_default();
                // Mirror PHP `isset()`, which is false for both missing keys and null values.
                let isset =
                    |key: &str| matches!(section.get(key), Some(v) if !matches!(v, PhpMixed::Null));
                if !isset("type") {
                    self.errors
                        .borrow_mut()
                        .push(format!("{}.type : must be present", src_type));
                }
                if !isset("url") {
                    self.errors
                        .borrow_mut()
                        .push(format!("{}.url : must be present", src_type));
                }
                if src_type == "source" && !isset("reference") {
                    self.errors
                        .borrow_mut()
                        .push(format!("{}.reference : must be present", src_type));
                }
                if let Some(type_val) = section.get("type").filter(|_| isset("type"))
                    && !is_string(type_val)
                {
                    self.errors.borrow_mut().push(format!(
                        "{}.type : should be a string, {} given",
                        src_type,
                        get_debug_type(type_val)
                    ));
                }
                if let Some(url_val) = section.get("url").filter(|_| isset("url"))
                    && !is_string(url_val)
                {
                    self.errors.borrow_mut().push(format!(
                        "{}.url : should be a string, {} given",
                        src_type,
                        get_debug_type(url_val)
                    ));
                }
                if let Some(ref_val) = section.get("reference").filter(|_| isset("reference"))
                    && !is_string(ref_val)
                    && !is_int(ref_val)
                {
                    self.errors.borrow_mut().push(format!(
                        "{}.reference : should be a string or int, {} given",
                        src_type,
                        get_debug_type(ref_val)
                    ));
                }
                if let Some(ref_val) = section.get("reference").filter(|_| isset("reference")) {
                    let ref_str = php_to_string(ref_val);
                    if Preg::is_match("{^\\s*-}", &ref_str) {
                        self.errors.borrow_mut().push(format!(
                            "{}.reference : must not start with a \"-\", \"{}\" given",
                            src_type, ref_str
                        ));
                    }
                }
                if let Some(url_val) = section.get("url").filter(|_| isset("url")) {
                    let url_str = php_to_string(url_val);
                    if Preg::is_match("{^\\s*-}", &url_str) {
                        self.errors.borrow_mut().push(format!(
                            "{}.url : must not start with a \"-\", \"{}\" given",
                            src_type, url_str
                        ));
                    }
                }
            }
        }

        // TODO validate repositories
        // TODO validate package repositories' packages using this recursively

        self.validate_flat_array("include-path", None, false);
        self.validate_array("transport-options", false);

        // branch alias validation
        let has_branch_alias = self
            .config
            .borrow()
            .get("extra")
            .and_then(|v| v.as_array())
            .map(|m| m.contains_key("branch-alias"))
            .unwrap_or(false);
        if has_branch_alias {
            let branch_alias_val =
                self.config.borrow()["extra"].as_array().unwrap()["branch-alias"].clone();
            if !is_array(&branch_alias_val) {
                self.errors.borrow_mut().push(
                    "extra.branch-alias : must be an array of versions => aliases".to_string(),
                );
            } else {
                let branch_alias_map = branch_alias_val.as_array().cloned().unwrap_or_default();
                for (source_branch, target_branch) in &branch_alias_map {
                    if !is_string(target_branch) {
                        self.warnings.borrow_mut().push(format!(
                            "extra.branch-alias.{} : the target branch ({}) must be a string, \"{}\" received.",
                            source_branch,
                            json_encode(target_branch).unwrap_or_default(),
                            get_debug_type(target_branch)
                        ));
                        if let Some(PhpMixed::Array(extra)) =
                            self.config.borrow_mut().get_mut("extra")
                            && let Some(ba) = extra.get_mut("branch-alias")
                            && let PhpMixed::Array(bam) = ba
                        {
                            bam.shift_remove(source_branch);
                        }
                        continue;
                    }

                    let target_branch_str = target_branch.as_string().unwrap_or("").to_string();

                    // ensure it is an alias to a -dev package
                    if substr(&target_branch_str, -4, None) != "-dev" {
                        self.warnings.borrow_mut().push(format!(
                            "extra.branch-alias.{} : the target branch ({}) must end in -dev",
                            source_branch, target_branch_str
                        ));
                        if let Some(PhpMixed::Array(extra)) =
                            self.config.borrow_mut().get_mut("extra")
                            && let Some(ba) = extra.get_mut("branch-alias")
                            && let PhpMixed::Array(bam) = ba
                        {
                            bam.shift_remove(source_branch);
                        }
                        continue;
                    }

                    // normalize without -dev and ensure it's a numeric branch that is parseable
                    let trimmed = substr(
                        &target_branch_str,
                        0,
                        Some((target_branch_str.len() as i64) - 4),
                    );
                    let validated_target_branch = self.version_parser.normalize_branch(&trimmed)?;
                    if substr(&validated_target_branch, -4, None) != "-dev" {
                        self.warnings.borrow_mut().push(format!(
                            "extra.branch-alias.{} : the target branch ({}) must be a parseable number like 2.0-dev",
                            source_branch, target_branch_str
                        ));
                        if let Some(PhpMixed::Array(extra)) =
                            self.config.borrow_mut().get_mut("extra")
                            && let Some(ba) = extra.get_mut("branch-alias")
                            && let PhpMixed::Array(bam) = ba
                        {
                            bam.shift_remove(source_branch);
                        }
                        continue;
                    }

                    // If using numeric aliases ensure the alias is a valid subversion
                    let source_prefix = self
                        .version_parser
                        .parse_numeric_alias_prefix(source_branch);
                    let target_prefix = self
                        .version_parser
                        .parse_numeric_alias_prefix(&target_branch_str);
                    if let (Some(sp), Some(tp)) = (source_prefix, target_prefix)
                        && !tp.to_lowercase().starts_with(&sp.to_lowercase())
                    {
                        self.warnings.borrow_mut().push(format!(
                                "extra.branch-alias.{} : the target branch ({}) is not a valid numeric alias for this version",
                                source_branch, target_branch_str
                            ));
                        if let Some(PhpMixed::Array(extra)) =
                            self.config.borrow_mut().get_mut("extra")
                            && let Some(ba) = extra.get_mut("branch-alias")
                            && let PhpMixed::Array(bam) = ba
                        {
                            bam.shift_remove(source_branch);
                        }
                    }
                }
            }
        }

        if !self.errors.borrow().is_empty() {
            return Err(anyhow::anyhow!(InvalidPackageException::new(
                self.errors.borrow().clone(),
                self.warnings.borrow().clone(),
                config.iter().map(|(k, v)| (k.clone(), v.clone())).collect(),
            )));
        }

        let package = self.loader.load(
            self.config
                .borrow()
                .iter()
                .map(|(k, v)| (k.clone(), v.clone()))
                .collect(),
            Some(class),
        )?;
        *self.config.borrow_mut() = IndexMap::new();

        Ok(package)
    }
}

impl ValidatingArrayLoader {
    pub fn get_warnings(&self) -> Vec<String> {
        self.warnings.borrow().clone()
    }

    pub fn get_errors(&self) -> Vec<String> {
        self.errors.borrow().clone()
    }

    pub fn has_package_naming_error(name: &str, is_link: bool) -> Option<String> {
        if PlatformRepository::is_platform_package(name) {
            return None;
        }

        if !Preg::is_match(
            "{^[a-z0-9](?:[_.-]?[a-z0-9]++)*+/[a-z0-9](?:(?:[_.]|-{1,2})?[a-z0-9]++)*+$}iD",
            name,
        ) {
            return Some(format!(
                "{} is invalid, it should have a vendor name, a forward slash, and a package name. The vendor and package name can be words separated by -, . or _. The complete name should match \"^[a-z0-9]([_.-]?[a-z0-9]+)*/[a-z0-9](([_.]?|-{{0,2}})[a-z0-9]+)*$\".",
                name
            ));
        }

        let reserved_names = [
            "nul", "con", "prn", "aux", "com1", "com2", "com3", "com4", "com5", "com6", "com7",
            "com8", "com9", "lpt1", "lpt2", "lpt3", "lpt4", "lpt5", "lpt6", "lpt7", "lpt8", "lpt9",
        ];
        let lower = strtolower(name);
        let bits: Vec<&str> = lower.split('/').collect();
        if reserved_names.contains(&bits[0]) || reserved_names.contains(&bits[1]) {
            return Some(format!(
                "{} is reserved, package and vendor names can not match any of: {}.",
                name,
                reserved_names.join(", ")
            ));
        }

        if Preg::is_match("{\\.json$}", name) {
            return Some(format!(
                "{} is invalid, package names can not end in .json, consider renaming it or perhaps using a -json suffix instead.",
                name
            ));
        }

        if Preg::is_match("{[A-Z]}", name) {
            if is_link {
                return Some(format!(
                    "{} is invalid, it should not contain uppercase characters. Please use {} instead.",
                    name,
                    strtolower(name)
                ));
            }

            let suggest_name = Preg::replace(
                "{(?:([a-z])([A-Z])|([A-Z])([A-Z][a-z]))}",
                "\\1\\3-\\2\\4",
                name,
            );
            let suggest_name = strtolower(&suggest_name);

            return Some(format!(
                "{} is invalid, it should not contain uppercase characters. We suggest using {} instead.",
                name, suggest_name
            ));
        }

        None
    }

    fn validate_regex(&self, property: &str, regex: &str, mandatory: bool) -> bool {
        if !self.validate_string(property, mandatory) {
            return false;
        }

        let value = self.config.borrow()[property]
            .as_string()
            .unwrap_or("")
            .to_string();
        if !Preg::is_match(&format!("{{^{}$}}u", regex), &value) {
            let message = format!(
                "{} : invalid value ({}), must match {}",
                property, value, regex
            );
            if mandatory {
                self.errors.borrow_mut().push(message);
            } else {
                self.warnings.borrow_mut().push(message);
            }
            self.config.borrow_mut().shift_remove(property);

            return false;
        }

        true
    }

    fn validate_string(&self, property: &str, mandatory: bool) -> bool {
        if self.config.borrow().contains_key(property)
            && !is_string(&self.config.borrow()[property])
        {
            self.errors.borrow_mut().push(format!(
                "{} : should be a string, {} given",
                property,
                get_debug_type(&self.config.borrow()[property])
            ));
            self.config.borrow_mut().shift_remove(property);

            return false;
        }

        let is_empty = !self.config.borrow().contains_key(property)
            || trim(
                self.config.borrow()[property].as_string().unwrap_or(""),
                Some(" \t\n\r\0\u{0B}"),
            )
            .is_empty();
        if is_empty {
            if mandatory {
                self.errors
                    .borrow_mut()
                    .push(format!("{} : must be present", property));
            }
            self.config.borrow_mut().shift_remove(property);

            return false;
        }

        true
    }

    fn validate_array(&self, property: &str, mandatory: bool) -> bool {
        if self.config.borrow().contains_key(property) && !is_array(&self.config.borrow()[property])
        {
            self.errors.borrow_mut().push(format!(
                "{} : should be an array, {} given",
                property,
                get_debug_type(&self.config.borrow()[property])
            ));
            self.config.borrow_mut().shift_remove(property);

            return false;
        }

        let is_empty = !self.config.borrow().contains_key(property)
            || self.config.borrow()[property]
                .as_array()
                .map(|m| m.is_empty())
                .unwrap_or(true);
        if is_empty {
            if mandatory {
                self.errors.borrow_mut().push(format!(
                    "{} : must be present and contain at least one element",
                    property
                ));
            }
            self.config.borrow_mut().shift_remove(property);

            return false;
        }

        true
    }

    fn validate_flat_array(&self, property: &str, regex: Option<&str>, mandatory: bool) -> bool {
        if !self.validate_array(property, mandatory) {
            return false;
        }

        let mut pass = true;
        let entries: Vec<(String, PhpMixed)> = self.config.borrow()[property]
            .as_array()
            .map(|m| m.iter().map(|(k, v)| (k.clone(), v.clone())).collect())
            .unwrap_or_default();
        for (key, value) in entries {
            if !is_string(&value) && !is_numeric(&value) {
                self.errors.borrow_mut().push(format!(
                    "{}.{} : must be a string or int, {} given",
                    property,
                    key,
                    get_debug_type(&value)
                ));
                if let Some(PhpMixed::Array(arr)) = self.config.borrow_mut().get_mut(property) {
                    arr.shift_remove(&key);
                }
                pass = false;

                continue;
            }

            if let Some(regex_str) = regex {
                let value_str = php_to_string(&value);
                if !Preg::is_match(&format!("{{^{}$}}u", regex_str), &value_str) {
                    self.warnings.borrow_mut().push(format!(
                        "{}.{} : invalid value ({}), must match {}",
                        property, key, value_str, regex_str
                    ));
                    if let Some(PhpMixed::Array(arr)) = self.config.borrow_mut().get_mut(property) {
                        arr.shift_remove(&key);
                    }
                    pass = false;
                }
            }
        }

        pass
    }

    fn validate_url(&self, property: &str, mandatory: bool) -> bool {
        if !self.validate_string(property, mandatory) {
            return false;
        }

        let value = self.config.borrow()[property]
            .as_string()
            .unwrap_or("")
            .to_string();
        if !self.filter_url(&value, &["http", "https"]) {
            self.warnings.borrow_mut().push(format!(
                "{} : invalid value ({}), must be an http/https URL",
                property, value
            ));
            self.config.borrow_mut().shift_remove(property);

            return false;
        }

        true
    }

    fn filter_url(&self, value: &str, schemes: &[&str]) -> bool {
        if value.is_empty() {
            return true;
        }

        let bits = parse_url_all(value);
        let bits_map = match bits {
            PhpMixed::Array(m) => m,
            _ => return false,
        };
        let scheme = bits_map
            .get("scheme")
            .and_then(|v| v.as_string())
            .unwrap_or("");
        let host = bits_map
            .get("host")
            .and_then(|v| v.as_string())
            .unwrap_or("");
        if scheme.is_empty() || host.is_empty() {
            return false;
        }

        if !schemes.contains(&scheme) {
            return false;
        }

        true
    }

    fn is_empty_array(val: Option<&PhpMixed>) -> bool {
        match val {
            Some(v) => match v {
                PhpMixed::Array(m) => m.is_empty(),
                PhpMixed::Null => true,
                PhpMixed::Bool(false) => true,
                PhpMixed::String(s) => s.is_empty(),
                PhpMixed::Int(0) => true,
                _ => false,
            },
            None => true,
        }
    }
}
