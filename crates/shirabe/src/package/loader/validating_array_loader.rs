//! ref: composer/src/Composer/Package/Loader/ValidatingArrayLoader.php

use chrono::TimeZone;
use indexmap::IndexMap;

use shirabe_external_packages::composer::pcre::preg::Preg;
use shirabe_external_packages::composer::spdx_licenses::spdx_licenses::SpdxLicenses;
use shirabe_php_shim::{
    array_intersect_key, array_values, filter_var, get_debug_type, is_array, is_bool, is_int,
    is_numeric, is_scalar, is_string, json_encode, parse_url_all, php_to_string, sprintf,
    str_replace, strcasecmp, strtolower, strtotime, substr, trigger_error, trim, var_export,
    Exception, PhpMixed, E_USER_DEPRECATED, FILTER_VALIDATE_EMAIL, PHP_EOL,
};
use shirabe_semver::constraint::constraint::Constraint;
use shirabe_semver::constraint::match_none_constraint::MatchNoneConstraint;
use shirabe_semver::intervals::Intervals;

use crate::package::base_package::{BasePackage, STABILITIES, SUPPORTED_LINK_TYPES};
use crate::package::loader::invalid_package_exception::InvalidPackageException;
use crate::package::loader::loader_interface::LoaderInterface;
use crate::package::version::version_parser::VersionParser;
use crate::repository::platform_repository::PlatformRepository;

#[derive(Debug)]
pub struct ValidatingArrayLoader {
    loader: Box<dyn LoaderInterface>,
    version_parser: VersionParser,
    errors: Vec<String>,
    warnings: Vec<String>,
    config: IndexMap<String, Box<PhpMixed>>,
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
        let version_parser = parser.unwrap_or_else(|| VersionParser::new());

        if strict_name != true {
            trigger_error(
                "$strictName must be set to true in ValidatingArrayLoader's constructor as of 2.2, and it will be removed in 3.0",
                E_USER_DEPRECATED,
            );
        }

        Self {
            loader,
            version_parser,
            errors: Vec::new(),
            warnings: Vec::new(),
            config: IndexMap::new(),
            flags,
        }
    }

    pub fn load(
        &mut self,
        config: IndexMap<String, Box<PhpMixed>>,
        class: &str,
    ) -> anyhow::Result<Box<BasePackage>> {
        self.errors = Vec::new();
        self.warnings = Vec::new();
        self.config = config.clone();

        self.validate_string("name", true);
        if let Some(name_val) = config.get("name").and_then(|v| v.as_string()) {
            if let Some(err) = Self::has_package_naming_error(name_val, false) {
                self.errors.push(format!("name : {}", err));
            }
        }

        if self.config.contains_key("version") {
            let version_val = self.config["version"].clone();
            if !is_scalar(&*version_val) {
                self.validate_string("version", false);
            } else {
                if !is_string(&*version_val) {
                    self.config.insert(
                        "version".to_string(),
                        Box::new(PhpMixed::String(php_to_string(&*version_val))),
                    );
                }
                let version_str = self
                    .config
                    .get("version")
                    .and_then(|v| v.as_string())
                    .unwrap_or("")
                    .to_string();
                match self.version_parser.normalize(&version_str, None) {
                    Ok(_) => {}
                    Err(e) => {
                        self.errors.push(format!(
                            "version : invalid value ({}): {}",
                            version_str,
                            e
                        ));
                        self.config.shift_remove("version");
                    }
                }
            }
        }

        if let Some(config_section) = self
            .config
            .get("config")
            .and_then(|v| v.as_array())
            .cloned()
        {
            if let Some(platform_val) = config_section.get("platform") {
                let platform_array: IndexMap<String, Box<PhpMixed>> = match platform_val.as_ref() {
                    PhpMixed::Array(m) => m.clone(),
                    other => {
                        let mut m = IndexMap::new();
                        m.insert("0".to_string(), Box::new(other.clone()));
                        m
                    }
                };
                for (key, platform) in &platform_array {
                    if let PhpMixed::Bool(false) = platform.as_ref() {
                        continue;
                    }
                    if !is_string(platform) {
                        self.errors.push(format!(
                            "config.platform.{} : invalid value ({} {}): expected string or false",
                            key,
                            get_debug_type(platform),
                            var_export(platform, true)
                        ));
                        continue;
                    }
                    let platform_str = platform.as_string().unwrap_or("").to_string();
                    if let Err(e) = self.version_parser.normalize(&platform_str, None) {
                        self.errors.push(format!(
                            "config.platform.{} : invalid value ({}): {}",
                            key, platform_str, e
                        ));
                    }
                }
            }
        }

        self.validate_regex("type", "[A-Za-z0-9-]+", false);
        self.validate_string("target-dir", false);
        self.validate_array("extra", false);

        if self.config.contains_key("bin") {
            if is_string(&*self.config["bin"]) {
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
        if self.config.contains_key("time") {
            let time_str = self.config["time"].as_string().unwrap_or("").to_string();
            match Self::parse_datetime_utc(&time_str) {
                Ok(dt) => {
                    release_date = Some(dt);
                }
                Err(e) => {
                    self.errors.push(format!(
                        "time : invalid value ({}): {}",
                        time_str, e
                    ));
                    self.config.shift_remove("time");
                }
            }
        }

        if self.config.contains_key("license") {
            let license_val = self.config["license"].clone();
            // validate main data types
            if is_array(&*license_val) || is_string(&*license_val) {
                let mut licenses: IndexMap<String, Box<PhpMixed>> = match license_val.as_ref() {
                    PhpMixed::Array(m) => m.clone(),
                    other => {
                        let mut m = IndexMap::new();
                        m.insert("0".to_string(), Box::new(other.clone()));
                        m
                    }
                };

                let license_keys: Vec<String> = licenses.keys().cloned().collect();
                for index in &license_keys {
                    let license = licenses[index].clone();
                    if !is_string(&*license) {
                        self.warnings.push(sprintf(
                            "License %s should be a string.",
                            &[PhpMixed::String(json_encode(&*license).unwrap_or_default())],
                        ));
                        licenses.shift_remove(index);
                    }
                }

                // check for license validity on newly updated branches/tags
                let cutoff = strtotime("-8days").unwrap_or(0);
                if release_date.is_none()
                    || release_date.unwrap().timestamp() >= cutoff
                {
                    let license_validator = SpdxLicenses::new();
                    for license in licenses.values() {
                        let license_str = license.as_string().unwrap_or("").to_string();
                        // replace proprietary by MIT for validation purposes since it's not a valid SPDX identifier, but is accepted by composer
                        if license_str == "proprietary" {
                            continue;
                        }
                        let license_to_validate =
                            str_replace("proprietary", "MIT", &license_str);
                        if !license_validator.validate(&license_to_validate) {
                            if license_validator.validate(&trim(&license_to_validate, " \t\n\r\0\u{0B}")) {
                                self.warnings.push(sprintf(
                                    "License %s must not contain extra spaces, make sure to trim it.",
                                    &[PhpMixed::String(
                                        json_encode(&PhpMixed::String(license_str.clone()))
                                            .unwrap_or_default(),
                                    )],
                                ));
                            } else {
                                self.warnings.push(sprintf(
                                    &format!(
                                        "License %s is not a valid SPDX license identifier, see https://spdx.org/licenses/ if you use an open license.{}If the software is closed-source, you may use \"proprietary\" as license.",
                                        PHP_EOL
                                    ),
                                    &[PhpMixed::String(
                                        json_encode(&PhpMixed::String(license_str.clone()))
                                            .unwrap_or_default(),
                                    )],
                                ));
                            }
                        }
                    }
                }

                let reindexed: Vec<Box<PhpMixed>> = array_values(&licenses);
                let mut reindexed_map: IndexMap<String, Box<PhpMixed>> = IndexMap::new();
                for (i, v) in reindexed.into_iter().enumerate() {
                    reindexed_map.insert(i.to_string(), v);
                }
                self.config.insert(
                    "license".to_string(),
                    Box::new(PhpMixed::Array(reindexed_map)),
                );
            } else {
                self.warnings.push(sprintf(
                    "License must be a string or array of strings, got %s.",
                    &[PhpMixed::String(json_encode(&*license_val).unwrap_or_default())],
                ));
                self.config.shift_remove("license");
            }
        }

        if self.validate_array("authors", false) {
            let author_keys: Vec<String> = self.config["authors"]
                .as_array()
                .map(|a| a.keys().cloned().collect())
                .unwrap_or_default();
            for key in &author_keys {
                let author = self.config["authors"].as_array().unwrap()[key].clone();
                if !is_array(&*author) {
                    self.errors.push(format!(
                        "authors.{} : should be an array, {} given",
                        key,
                        get_debug_type(&*author)
                    ));
                    if let Some(PhpMixed::Array(m)) = self
                        .config
                        .get_mut("authors")
                        .map(|v| v.as_mut())
                    {
                        m.shift_remove(key);
                    }
                    continue;
                }
                for author_data in ["homepage", "email", "name", "role"] {
                    let val_opt = author
                        .as_array()
                        .and_then(|m| m.get(author_data))
                        .cloned();
                    if let Some(val) = val_opt {
                        if !is_string(&*val) {
                            self.errors.push(format!(
                                "authors.{}.{} : invalid value, must be a string",
                                key, author_data
                            ));
                            if let Some(PhpMixed::Array(authors)) =
                                self.config.get_mut("authors").map(|v| v.as_mut())
                            {
                                if let Some(author_entry) = authors.get_mut(key) {
                                    if let PhpMixed::Array(am) = author_entry.as_mut() {
                                        am.shift_remove(author_data);
                                    }
                                }
                            }
                        }
                    }
                }
                let homepage = author
                    .as_array()
                    .and_then(|m| m.get("homepage"))
                    .and_then(|v| v.as_string())
                    .map(|s| s.to_string());
                if let Some(homepage_str) = homepage {
                    if !self.filter_url(&homepage_str, &["http", "https"]) {
                        self.warnings.push(format!(
                            "authors.{}.homepage : invalid value ({}), must be an http/https URL",
                            key, homepage_str
                        ));
                        if let Some(PhpMixed::Array(authors)) =
                            self.config.get_mut("authors").map(|v| v.as_mut())
                        {
                            if let Some(author_entry) = authors.get_mut(key) {
                                if let PhpMixed::Array(am) = author_entry.as_mut() {
                                    am.shift_remove("homepage");
                                }
                            }
                        }
                    }
                }
                let email = author
                    .as_array()
                    .and_then(|m| m.get("email"))
                    .and_then(|v| v.as_string())
                    .map(|s| s.to_string());
                if let Some(email_str) = email {
                    if !filter_var(&email_str, FILTER_VALIDATE_EMAIL) {
                        self.warnings.push(format!(
                            "authors.{}.email : invalid value ({}), must be a valid email address",
                            key, email_str
                        ));
                        if let Some(PhpMixed::Array(authors)) =
                            self.config.get_mut("authors").map(|v| v.as_mut())
                        {
                            if let Some(author_entry) = authors.get_mut(key) {
                                if let PhpMixed::Array(am) = author_entry.as_mut() {
                                    am.shift_remove("email");
                                }
                            }
                        }
                    }
                }
                let current_author_len = self
                    .config
                    .get("authors")
                    .and_then(|v| v.as_array())
                    .and_then(|m| m.get(key))
                    .and_then(|v| v.as_array())
                    .map(|m| m.len())
                    .unwrap_or(0);
                if current_author_len == 0 {
                    if let Some(PhpMixed::Array(authors)) =
                        self.config.get_mut("authors").map(|v| v.as_mut())
                    {
                        authors.shift_remove(key);
                    }
                }
            }
            let authors_len = self
                .config
                .get("authors")
                .and_then(|v| v.as_array())
                .map(|m| m.len())
                .unwrap_or(0);
            if authors_len == 0 {
                self.config.shift_remove("authors");
            }
        }

        if self.validate_array("support", false)
            && !Self::is_empty_array(self.config.get("support"))
        {
            for key in [
                "issues", "forum", "wiki", "source", "email", "irc", "docs", "rss", "chat",
                "security",
            ] {
                let val_opt = self
                    .config
                    .get("support")
                    .and_then(|v| v.as_array())
                    .and_then(|m| m.get(key))
                    .cloned();
                if let Some(val) = val_opt {
                    if !is_string(&*val) {
                        self.errors.push(format!(
                            "support.{} : invalid value, must be a string",
                            key
                        ));
                        if let Some(PhpMixed::Array(support)) =
                            self.config.get_mut("support").map(|v| v.as_mut())
                        {
                            support.shift_remove(key);
                        }
                    }
                }
            }

            let support_email = self
                .config
                .get("support")
                .and_then(|v| v.as_array())
                .and_then(|m| m.get("email"))
                .and_then(|v| v.as_string())
                .map(|s| s.to_string());
            if let Some(email_str) = support_email {
                if !filter_var(&email_str, FILTER_VALIDATE_EMAIL) {
                    self.warnings.push(format!(
                        "support.email : invalid value ({}), must be a valid email address",
                        email_str
                    ));
                    if let Some(PhpMixed::Array(support)) =
                        self.config.get_mut("support").map(|v| v.as_mut())
                    {
                        support.shift_remove("email");
                    }
                }
            }

            let support_irc = self
                .config
                .get("support")
                .and_then(|v| v.as_array())
                .and_then(|m| m.get("irc"))
                .and_then(|v| v.as_string())
                .map(|s| s.to_string());
            if let Some(irc_str) = support_irc {
                if !self.filter_url(&irc_str, &["irc", "ircs"]) {
                    self.warnings.push(format!(
                        "support.irc : invalid value ({}), must be a irc://<server>/<channel> or ircs:// URL",
                        irc_str
                    ));
                    if let Some(PhpMixed::Array(support)) =
                        self.config.get_mut("support").map(|v| v.as_mut())
                    {
                        support.shift_remove("irc");
                    }
                }
            }

            for key in ["issues", "forum", "wiki", "source", "docs", "chat", "security"] {
                let url_opt = self
                    .config
                    .get("support")
                    .and_then(|v| v.as_array())
                    .and_then(|m| m.get(key))
                    .and_then(|v| v.as_string())
                    .map(|s| s.to_string());
                if let Some(url_str) = url_opt {
                    if !self.filter_url(&url_str, &["http", "https"]) {
                        self.warnings.push(format!(
                            "support.{} : invalid value ({}), must be an http/https URL",
                            key, url_str
                        ));
                        if let Some(PhpMixed::Array(support)) =
                            self.config.get_mut("support").map(|v| v.as_mut())
                        {
                            support.shift_remove(key);
                        }
                    }
                }
            }
            if Self::is_empty_array(self.config.get("support")) {
                self.config.shift_remove("support");
            }
        }

        if self.validate_array("funding", false)
            && !Self::is_empty_array(self.config.get("funding"))
        {
            let funding_keys: Vec<String> = self
                .config
                .get("funding")
                .and_then(|v| v.as_array())
                .map(|m| m.keys().cloned().collect())
                .unwrap_or_default();
            for key in &funding_keys {
                let funding_option = self.config["funding"].as_array().unwrap()[key].clone();
                if !is_array(&*funding_option) {
                    self.errors.push(format!(
                        "funding.{} : should be an array, {} given",
                        key,
                        get_debug_type(&*funding_option)
                    ));
                    if let Some(PhpMixed::Array(funding)) =
                        self.config.get_mut("funding").map(|v| v.as_mut())
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
                    if let Some(val) = val_opt {
                        if !is_string(&*val) {
                            self.errors.push(format!(
                                "funding.{}.{} : invalid value, must be a string",
                                key, funding_data
                            ));
                            if let Some(PhpMixed::Array(funding)) =
                                self.config.get_mut("funding").map(|v| v.as_mut())
                            {
                                if let Some(entry) = funding.get_mut(key) {
                                    if let PhpMixed::Array(em) = entry.as_mut() {
                                        em.shift_remove(funding_data);
                                    }
                                }
                            }
                        }
                    }
                }
                let url = funding_option
                    .as_array()
                    .and_then(|m| m.get("url"))
                    .and_then(|v| v.as_string())
                    .map(|s| s.to_string());
                if let Some(url_str) = url {
                    if !self.filter_url(&url_str, &["http", "https"]) {
                        self.warnings.push(format!(
                            "funding.{}.url : invalid value ({}), must be an http/https URL",
                            key, url_str
                        ));
                        if let Some(PhpMixed::Array(funding)) =
                            self.config.get_mut("funding").map(|v| v.as_mut())
                        {
                            if let Some(entry) = funding.get_mut(key) {
                                if let PhpMixed::Array(em) = entry.as_mut() {
                                    em.shift_remove("url");
                                }
                            }
                        }
                    }
                }
                let entry_empty = self
                    .config
                    .get("funding")
                    .and_then(|v| v.as_array())
                    .and_then(|m| m.get(key))
                    .and_then(|v| v.as_array())
                    .map(|m| m.is_empty())
                    .unwrap_or(true);
                if entry_empty {
                    if let Some(PhpMixed::Array(funding)) =
                        self.config.get_mut("funding").map(|v| v.as_mut())
                    {
                        funding.shift_remove(key);
                    }
                }
            }
            if Self::is_empty_array(self.config.get("funding")) {
                self.config.shift_remove("funding");
            }
        }

        if self.config.contains_key("php-ext") && self.validate_array("php-ext", false) {
            let pkg_type = self
                .config
                .get("type")
                .and_then(|v| v.as_string())
                .unwrap_or("")
                .to_string();
            if !["php-ext", "php-ext-zend"].contains(&pkg_type.as_str()) {
                self.errors.push(
                    "php-ext can only be set by packages of type \"php-ext\" or \"php-ext-zend\" which must be C extensions".to_string()
                );
                self.config.shift_remove("php-ext");
            }

            if self.config.contains_key("php-ext") {
                let mut php_ext: IndexMap<String, Box<PhpMixed>> =
                    match self.config.shift_remove("php-ext").unwrap().as_ref() {
                        PhpMixed::Array(m) => m.clone(),
                        _ => IndexMap::new(),
                    };

                if let Some(v) = php_ext.get("extension-name").cloned() {
                    if !is_string(&*v) {
                        self.errors.push(format!(
                            "php-ext.extension-name : should be a string, {} given",
                            get_debug_type(&*v)
                        ));
                        php_ext.shift_remove("extension-name");
                    }
                }

                if let Some(v) = php_ext.get("priority").cloned() {
                    if !is_int(&*v) {
                        self.errors.push(format!(
                            "php-ext.priority : should be an integer, {} given",
                            get_debug_type(&*v)
                        ));
                        php_ext.shift_remove("priority");
                    }
                }

                if let Some(v) = php_ext.get("support-zts").cloned() {
                    if !is_bool(&*v) {
                        self.errors.push(format!(
                            "php-ext.support-zts : should be a boolean, {} given",
                            get_debug_type(&*v)
                        ));
                        php_ext.shift_remove("support-zts");
                    }
                }

                if let Some(v) = php_ext.get("support-nts").cloned() {
                    if !is_bool(&*v) {
                        self.errors.push(format!(
                            "php-ext.support-nts : should be a boolean, {} given",
                            get_debug_type(&*v)
                        ));
                        php_ext.shift_remove("support-nts");
                    }
                }

                if let Some(v) = php_ext.get("build-path").cloned() {
                    if !is_string(&*v) && !matches!(v.as_ref(), PhpMixed::Null) {
                        self.errors.push(format!(
                            "php-ext.build-path : should be a string or null, {} given",
                            get_debug_type(&*v)
                        ));
                        php_ext.shift_remove("build-path");
                    }
                }

                if php_ext.contains_key("download-url-method") {
                    let v = php_ext["download-url-method"].clone();
                    if !is_array(&*v) && !is_string(&*v) {
                        self.errors.push(format!(
                            "php-ext.download-url-method : should be an array or a string, {} given",
                            get_debug_type(&*v)
                        ));
                        php_ext.shift_remove("download-url-method");
                    } else {
                        let valid_download_url_methods =
                            ["composer-default", "pre-packaged-source", "pre-packaged-binary"];
                        let defined_download_url_methods: IndexMap<String, Box<PhpMixed>> =
                            if is_array(&*v) {
                                v.as_array().unwrap().clone()
                            } else {
                                let mut m = IndexMap::new();
                                m.insert("0".to_string(), v.clone());
                                m
                            };

                        if defined_download_url_methods.is_empty() {
                            self.errors.push(
                                "php-ext.download-url-method : must contain at least one element"
                                    .to_string(),
                            );
                            php_ext.shift_remove("download-url-method");
                        } else {
                            for (key, download_url_method) in &defined_download_url_methods {
                                if !is_string(&**download_url_method) {
                                    self.errors.push(format!(
                                        "php-ext.download-url-method.{} : should be a string, {} given",
                                        key,
                                        get_debug_type(&**download_url_method)
                                    ));
                                    php_ext.shift_remove("download-url-method");
                                } else if !valid_download_url_methods.contains(
                                    &download_url_method.as_string().unwrap_or(""),
                                ) {
                                    self.errors.push(format!(
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
                    self.errors.push(
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
                            if !is_array(&*field_val) {
                                self.errors.push(format!(
                                    "php-ext.{} : should be an array, {} given",
                                    field_name,
                                    get_debug_type(&*field_val)
                                ));
                                php_ext.shift_remove(field_name);
                            } else if field_val.as_array().unwrap().is_empty() {
                                self.errors.push(format!(
                                    "php-ext.{} : must contain at least one element",
                                    field_name
                                ));
                                php_ext.shift_remove(field_name);
                            } else {
                                let field_keys: Vec<String> = field_val
                                    .as_array()
                                    .unwrap()
                                    .keys()
                                    .cloned()
                                    .collect();
                                for key in &field_keys {
                                    let os_family =
                                        field_val.as_array().unwrap()[key].clone();
                                    if !is_string(&*os_family) {
                                        self.errors.push(format!(
                                            "php-ext.{}.{} : should be a string, {} given",
                                            field_name,
                                            key,
                                            get_debug_type(&*os_family)
                                        ));
                                        if let Some(PhpMixed::Array(arr)) =
                                            php_ext.get_mut(field_name).map(|v| v.as_mut())
                                        {
                                            arr.shift_remove(key);
                                        }
                                    } else if !valid_os_families
                                        .contains(&os_family.as_string().unwrap_or(""))
                                    {
                                        self.errors.push(format!(
                                            "php-ext.{}.{} : invalid value ({}), must be one of {}",
                                            field_name,
                                            key,
                                            os_family.as_string().unwrap_or(""),
                                            valid_os_families.join(", ")
                                        ));
                                        if let Some(PhpMixed::Array(arr)) =
                                            php_ext.get_mut(field_name).map(|v| v.as_mut())
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
                    if !is_array(&*configure_options) {
                        self.errors.push(format!(
                            "php-ext.configure-options : should be an array, {} given",
                            get_debug_type(&*configure_options)
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
                            let option =
                                configure_options.as_array().unwrap()[key].clone();
                            if !is_array(&*option) {
                                self.errors.push(format!(
                                    "php-ext.configure-options.{} : should be an array, {} given",
                                    key,
                                    get_debug_type(&*option)
                                ));
                                if let Some(PhpMixed::Array(arr)) =
                                    php_ext.get_mut("configure-options").map(|v| v.as_mut())
                                {
                                    arr.shift_remove(key);
                                }
                                continue;
                            }

                            let option_map = option.as_array().unwrap();
                            if !option_map.contains_key("name") {
                                self.errors.push(format!(
                                    "php-ext.configure-options.{}.name : must be present",
                                    key
                                ));
                                if let Some(PhpMixed::Array(arr)) =
                                    php_ext.get_mut("configure-options").map(|v| v.as_mut())
                                {
                                    arr.shift_remove(key);
                                }
                                continue;
                            }

                            let name_val = option_map["name"].clone();
                            if !is_string(&*name_val) {
                                self.errors.push(format!(
                                    "php-ext.configure-options.{}.name : should be a string, {} given",
                                    key,
                                    get_debug_type(&*name_val)
                                ));
                                if let Some(PhpMixed::Array(arr)) =
                                    php_ext.get_mut("configure-options").map(|v| v.as_mut())
                                {
                                    arr.shift_remove(key);
                                }
                                continue;
                            }

                            if let Some(needs_value) = option_map.get("needs-value").cloned() {
                                if !is_bool(&*needs_value) {
                                    self.errors.push(format!(
                                        "php-ext.configure-options.{}.needs-value : should be a boolean, {} given",
                                        key,
                                        get_debug_type(&*needs_value)
                                    ));
                                    if let Some(PhpMixed::Array(co)) = php_ext
                                        .get_mut("configure-options")
                                        .map(|v| v.as_mut())
                                    {
                                        if let Some(entry) = co.get_mut(key) {
                                            if let PhpMixed::Array(em) = entry.as_mut() {
                                                em.shift_remove("needs-value");
                                            }
                                        }
                                    }
                                }
                            }

                            if let Some(description) =
                                option_map.get("description").cloned()
                            {
                                if !is_string(&*description) {
                                    self.errors.push(format!(
                                        "php-ext.configure-options.{}.description : should be a string, {} given",
                                        key,
                                        get_debug_type(&*description)
                                    ));
                                    if let Some(PhpMixed::Array(co)) = php_ext
                                        .get_mut("configure-options")
                                        .map(|v| v.as_mut())
                                    {
                                        if let Some(entry) = co.get_mut(key) {
                                            if let PhpMixed::Array(em) = entry.as_mut() {
                                                em.shift_remove("description");
                                            }
                                        }
                                    }
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
                    self.config.insert(
                        "php-ext".to_string(),
                        Box::new(PhpMixed::Array(php_ext)),
                    );
                }
            }
        }

        let unbound_constraint = Constraint::new("=", "10000000-dev");

        let link_types: Vec<&'static str> = SUPPORTED_LINK_TYPES.keys().copied().collect();
        for link_type in link_types {
            if self.validate_array(link_type, false) && self.config.contains_key(link_type) {
                let link_section = self.config[link_type]
                    .as_array()
                    .cloned()
                    .unwrap_or_default();
                for (package, constraint) in &link_section {
                    let package = package.to_string();
                    if let Some(name_val) = self.config.get("name").and_then(|v| v.as_string()) {
                        if strcasecmp(&package, name_val) == 0 {
                            self.errors.push(format!(
                                "{}.{} : a package cannot set a {} on itself",
                                link_type, package, link_type
                            ));
                            if let Some(PhpMixed::Array(arr)) =
                                self.config.get_mut(link_type).map(|v| v.as_mut())
                            {
                                arr.shift_remove(&package);
                            }
                            continue;
                        }
                    }
                    if let Some(err) = Self::has_package_naming_error(&package, true) {
                        self.warnings.push(format!("{}.{}", link_type, err));
                    } else if !Preg::is_match("{^[A-Za-z0-9_./-]+$}", &package).unwrap_or(false) {
                        self.errors.push(format!(
                            "{}.{} : invalid key, package names must be strings containing only [A-Za-z0-9_./-]",
                            link_type, package
                        ));
                    }
                    if !is_string(&**constraint) {
                        self.errors.push(format!(
                            "{}.{} : invalid value, must be a string containing a version constraint",
                            link_type, package
                        ));
                        if let Some(PhpMixed::Array(arr)) =
                            self.config.get_mut(link_type).map(|v| v.as_mut())
                        {
                            arr.shift_remove(&package);
                        }
                    } else if constraint.as_string().unwrap_or("") != "self.version" {
                        let constraint_str =
                            constraint.as_string().unwrap_or("").to_string();
                        let link_constraint =
                            match self.version_parser.parse_constraints(&constraint_str) {
                                Ok(c) => c,
                                Err(e) => {
                                    self.errors.push(format!(
                                        "{}.{} : invalid version constraint ({})",
                                        link_type, package, e
                                    ));
                                    if let Some(PhpMixed::Array(arr)) =
                                        self.config.get_mut(link_type).map(|v| v.as_mut())
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
                            self.warnings.push(format!(
                                "{}.{} : unbound version constraints ({}) should be avoided",
                                link_type, package, constraint_str
                            ));
                        } else if (self.flags & Self::CHECK_STRICT_CONSTRAINTS) != 0
                            && link_type == "require"
                            && link_constraint
                                .as_any()
                                .downcast_ref::<Constraint>()
                                .map_or(false, |c| {
                                    ["==", "="].contains(&c.get_operator())
                                })
                            && Constraint::new(">=", "1.0.0.0-dev")
                                .matches(link_constraint.as_ref())
                        {
                            self.warnings.push(format!(
                                "{}.{} : exact version constraints ({}) should be avoided if the package follows semantic versioning",
                                link_type, package, constraint_str
                            ));
                        }

                        let compacted = Intervals::compact_constraint(link_constraint.as_ref());
                        if compacted.as_any().is::<MatchNoneConstraint>() {
                            self.warnings.push(format!(
                                "{}.{} : this version constraint cannot possibly match anything ({})",
                                link_type, package, constraint_str
                            ));
                        }
                    }

                    if link_type == "conflict" && self.config.contains_key("replace") {
                        let replace_map = self
                            .config
                            .get("replace")
                            .and_then(|v| v.as_array())
                            .cloned()
                            .unwrap_or_default();
                        let conflict_map = self
                            .config
                            .get("conflict")
                            .and_then(|v| v.as_array())
                            .cloned()
                            .unwrap_or_default();
                        let keys = array_intersect_key(&replace_map, &conflict_map);
                        if !keys.is_empty() {
                            self.errors.push(format!(
                                "{}.{} : you cannot conflict with a package that is also replaced, as replace already creates an implicit conflict rule",
                                link_type, package
                            ));
                            if let Some(PhpMixed::Array(arr)) =
                                self.config.get_mut(link_type).map(|v| v.as_mut())
                            {
                                arr.shift_remove(&package);
                            }
                        }
                    }
                }
            }
        }

        if self.validate_array("suggest", false) && self.config.contains_key("suggest") {
            let suggest_map = self.config["suggest"]
                .as_array()
                .cloned()
                .unwrap_or_default();
            for (package, description) in &suggest_map {
                if !is_string(&**description) {
                    self.errors.push(format!(
                        "suggest.{} : invalid value, must be a string describing why the package is suggested",
                        package
                    ));
                    if let Some(PhpMixed::Array(arr)) =
                        self.config.get_mut("suggest").map(|v| v.as_mut())
                    {
                        arr.shift_remove(package);
                    }
                }
            }
        }

        if self.validate_string("minimum-stability", false)
            && self.config.contains_key("minimum-stability")
        {
            let min_stability = self.config["minimum-stability"]
                .as_string()
                .unwrap_or("")
                .to_string();
            if !STABILITIES.contains_key(strtolower(&min_stability).as_str())
                && min_stability != "RC"
            {
                self.errors.push(format!(
                    "minimum-stability : invalid value ({}), must be one of {}",
                    min_stability,
                    STABILITIES.keys().copied().collect::<Vec<_>>().join(", ")
                ));
                self.config.shift_remove("minimum-stability");
            }
        }

        if self.validate_array("autoload", false) && self.config.contains_key("autoload") {
            let types = ["psr-0", "psr-4", "classmap", "files", "exclude-from-classmap"];
            let autoload_keys: Vec<String> = self.config["autoload"]
                .as_array()
                .map(|m| m.keys().cloned().collect())
                .unwrap_or_default();
            for r#type in &autoload_keys {
                let type_config = self.config["autoload"].as_array().unwrap()[r#type].clone();
                if !types.contains(&r#type.as_str()) {
                    self.errors.push(format!(
                        "autoload : invalid value ({}), must be one of {}",
                        r#type,
                        types.join(", ")
                    ));
                    if let Some(PhpMixed::Array(arr)) =
                        self.config.get_mut("autoload").map(|v| v.as_mut())
                    {
                        arr.shift_remove(r#type);
                    }
                }
                if r#type == "psr-4" {
                    if let Some(type_map) = type_config.as_array() {
                        for (namespace, _dirs) in type_map {
                            let ns_str = namespace.as_str();
                            if ns_str != ""
                                && substr(ns_str, -1, None) != "\\"
                            {
                                self.errors.push(format!(
                                    "autoload.psr-4 : invalid value ({}), namespaces must end with a namespace separator, should be {}\\\\",
                                    ns_str, ns_str
                                ));
                            }
                        }
                    }
                }
            }
        }

        let has_psr4 = self
            .config
            .get("autoload")
            .and_then(|v| v.as_array())
            .map(|m| m.contains_key("psr-4"))
            .unwrap_or(false);
        if has_psr4 && self.config.contains_key("target-dir") {
            self.errors.push(
                "target-dir : this can not be used together with the autoload.psr-4 setting, remove target-dir to upgrade to psr-4".to_string()
            );
            // Unset the psr-4 setting, since unsetting target-dir might
            // interfere with other settings.
            if let Some(PhpMixed::Array(arr)) =
                self.config.get_mut("autoload").map(|v| v.as_mut())
            {
                arr.shift_remove("psr-4");
            }
        }

        for src_type in ["source", "dist"] {
            if self.validate_array(src_type, false)
                && !Self::is_empty_array(self.config.get(src_type))
            {
                let section = self
                    .config
                    .get(src_type)
                    .and_then(|v| v.as_array())
                    .cloned()
                    .unwrap_or_default();
                if !section.contains_key("type") {
                    self.errors
                        .push(format!("{}.type : must be present", src_type));
                }
                if !section.contains_key("url") {
                    self.errors
                        .push(format!("{}.url : must be present", src_type));
                }
                if src_type == "source" && !section.contains_key("reference") {
                    self.errors
                        .push(format!("{}.reference : must be present", src_type));
                }
                if let Some(type_val) = section.get("type") {
                    if !is_string(&**type_val) {
                        self.errors.push(format!(
                            "{}.type : should be a string, {} given",
                            src_type,
                            get_debug_type(&**type_val)
                        ));
                    }
                }
                if let Some(url_val) = section.get("url") {
                    if !is_string(&**url_val) {
                        self.errors.push(format!(
                            "{}.url : should be a string, {} given",
                            src_type,
                            get_debug_type(&**url_val)
                        ));
                    }
                }
                if let Some(ref_val) = section.get("reference") {
                    if !is_string(&**ref_val) && !is_int(&**ref_val) {
                        self.errors.push(format!(
                            "{}.reference : should be a string or int, {} given",
                            src_type,
                            get_debug_type(&**ref_val)
                        ));
                    }
                }
                if let Some(ref_val) = section.get("reference") {
                    let ref_str = php_to_string(&**ref_val);
                    if Preg::is_match("{^\\s*-}", &ref_str).unwrap_or(false) {
                        self.errors.push(format!(
                            "{}.reference : must not start with a \"-\", \"{}\" given",
                            src_type, ref_str
                        ));
                    }
                }
                if let Some(url_val) = section.get("url") {
                    let url_str = php_to_string(&**url_val);
                    if Preg::is_match("{^\\s*-}", &url_str).unwrap_or(false) {
                        self.errors.push(format!(
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
            .get("extra")
            .and_then(|v| v.as_array())
            .map(|m| m.contains_key("branch-alias"))
            .unwrap_or(false);
        if has_branch_alias {
            let branch_alias_val = self.config["extra"].as_array().unwrap()["branch-alias"].clone();
            if !is_array(&*branch_alias_val) {
                self.errors
                    .push("extra.branch-alias : must be an array of versions => aliases".to_string());
            } else {
                let branch_alias_map = branch_alias_val.as_array().cloned().unwrap_or_default();
                for (source_branch, target_branch) in &branch_alias_map {
                    if !is_string(&**target_branch) {
                        self.warnings.push(format!(
                            "extra.branch-alias.{} : the target branch ({}) must be a string, \"{}\" received.",
                            source_branch,
                            json_encode(&**target_branch).unwrap_or_default(),
                            get_debug_type(&**target_branch)
                        ));
                        if let Some(PhpMixed::Array(extra)) =
                            self.config.get_mut("extra").map(|v| v.as_mut())
                        {
                            if let Some(ba) = extra.get_mut("branch-alias") {
                                if let PhpMixed::Array(bam) = ba.as_mut() {
                                    bam.shift_remove(source_branch);
                                }
                            }
                        }
                        continue;
                    }

                    let target_branch_str = target_branch.as_string().unwrap_or("").to_string();

                    // ensure it is an alias to a -dev package
                    if substr(&target_branch_str, -4, None) != "-dev" {
                        self.warnings.push(format!(
                            "extra.branch-alias.{} : the target branch ({}) must end in -dev",
                            source_branch, target_branch_str
                        ));
                        if let Some(PhpMixed::Array(extra)) =
                            self.config.get_mut("extra").map(|v| v.as_mut())
                        {
                            if let Some(ba) = extra.get_mut("branch-alias") {
                                if let PhpMixed::Array(bam) = ba.as_mut() {
                                    bam.shift_remove(source_branch);
                                }
                            }
                        }
                        continue;
                    }

                    // normalize without -dev and ensure it's a numeric branch that is parseable
                    let trimmed = substr(
                        &target_branch_str,
                        0,
                        Some((target_branch_str.len() as i64) - 4),
                    );
                    let validated_target_branch = self
                        .version_parser
                        .normalize_branch(&trimmed);
                    if substr(&validated_target_branch, -4, None) != "-dev" {
                        self.warnings.push(format!(
                            "extra.branch-alias.{} : the target branch ({}) must be a parseable number like 2.0-dev",
                            source_branch, target_branch_str
                        ));
                        if let Some(PhpMixed::Array(extra)) =
                            self.config.get_mut("extra").map(|v| v.as_mut())
                        {
                            if let Some(ba) = extra.get_mut("branch-alias") {
                                if let PhpMixed::Array(bam) = ba.as_mut() {
                                    bam.shift_remove(source_branch);
                                }
                            }
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
                    if let (Some(sp), Some(tp)) = (source_prefix, target_prefix) {
                        if !tp.to_lowercase().starts_with(&sp.to_lowercase()) {
                            self.warnings.push(format!(
                                "extra.branch-alias.{} : the target branch ({}) is not a valid numeric alias for this version",
                                source_branch, target_branch_str
                            ));
                            if let Some(PhpMixed::Array(extra)) =
                                self.config.get_mut("extra").map(|v| v.as_mut())
                            {
                                if let Some(ba) = extra.get_mut("branch-alias") {
                                    if let PhpMixed::Array(bam) = ba.as_mut() {
                                        bam.shift_remove(source_branch);
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }

        if !self.errors.is_empty() {
            return Err(anyhow::anyhow!(InvalidPackageException::new(
                self.errors.clone(),
                self.warnings.clone(),
                config.values().map(|v| (**v).clone()).collect(),
            )));
        }

        let package = self
            .loader
            .load(
                self.config
                    .iter()
                    .map(|(k, v)| (k.clone(), (**v).clone()))
                    .collect(),
                Some(class.to_string()),
            )?;
        self.config = IndexMap::new();

        Ok(package)
    }

    pub fn get_warnings(&self) -> &[String] {
        &self.warnings
    }

    pub fn get_errors(&self) -> &[String] {
        &self.errors
    }

    pub fn has_package_naming_error(name: &str, is_link: bool) -> Option<String> {
        if PlatformRepository::is_platform_package(name) {
            return None;
        }

        if !Preg::is_match(
            "{^[a-z0-9](?:[_.-]?[a-z0-9]++)*+/[a-z0-9](?:(?:[_.]|-{1,2})?[a-z0-9]++)*+$}iD",
            name,
        )
        .unwrap_or(false)
        {
            return Some(format!("{} is invalid, it should have a vendor name, a forward slash, and a package name. The vendor and package name can be words separated by -, . or _. The complete name should match \"^[a-z0-9]([_.-]?[a-z0-9]+)*/[a-z0-9](([_.]?|-{{0,2}})[a-z0-9]+)*$\".", name));
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

        if Preg::is_match("{\\.json$}", name).unwrap_or(false) {
            return Some(format!(
                "{} is invalid, package names can not end in .json, consider renaming it or perhaps using a -json suffix instead.",
                name
            ));
        }

        if Preg::is_match("{[A-Z]}", name).unwrap_or(false) {
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
            )
            .unwrap_or_else(|_| name.to_string());
            let suggest_name = strtolower(&suggest_name);

            return Some(format!(
                "{} is invalid, it should not contain uppercase characters. We suggest using {} instead.",
                name, suggest_name
            ));
        }

        None
    }

    fn validate_regex(&mut self, property: &str, regex: &str, mandatory: bool) -> bool {
        if !self.validate_string(property, mandatory) {
            return false;
        }

        let value = self.config[property].as_string().unwrap_or("").to_string();
        if !Preg::is_match(&format!("{{^{}$}}u", regex), &value).unwrap_or(false) {
            let message = format!("{} : invalid value ({}), must match {}", property, value, regex);
            if mandatory {
                self.errors.push(message);
            } else {
                self.warnings.push(message);
            }
            self.config.shift_remove(property);

            return false;
        }

        true
    }

    fn validate_string(&mut self, property: &str, mandatory: bool) -> bool {
        if self.config.contains_key(property) && !is_string(&*self.config[property]) {
            self.errors.push(format!(
                "{} : should be a string, {} given",
                property,
                get_debug_type(&*self.config[property])
            ));
            self.config.shift_remove(property);

            return false;
        }

        let is_empty = !self.config.contains_key(property)
            || trim(self.config[property].as_string().unwrap_or(""), " \t\n\r\0\u{0B}") == "";
        if is_empty {
            if mandatory {
                self.errors
                    .push(format!("{} : must be present", property));
            }
            self.config.shift_remove(property);

            return false;
        }

        true
    }

    fn validate_array(&mut self, property: &str, mandatory: bool) -> bool {
        if self.config.contains_key(property) && !is_array(&*self.config[property]) {
            self.errors.push(format!(
                "{} : should be an array, {} given",
                property,
                get_debug_type(&*self.config[property])
            ));
            self.config.shift_remove(property);

            return false;
        }

        let is_empty = !self.config.contains_key(property)
            || self.config[property]
                .as_array()
                .map(|m| m.is_empty())
                .unwrap_or(true);
        if is_empty {
            if mandatory {
                self.errors.push(format!(
                    "{} : must be present and contain at least one element",
                    property
                ));
            }
            self.config.shift_remove(property);

            return false;
        }

        true
    }

    fn validate_flat_array(
        &mut self,
        property: &str,
        regex: Option<&str>,
        mandatory: bool,
    ) -> bool {
        if !self.validate_array(property, mandatory) {
            return false;
        }

        let mut pass = true;
        let entries: Vec<(String, Box<PhpMixed>)> = self.config[property]
            .as_array()
            .map(|m| m.iter().map(|(k, v)| (k.clone(), v.clone())).collect())
            .unwrap_or_default();
        for (key, value) in entries {
            if !is_string(&*value) && !is_numeric(&*value) {
                self.errors.push(format!(
                    "{}.{} : must be a string or int, {} given",
                    property,
                    key,
                    get_debug_type(&*value)
                ));
                if let Some(PhpMixed::Array(arr)) =
                    self.config.get_mut(property).map(|v| v.as_mut())
                {
                    arr.shift_remove(&key);
                }
                pass = false;

                continue;
            }

            if let Some(regex_str) = regex {
                let value_str = php_to_string(&*value);
                if !Preg::is_match(&format!("{{^{}$}}u", regex_str), &value_str).unwrap_or(false) {
                    self.warnings.push(format!(
                        "{}.{} : invalid value ({}), must match {}",
                        property, key, value_str, regex_str
                    ));
                    if let Some(PhpMixed::Array(arr)) =
                        self.config.get_mut(property).map(|v| v.as_mut())
                    {
                        arr.shift_remove(&key);
                    }
                    pass = false;
                }
            }
        }

        pass
    }

    fn validate_url(&mut self, property: &str, mandatory: bool) -> bool {
        if !self.validate_string(property, mandatory) {
            return false;
        }

        let value = self.config[property].as_string().unwrap_or("").to_string();
        if !self.filter_url(&value, &["http", "https"]) {
            self.warnings.push(format!(
                "{} : invalid value ({}), must be an http/https URL",
                property, value
            ));
            self.config.shift_remove(property);

            return false;
        }

        true
    }

    fn filter_url(&self, value: &str, schemes: &[&str]) -> bool {
        if value == "" {
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

    fn is_empty_array(val: Option<&Box<PhpMixed>>) -> bool {
        match val {
            Some(v) => match v.as_ref() {
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

    fn parse_datetime_utc(s: &str) -> anyhow::Result<chrono::DateTime<chrono::Utc>> {
        // TODO(phase-b): PHP's `new \DateTime($s, new \DateTimeZone('UTC'))` accepts
        // many free-form formats; approximate with chrono for now.
        if let Ok(dt) = chrono::DateTime::parse_from_rfc3339(s) {
            return Ok(dt.with_timezone(&chrono::Utc));
        }
        if let Ok(dt) = chrono::NaiveDateTime::parse_from_str(s, "%Y-%m-%d %H:%M:%S") {
            return Ok(chrono::Utc.from_utc_datetime(&dt));
        }
        if let Ok(d) = chrono::NaiveDate::parse_from_str(s, "%Y-%m-%d") {
            return Ok(chrono::Utc.from_utc_datetime(&d.and_hms_opt(0, 0, 0).unwrap()));
        }
        Err(anyhow::anyhow!(Exception {
            message: format!("Failed to parse date: {}", s),
            code: 0,
        }))
    }
}
