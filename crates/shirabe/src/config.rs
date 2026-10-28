//! ref: composer/src/Composer/Config.php

mod config_source_interface;
mod json_config_source;

pub use config_source_interface::*;
pub use json_config_source::*;

use crate::io::{IOInterface, IOInterfaceImmutable, io_interface};
use bstr::{B, BString, ByteSlice};
use indexmap::IndexMap;
use shirabe_php_shim::{
    E_USER_DEPRECATED, PhpMixed, PregMatches, RuntimeException, array_key_exists, array_merge,
    array_search_mixed, array_unique, canonical_int_key, empty, filter_var_url, implode,
    in_array_loose, in_array_strict, intval, is_array, is_string, legacy_lossy, parse_url,
    php_regex, php_to_string, preg_is_match, preg_match, preg_replace_callback, rtrim2, strtolower,
    strtoupper, strtr, substr, trigger_error,
};

use crate::advisory::Auditor;
use crate::downloader::TransportException;
use crate::util::{Platform, ProcessExecutor};

#[derive(Debug)]
pub struct Config {
    config: IndexMap<BString, PhpMixed>,
    base_dir: Option<String>,
    repositories: IndexMap<BString, PhpMixed>,
    config_source: Option<Box<dyn ConfigSourceInterface>>,
    auth_config_source: Option<Box<dyn ConfigSourceInterface>>,
    local_auth_config_source: Option<Box<dyn ConfigSourceInterface>>,
    use_environment: bool,
    warned_hosts: IndexMap<String, bool>,
    ssl_verify_warned_hosts: IndexMap<String, bool>,
    source_of_config_value: std::cell::RefCell<IndexMap<String, String>>,
}

impl Config {
    pub const SOURCE_DEFAULT: &'static str = "default";
    pub const SOURCE_COMMAND: &'static str = "command";
    pub const SOURCE_UNKNOWN: &'static str = "unknown";

    pub const RELATIVE_PATHS: i64 = 1;

    pub fn default_config() -> IndexMap<BString, PhpMixed> {
        let mut c: IndexMap<BString, PhpMixed> = IndexMap::new();
        c.insert("process-timeout".into(), PhpMixed::Int(300));
        c.insert("use-include-path".into(), PhpMixed::Bool(false));
        c.insert("allow-plugins".into(), PhpMixed::Array(IndexMap::new()));
        c.insert(
            "use-parent-dir".into(),
            PhpMixed::string("prompt".to_string()),
        );
        c.insert(
            "preferred-install".into(),
            PhpMixed::string("dist".to_string()),
        );
        let mut audit: IndexMap<BString, PhpMixed> = IndexMap::new();
        audit.insert("ignore".into(), PhpMixed::Array(IndexMap::new()));
        audit.insert(
            "abandoned".into(),
            PhpMixed::string(Auditor::ABANDONED_FAIL.to_string()),
        );
        c.insert("audit".into(), PhpMixed::Array(audit));
        c.insert("notify-on-install".into(), PhpMixed::Bool(true));
        c.insert(
            "github-protocols".into(),
            PhpMixed::List(vec![
                PhpMixed::string("https".to_string()),
                PhpMixed::string("ssh".to_string()),
                PhpMixed::string("git".to_string()),
            ]),
        );
        c.insert("gitlab-protocol".into(), PhpMixed::Null);
        c.insert("vendor-dir".into(), PhpMixed::string("vendor".to_string()));
        c.insert(
            "bin-dir".into(),
            PhpMixed::string("{$vendor-dir}/bin".to_string()),
        );
        c.insert(
            "cache-dir".into(),
            PhpMixed::string("{$home}/cache".to_string()),
        );
        c.insert("data-dir".into(), PhpMixed::string("{$home}".to_string()));
        c.insert(
            "cache-files-dir".into(),
            PhpMixed::string("{$cache-dir}/files".to_string()),
        );
        c.insert(
            "cache-repo-dir".into(),
            PhpMixed::string("{$cache-dir}/repo".to_string()),
        );
        c.insert(
            "cache-vcs-dir".into(),
            PhpMixed::string("{$cache-dir}/vcs".to_string()),
        );
        c.insert("cache-ttl".into(), PhpMixed::Int(15552000)); // 6 months
        c.insert("cache-files-ttl".into(), PhpMixed::Null); // fallback to cache-ttl
        c.insert(
            "cache-files-maxsize".into(),
            PhpMixed::string("300MiB".to_string()),
        );
        c.insert("cache-read-only".into(), PhpMixed::Bool(false));
        c.insert("bin-compat".into(), PhpMixed::string("auto".to_string()));
        c.insert("discard-changes".into(), PhpMixed::Bool(false));
        c.insert("autoloader-suffix".into(), PhpMixed::Null);
        c.insert("sort-packages".into(), PhpMixed::Bool(false));
        c.insert("optimize-autoloader".into(), PhpMixed::Bool(false));
        c.insert("classmap-authoritative".into(), PhpMixed::Bool(false));
        c.insert("apcu-autoloader".into(), PhpMixed::Bool(false));
        c.insert("prepend-autoloader".into(), PhpMixed::Bool(true));
        c.insert("update-with-minimal-changes".into(), PhpMixed::Bool(false));
        c.insert(
            "github-domains".into(),
            PhpMixed::List(vec![PhpMixed::string("github.com".to_string())]),
        );
        c.insert("bitbucket-expose-hostname".into(), PhpMixed::Bool(true));
        c.insert("disable-tls".into(), PhpMixed::Bool(false));
        c.insert("secure-http".into(), PhpMixed::Bool(true));
        c.insert("secure-svn-domains".into(), PhpMixed::List(vec![]));
        c.insert("cafile".into(), PhpMixed::Null);
        c.insert("capath".into(), PhpMixed::Null);
        c.insert("github-expose-hostname".into(), PhpMixed::Bool(true));
        c.insert(
            "gitlab-domains".into(),
            PhpMixed::List(vec![PhpMixed::string("gitlab.com".to_string())]),
        );
        c.insert("store-auths".into(), PhpMixed::string("prompt".to_string()));
        c.insert("platform".into(), PhpMixed::Array(IndexMap::new()));
        c.insert("archive-format".into(), PhpMixed::string("tar".to_string()));
        c.insert("archive-dir".into(), PhpMixed::string(".".to_string()));
        c.insert("htaccess-protect".into(), PhpMixed::Bool(true));
        c.insert("use-github-api".into(), PhpMixed::Bool(true));
        c.insert("lock".into(), PhpMixed::Bool(true));
        c.insert(
            "platform-check".into(),
            PhpMixed::string("php-only".to_string()),
        );
        c.insert("bitbucket-oauth".into(), PhpMixed::Array(IndexMap::new()));
        c.insert("github-oauth".into(), PhpMixed::Array(IndexMap::new()));
        c.insert("gitlab-oauth".into(), PhpMixed::Array(IndexMap::new()));
        c.insert("gitlab-token".into(), PhpMixed::Array(IndexMap::new()));
        c.insert("http-basic".into(), PhpMixed::Array(IndexMap::new()));
        c.insert("bearer".into(), PhpMixed::Array(IndexMap::new()));
        c.insert("custom-headers".into(), PhpMixed::Array(IndexMap::new()));
        c.insert("bump-after-update".into(), PhpMixed::Bool(false));
        c.insert("allow-missing-requirements".into(), PhpMixed::Bool(false));
        c.insert(
            "client-certificate".into(),
            PhpMixed::Array(IndexMap::new()),
        );
        c.insert(
            "forgejo-domains".into(),
            PhpMixed::List(vec![PhpMixed::string("codeberg.org".to_string())]),
        );
        c.insert("forgejo-token".into(), PhpMixed::Array(IndexMap::new()));
        c
    }

    pub fn default_repositories() -> IndexMap<BString, PhpMixed> {
        let mut r: IndexMap<BString, PhpMixed> = IndexMap::new();
        let mut packagist: IndexMap<BString, PhpMixed> = IndexMap::new();
        packagist.insert("type".into(), PhpMixed::string("composer".to_string()));
        packagist.insert(
            "url".into(),
            PhpMixed::string("https://repo.packagist.org".to_string()),
        );
        r.insert("packagist.org".into(), PhpMixed::Array(packagist));
        r
    }

    pub fn new(use_environment: bool, base_dir: Option<String>) -> Self {
        let this = Self {
            // load defaults
            config: Self::default_config(),
            repositories: Self::default_repositories(),
            use_environment,
            base_dir: base_dir.filter(|s| is_string(&PhpMixed::string(s.clone())) && !s.is_empty()),
            config_source: None,
            auth_config_source: None,
            local_auth_config_source: None,
            warned_hosts: IndexMap::new(),
            ssl_verify_warned_hosts: IndexMap::new(),
            source_of_config_value: std::cell::RefCell::new(IndexMap::new()),
        };

        let config_clone = this.config.clone();
        for (config_key, config_value) in &config_clone {
            // TODO(bytes): config source paths are Strings, so non-UTF-8 bytes are replaced.
            this.set_source_of_config_value(
                config_value,
                &config_key.to_str_lossy(),
                Self::SOURCE_DEFAULT,
            );
        }

        let repositories_clone = this.repositories.clone();
        for (config_key, config_value) in &repositories_clone {
            this.set_source_of_config_value(
                config_value,
                &format!("repositories.{}", config_key),
                Self::SOURCE_DEFAULT,
            );
        }

        this
    }

    /// Changing this can break path resolution for relative config paths so do not call this without knowing what you are doing
    ///
    /// The $baseDir should be an absolute path and without trailing slash
    pub fn set_base_dir(&mut self, base_dir: Option<String>) {
        self.base_dir = base_dir;
    }

    pub fn set_config_source(&mut self, source: Box<dyn ConfigSourceInterface>) {
        self.config_source = Some(source);
    }

    pub fn get_config_source(&self) -> &dyn ConfigSourceInterface {
        self.config_source.as_ref().unwrap().as_ref()
    }

    pub fn get_config_source_mut(&mut self) -> &mut dyn ConfigSourceInterface {
        self.config_source.as_mut().unwrap().as_mut()
    }

    pub fn set_auth_config_source(&mut self, source: Box<dyn ConfigSourceInterface>) {
        self.auth_config_source = Some(source);
    }

    pub fn get_auth_config_source(&self) -> &dyn ConfigSourceInterface {
        self.auth_config_source.as_ref().unwrap().as_ref()
    }

    pub fn get_auth_config_source_mut(&mut self) -> &mut dyn ConfigSourceInterface {
        self.auth_config_source.as_mut().unwrap().as_mut()
    }

    pub fn set_local_auth_config_source(&mut self, source: Box<dyn ConfigSourceInterface>) {
        self.local_auth_config_source = Some(source);
    }

    pub fn get_local_auth_config_source(&self) -> Option<&dyn ConfigSourceInterface> {
        self.local_auth_config_source.as_deref()
    }

    pub fn get_local_auth_config_source_mut(
        &mut self,
    ) -> Option<&mut (dyn ConfigSourceInterface + 'static)> {
        self.local_auth_config_source
            .as_mut()
            .map(|b| &mut **b as &mut dyn ConfigSourceInterface)
    }

    /// Merges new config values with the existing ones (overriding)
    pub fn merge(&mut self, config: &IndexMap<BString, PhpMixed>, source: &str) {
        // override defaults with given config
        let config_section = config.get(B("config")).cloned().unwrap_or(PhpMixed::Null);
        if !empty(&config_section) && is_array(&config_section) {
            let config_section_map = match config_section {
                PhpMixed::Array(m) => m,
                _ => IndexMap::new(),
            };
            for (key, val_box) in &config_section_map {
                // TODO(bytes): config source paths are Strings, so non-UTF-8 bytes are replaced.
                let key_str = key.to_str_lossy();
                let val = val_box.clone();
                if matches!(
                    key_str.as_ref(),
                    "bitbucket-oauth"
                        | "github-oauth"
                        | "gitlab-oauth"
                        | "gitlab-token"
                        | "http-basic"
                        | "bearer"
                        | "client-certificate"
                        | "forgejo-token"
                ) && self.config.contains_key(key)
                {
                    let existing = self.config.get(key).cloned().unwrap_or(PhpMixed::Null);
                    self.config
                        .insert(key.clone(), array_merge(existing, val.clone()));
                    self.set_source_of_config_value(&val, &key_str, source);
                } else if key == "allow-plugins"
                    && self.config.contains_key(key)
                    && is_array(self.config.get(key).unwrap_or(&PhpMixed::Null))
                    && is_array(&val)
                {
                    // merging $val first to get the local config on top of the global one, then appending the global config,
                    // then merging local one again to make sure the values from local win over global ones for keys present in both
                    let existing = self.config.get(key).cloned().unwrap_or(PhpMixed::Null);
                    self.config.insert(
                        key.clone(),
                        array_merge(array_merge(val.clone(), existing), val.clone()),
                    );
                    self.set_source_of_config_value(&val, &key_str, source);
                } else if matches!(key_str.as_ref(), "gitlab-domains" | "github-domains")
                    && self.config.contains_key(key)
                {
                    let existing = self.config.get(key).cloned().unwrap_or(PhpMixed::Null);
                    let merged = array_merge(existing, val.clone());
                    let unique_list: Vec<String> = match &merged {
                        PhpMixed::List(l) => l
                            .iter()
                            .filter_map(|v| v.as_string().map(|s| s.to_string()))
                            .collect(),
                        _ => vec![],
                    };
                    let deduped = array_unique(&unique_list);
                    self.config.insert(
                        key.clone(),
                        PhpMixed::List(deduped.into_iter().map(PhpMixed::string).collect()),
                    );
                    self.set_source_of_config_value(&val, &key_str, source);
                } else if key == "preferred-install" && self.config.contains_key(key) {
                    let mut val = val.clone();
                    let existing = self.config.get(key).cloned().unwrap_or(PhpMixed::Null);
                    if is_array(&val) || is_array(&existing) {
                        if is_string(&val) {
                            let mut m = IndexMap::new();
                            m.insert("*".into(), val.clone());
                            val = PhpMixed::Array(m);
                        }
                        let existing = self.config.get(key).cloned().unwrap_or(PhpMixed::Null);
                        if is_string(&existing) {
                            let mut m = IndexMap::new();
                            m.insert("*".into(), existing);
                            self.config.insert(key.clone(), PhpMixed::Array(m));
                            self.source_of_config_value
                                .borrow_mut()
                                .insert(format!("{}*", key), source.to_string());
                        }
                        let cur = self.config.get(key).cloned().unwrap_or(PhpMixed::Null);
                        self.config
                            .insert(key.clone(), array_merge(cur, val.clone()));
                        self.set_source_of_config_value(&val, &key_str, source);
                        // the full match pattern needs to be last
                        let has_wildcard = matches!(
                            self.config.get(key),
                            Some(PhpMixed::Array(m)) if m.contains_key(B("*"))
                        );
                        if has_wildcard
                            && let Some(PhpMixed::Array(m)) = self.config.get_mut(key)
                            && let Some(wildcard) = m.shift_remove(B("*"))
                        {
                            m.insert("*".into(), wildcard);
                        }
                    } else {
                        self.config.insert(key.clone(), val.clone());
                        self.set_source_of_config_value(&val, &key_str, source);
                    }
                } else if key == "audit" {
                    let current_ignores = self
                        .config
                        .get(B("audit"))
                        .and_then(|v| v.as_array())
                        .and_then(|m| m.get(B("ignore")))
                        .cloned()
                        .unwrap_or(PhpMixed::List(vec![]));
                    let merged = array_merge(
                        self.config
                            .get(B("audit"))
                            .cloned()
                            .unwrap_or(PhpMixed::Null),
                        val.clone(),
                    );
                    self.config.insert(key.clone(), merged);
                    self.set_source_of_config_value(&val, &key_str, source);
                    let val_ignore = match &val {
                        PhpMixed::Array(m) => m
                            .get(B("ignore"))
                            .cloned()
                            .unwrap_or(PhpMixed::List(vec![])),
                        _ => PhpMixed::List(vec![]),
                    };
                    let new_ignores = array_merge(current_ignores, val_ignore);
                    if let Some(PhpMixed::Array(audit)) = self.config.get_mut(B("audit")) {
                        audit.insert("ignore".into(), new_ignores);
                    }
                } else {
                    self.config.insert(key.clone(), val.clone());
                    self.set_source_of_config_value(&val, &key_str, source);
                }
            }
        }

        let repositories_section = config
            .get(B("repositories"))
            .cloned()
            .unwrap_or(PhpMixed::Null);
        if !empty(&repositories_section) && is_array(&repositories_section) {
            // PHP: array_reverse on IndexMap preserves keys (preserve_keys=true)
            self.repositories = self
                .repositories
                .iter()
                .rev()
                .map(|(k, v)| (k.clone(), v.clone()))
                .collect();
            let new_repos_map: IndexMap<BString, PhpMixed> = match &repositories_section {
                PhpMixed::Array(m) => m.iter().map(|(k, v)| (k.clone(), v.clone())).collect(),
                // A JSON array decodes to a List; in PHP it is an array with integer keys.
                PhpMixed::List(items) => items
                    .iter()
                    .enumerate()
                    .map(|(i, v)| (i.to_string().into(), v.clone()))
                    .collect(),
                _ => IndexMap::new(),
            };
            let new_repos: IndexMap<BString, PhpMixed> = new_repos_map.into_iter().rev().collect();
            for (name, repository) in &new_repos {
                // disable a repository by name
                // this is a code path, that will be used less as the next check will be preferred
                if matches!(repository, PhpMixed::Bool(false)) {
                    // TODO(bytes): repository names are matched as UTF-8 text, so non-UTF-8 bytes
                    // are replaced.
                    self.disable_repo_by_name(&name.to_str_lossy());
                    continue;
                }

                // disable a repository with an anonymous {"name": false} repo
                if is_array(repository)
                    && repository.as_array().map(|m| m.len()).unwrap_or(0) == 1
                    && matches!(
                        repository.as_array().and_then(|m| m.values().next()),
                        Some(PhpMixed::Bool(false))
                    )
                {
                    let name = repository
                        .as_array()
                        .and_then(|m| m.keys().next())
                        .cloned()
                        .unwrap_or_default();
                    // TODO(bytes): repository names are matched as UTF-8 text, so non-UTF-8 bytes
                    // are replaced.
                    self.disable_repo_by_name(&name.to_str_lossy());
                    continue;
                }

                // auto-deactivate the default packagist.org repo if it gets redefined
                let is_composer = repository
                    .as_array()
                    .and_then(|m| m.get(B("type")))
                    .and_then(|v| v.as_string())
                    == Some("composer".into());
                let repo_url = repository
                    .as_array()
                    .and_then(|m| m.get(B("url")))
                    .and_then(|v| v.as_string())
                    .unwrap_or_default()
                    .to_string();
                if is_composer
                    && preg_is_match(
                        php_regex!(r"{^https?://(?:[a-z0-9-.]+\.)?packagist.org(/|$)}"),
                        &repo_url,
                    )
                {
                    self.disable_repo_by_name("packagist.org");
                }

                // store repo
                let is_numeric_name = is_php_integer_key(name);
                if is_numeric_name {
                    if !self.repositories.contains_key(name) {
                        self.repositories.insert(name.clone(), repository.clone());
                    } else {
                        // PHP: $this->repositories[] = $repository
                        // appending to numeric-keyed map
                        let next_idx = self.repositories.len();
                        self.repositories
                            .insert(next_idx.to_string().into(), repository.clone());
                    }
                    let found_key = array_search_mixed(
                        repository,
                        &PhpMixed::Array(self.repositories.clone()),
                        true,
                    )
                    .and_then(|v| v.as_string().map(|s| s.to_string()))
                    .unwrap_or_default();
                    self.set_source_of_config_value(
                        repository,
                        &format!("repositories.{}", found_key),
                        source,
                    );
                } else if name == "packagist" {
                    // BC support for default "packagist" named repo
                    self.repositories
                        .insert(format!("{}.org", name).into(), repository.clone());
                    self.set_source_of_config_value(
                        repository,
                        &format!("repositories.{}.org", name),
                        source,
                    );
                } else {
                    self.repositories.insert(name.clone(), repository.clone());
                    self.set_source_of_config_value(
                        repository,
                        &format!("repositories.{}", name),
                        source,
                    );
                }
            }
            self.repositories = self
                .repositories
                .iter()
                .rev()
                .map(|(k, v)| (k.clone(), v.clone()))
                .collect();
        }
    }

    pub fn get_repositories(&self) -> IndexMap<BString, PhpMixed> {
        self.repositories.clone()
    }

    /// Returns a setting
    pub fn get(&self, key: &str) -> PhpMixed {
        self.get_with_flags(key, 0).unwrap_or(PhpMixed::Null)
    }

    /// Typed convenience accessor for keys whose `get()` value is always a string.
    pub fn get_str(&self, key: &str) -> anyhow::Result<String> {
        Ok(self
            .get_with_flags(key, 0)?
            .as_string()
            .unwrap_or_default()
            .to_string())
    }

    pub fn get_with_flags(&self, key: &str, flags: i64) -> anyhow::Result<PhpMixed> {
        match key {
            // strings/paths with env var and {$refs} support
            "vendor-dir" | "bin-dir" | "process-timeout" | "data-dir" | "cache-dir"
            | "cache-files-dir" | "cache-repo-dir" | "cache-vcs-dir" | "cafile" | "capath" => {
                // convert foo-bar to COMPOSER_FOO_BAR and check if it exists since it overrides the local config
                let env = format!("COMPOSER_{}", strtoupper(strtr(key, "-", "_")));

                let val = self.get_composer_env(&env);
                if !matches!(val, PhpMixed::Bool(false)) {
                    self.set_source_of_config_value(&val, key, &env);
                }

                if key == "process-timeout" {
                    let raw = if matches!(val, PhpMixed::Bool(false)) {
                        self.config.get(B(key)).cloned().unwrap_or(PhpMixed::Null)
                    } else {
                        val
                    };
                    return Ok(PhpMixed::Int(intval(&raw).max(0)));
                }

                let raw_val = if matches!(val, PhpMixed::Bool(false)) {
                    self.config.get(B(key)).cloned().unwrap_or(PhpMixed::Null)
                } else {
                    val
                };
                let processed = self.process(raw_val, flags)?;
                let mut val_str =
                    legacy_lossy(&rtrim2(php_to_string(&processed), "/\\")).into_owned();
                val_str = legacy_lossy(&bstr::BString::from(Platform::expand_path(&val_str)))
                    .into_owned();

                if substr(key, -4, None) != "-dir" {
                    return Ok(PhpMixed::string(val_str));
                }

                Ok(PhpMixed::string(
                    if (flags & Self::RELATIVE_PATHS) == Self::RELATIVE_PATHS {
                        val_str
                    } else {
                        legacy_lossy(&bstr::BString::from(self.realpath(&val_str))).into_owned()
                    },
                ))
            }

            // booleans with env var support
            "cache-read-only" | "htaccess-protect" => {
                // convert foo-bar to COMPOSER_FOO_BAR and check if it exists since it overrides the local config
                let env = format!("COMPOSER_{}", strtoupper(strtr(key, "-", "_")));

                let val = self.get_composer_env(&env);
                let val = if matches!(val, PhpMixed::Bool(false)) {
                    self.config.get(B(key)).cloned().unwrap_or(PhpMixed::Null)
                } else {
                    self.set_source_of_config_value(&val, key, &env);
                    val
                };

                Ok(PhpMixed::Bool(
                    val.as_string() != Some("false".into()) && val.to_bool(),
                ))
            }

            // booleans without env var support
            "disable-tls" | "secure-http" | "use-github-api" | "lock" => {
                // special case for secure-http
                if key == "secure-http"
                    && self.get_with_flags("disable-tls", 0)?.as_bool() == Some(true)
                {
                    return Ok(PhpMixed::Bool(false));
                }

                let v = self.config.get(B(key)).cloned().unwrap_or(PhpMixed::Null);
                Ok(PhpMixed::Bool(
                    v.as_string() != Some("false".into()) && v.to_bool(),
                ))
            }

            // ints without env var support
            "cache-ttl" => Ok(PhpMixed::Int(
                self.config.get(B(key)).map(intval).unwrap_or(0).max(0),
            )),

            // numbers with kb/mb/gb support, without env var support
            "cache-files-maxsize" => {
                let raw = self
                    .config
                    .get(B(key))
                    .map(php_to_string)
                    .unwrap_or_default();
                let Some(matches) = preg_match(
                    php_regex!(r"/^\s*([0-9.]+)\s*(?:([kmg])(?:i?b)?)?\s*$/i"),
                    &raw,
                ) else {
                    return Err(RuntimeException::new(format!(
                        "Could not parse the value of '{}': {}",
                        key, raw
                    ))
                    .into());
                };
                let mut size = matches
                    .get(1)
                    .unwrap_or_default()
                    .to_string()
                    .parse::<f64>()
                    .unwrap_or(0.0);
                let unit = matches.get(2).map(|s| legacy_lossy(s).into_owned());
                if let Some(unit) = unit {
                    match strtolower(&unit).as_slice() {
                        b"g" => {
                            size *= 1024.0;
                            size *= 1024.0;
                            size *= 1024.0;
                        }
                        b"m" => {
                            size *= 1024.0;
                            size *= 1024.0;
                        }
                        b"k" => {
                            size *= 1024.0;
                        }
                        _ => {}
                    }
                }

                Ok(PhpMixed::Int((size as i64).max(0)))
            }

            // special cases below
            "cache-files-ttl" => {
                let v = self.config.get(B(key)).cloned();
                if let Some(v) = v
                    && !v.is_null()
                {
                    return Ok(PhpMixed::Int(intval(&v).max(0)));
                }

                self.get_with_flags("cache-ttl", 0)
            }

            "home" => {
                let v = self.config.get(B(key)).cloned().unwrap_or(PhpMixed::Null);
                let expanded =
                    Platform::expand_path(&v.as_string().unwrap_or_default().to_str_lossy());
                let processed = self.process(PhpMixed::string(expanded), flags)?;
                Ok(PhpMixed::string(rtrim2(
                    processed.as_string().unwrap_or_default(),
                    "/\\",
                )))
            }

            "bin-compat" => {
                let env_val = self.get_composer_env("COMPOSER_BIN_COMPAT");
                let value = match env_val {
                    PhpMixed::Bool(false) | PhpMixed::Null => self
                        .config
                        .get(B(key))
                        .and_then(|v| v.as_string())
                        .unwrap_or_default()
                        .to_string(),
                    other => other.as_string().unwrap_or_default().to_string(),
                };

                if !in_array_loose(
                    value.clone(),
                    &[
                        PhpMixed::string("auto".to_string()),
                        PhpMixed::string("full".to_string()),
                        PhpMixed::string("proxy".to_string()),
                        PhpMixed::string("symlink".to_string()),
                    ],
                ) {
                    return Err(RuntimeException::new(format!(
                        "Invalid value for 'bin-compat': {}. Expected auto, full or proxy",
                        value
                    ))
                    .into());
                }

                if value == "symlink" {
                    trigger_error(
                        "config.bin-compat \"symlink\" is deprecated since Composer 2.2, use auto, full (for Windows compatibility) or proxy instead.",
                        E_USER_DEPRECATED,
                    );
                }

                Ok(PhpMixed::string(value))
            }

            "discard-changes" => {
                let env = self.get_composer_env("COMPOSER_DISCARD_CHANGES");
                if !matches!(env, PhpMixed::Bool(false)) {
                    let env_str = env.as_string().unwrap_or_default().to_string();
                    if !matches!(env_str.as_str(), "stash" | "true" | "false" | "1" | "0") {
                        return Err(RuntimeException::new(format!(
                            "Invalid value for COMPOSER_DISCARD_CHANGES: {}. Expected 1, 0, true, false or stash",
                            env_str
                        ))
                        .into());
                    }
                    if env_str == "stash" {
                        return Ok(PhpMixed::string("stash".to_string()));
                    }

                    // convert string value to bool
                    return Ok(PhpMixed::Bool(
                        env_str != "false" && !env_str.is_empty() && env_str != "0",
                    ));
                }

                let val = self.config.get(B(key)).cloned().unwrap_or(PhpMixed::Null);
                let allowed =
                    matches!(&val, PhpMixed::Bool(_)) || val.as_string() == Some("stash".into());
                if !allowed {
                    return Err(RuntimeException::new(format!(
                        "Invalid value for 'discard-changes': {:?}. Expected true, false or stash",
                        val
                    ))
                    .into());
                }

                Ok(val)
            }

            "github-protocols" => {
                let mut protos: Vec<String> = self
                    .config
                    .get(B("github-protocols"))
                    .and_then(|v| v.as_list())
                    .map(|l| {
                        l.iter()
                            .filter_map(|v| v.as_string().map(|s| s.to_string()))
                            .collect()
                    })
                    .unwrap_or_default();
                let secure_http = self
                    .config
                    .get(B("secure-http"))
                    .and_then(|v| v.as_bool())
                    .unwrap_or(false);
                if secure_http {
                    let found = array_search_mixed(
                        &PhpMixed::string("git".to_string()),
                        &PhpMixed::List(protos.iter().cloned().map(PhpMixed::string).collect()),
                        false,
                    );
                    if let Some(idx_val) = found {
                        let idx = idx_val.as_int().expect("list search index") as usize;
                        protos.remove(idx);
                    }
                }
                let first = protos.first().cloned();
                if first.as_deref() == Some("http") {
                    return Err(RuntimeException::new("The http protocol for github is not available anymore, update your config's github-protocols to use \"https\", \"git\" or \"ssh\"".to_string())
                    .into());
                }

                Ok(PhpMixed::List(
                    protos.into_iter().map(PhpMixed::string).collect(),
                ))
            }

            "autoloader-suffix" => {
                let v = self.config.get(B(key)).cloned().unwrap_or(PhpMixed::Null);
                if v.as_string() == Some("".into()) {
                    // we need to guarantee null or non-empty-string
                    return Ok(PhpMixed::Null);
                }

                self.process(v, flags)
            }

            "audit" => {
                let mut result = self.config.get(B(key)).cloned().unwrap_or(PhpMixed::Null);
                let abandoned_env = self.get_composer_env("COMPOSER_AUDIT_ABANDONED");
                if !matches!(abandoned_env, PhpMixed::Bool(false)) {
                    let abandoned_env_str =
                        abandoned_env.as_string().unwrap_or_default().to_string();
                    let valid_choices: Vec<String> =
                        Auditor::ABANDONEDS.iter().map(|s| s.to_string()).collect();
                    if !in_array_strict(
                        abandoned_env_str.clone(),
                        &valid_choices
                            .iter()
                            .map(|s| PhpMixed::string(s.clone()))
                            .collect::<Vec<_>>(),
                    ) {
                        return Err(RuntimeException::new(format!(
                            "Invalid value for COMPOSER_AUDIT_ABANDONED: {}. Expected one of {}.",
                            abandoned_env_str,
                            implode(", ", &valid_choices),
                        ))
                        .into());
                    }
                    if let PhpMixed::Array(ref mut m) = result {
                        m.insert("abandoned".into(), PhpMixed::string(abandoned_env_str));
                    }
                }

                let block_abandoned_env =
                    self.get_composer_env("COMPOSER_SECURITY_BLOCKING_ABANDONED");
                if !matches!(block_abandoned_env, PhpMixed::Bool(false)) {
                    let env_str = block_abandoned_env
                        .as_string()
                        .unwrap_or_default()
                        .to_string();
                    if !matches!(env_str.as_str(), "0" | "1") {
                        return Err(RuntimeException::new(format!(
                            "Invalid value for COMPOSER_SECURITY_BLOCKING_ABANDONED: {}. Expected 0 or 1.",
                            env_str
                        ))
                        .into());
                    }
                    if let PhpMixed::Array(ref mut m) = result {
                        m.insert("block-abandoned".into(), PhpMixed::Bool(env_str == "1"));
                    }
                }

                Ok(result)
            }

            _ => {
                if !self.config.contains_key(B(key)) {
                    return Ok(PhpMixed::Null);
                }

                let v = self.config.get(B(key)).cloned().unwrap_or(PhpMixed::Null);
                self.process(v, flags)
            }
        }
    }

    pub fn all(&mut self, flags: i64) -> anyhow::Result<IndexMap<BString, PhpMixed>> {
        let mut all: IndexMap<BString, PhpMixed> = IndexMap::new();
        all.insert(
            "repositories".into(),
            PhpMixed::Array(self.get_repositories()),
        );
        let keys: Vec<BString> = self.config.keys().cloned().collect();
        let mut config_section: IndexMap<BString, PhpMixed> = IndexMap::new();
        for key in keys {
            // TODO(bytes): get_with_flags takes UTF-8 text, so non-UTF-8 bytes are replaced.
            let value = self.get_with_flags(&key.to_str_lossy(), flags)?;
            config_section.insert(key, value);
        }
        all.insert("config".into(), PhpMixed::Array(config_section));

        Ok(all)
    }

    pub fn get_source_of_value(&mut self, key: &str) -> String {
        let _ = self.get(key);

        self.source_of_config_value
            .borrow()
            .get(key)
            .cloned()
            .unwrap_or_else(|| Self::SOURCE_UNKNOWN.to_string())
    }

    fn set_source_of_config_value(&self, config_value: &PhpMixed, path: &str, source: &str) {
        self.source_of_config_value
            .borrow_mut()
            .insert(path.to_string(), source.to_string());

        if is_array(config_value) {
            let map = match config_value {
                PhpMixed::Array(m) => m
                    .iter()
                    .map(|(k, v)| (k.clone(), v.clone()))
                    .collect::<Vec<_>>(),
                _ => vec![],
            };
            for (key, value) in map {
                self.set_source_of_config_value(&value, &format!("{}.{}", path, key), source);
            }
        }
    }

    pub fn raw(&self) -> IndexMap<BString, PhpMixed> {
        let mut result: IndexMap<BString, PhpMixed> = IndexMap::new();
        result.insert(
            "repositories".into(),
            PhpMixed::Array(self.get_repositories()),
        );
        result.insert("config".into(), PhpMixed::Array(self.config.clone()));
        result
    }

    /// Checks whether a setting exists
    pub fn has(&self, key: &str) -> bool {
        array_key_exists(B(key), &self.config)
    }

    /// Replaces {$refs} inside a config string
    fn process(&self, value: PhpMixed, flags: i64) -> anyhow::Result<PhpMixed> {
        if !is_string(&value) {
            return Ok(value);
        }

        let value_str = value.as_string().unwrap_or_default().to_string();
        let result = preg_replace_callback(
            php_regex!(r"#\{\$(.+)\}#"),
            |m: &PregMatches| -> anyhow::Result<String> {
                let key_match = m.get(1).unwrap_or_default().to_string();
                Ok(
                    legacy_lossy(&php_to_string(&self.get_with_flags(&key_match, flags)?))
                        .into_owned(),
                )
            },
            &value_str,
        )?;
        Ok(PhpMixed::string(result))
    }

    /// Turns relative paths in absolute paths without realpath()
    ///
    /// Since the dirs might not exist yet we can not call realpath or it will fail.
    fn realpath(&self, path: &str) -> String {
        if preg_is_match(php_regex!(r"{^(?:/|[a-z]:|[a-z0-9.]+://|\\\\\\\\)}i"), path) {
            return path.to_string();
        }

        match &self.base_dir {
            Some(base) => format!("{}/{}", base, path),
            None => path.to_string(),
        }
    }

    /// Reads the value of a Composer environment variable
    ///
    /// This should be used to read COMPOSER_ environment variables
    /// that overload config values.
    fn get_composer_env(&self, var: &str) -> PhpMixed {
        if self.use_environment {
            return match Platform::get_env(var) {
                Some(v) => PhpMixed::string(v),
                None => PhpMixed::Bool(false),
            };
        }

        PhpMixed::Bool(false)
    }

    fn disable_repo_by_name(&mut self, name: &str) {
        if self.repositories.contains_key(B(name)) {
            self.repositories.shift_remove(B(name));
        } else if name == "packagist" {
            // BC support for default "packagist" named repo
            self.repositories.shift_remove(B("packagist.org"));
        }
    }

    /// Validates that the passed URL is allowed to be used by current config, or throws an exception.
    pub fn prohibit_url_by_config(
        &mut self,
        url: &str,
        io: Option<std::rc::Rc<std::cell::RefCell<dyn IOInterface>>>,
        repo_options: &IndexMap<BString, PhpMixed>,
    ) -> anyhow::Result<()> {
        // Return right away if the URL is malformed or custom (see issue #5173), but only for non-HTTP(S) URLs
        if !filter_var_url(url) && !preg_is_match(php_regex!(r"{^https?://}"), url) {
            return Ok(());
        }

        // Extract scheme and throw exception on known insecure protocols
        let parsed = parse_url(url);
        let scheme = parsed.as_ref().and_then(|parsed| parsed.scheme.clone());
        let hostname = parsed.and_then(|parsed| parsed.host);
        if matches!(scheme.as_deref(), Some("http" | "git" | "ftp" | "svn")) {
            if self.get_with_flags("secure-http", 0)?.as_bool() == Some(true) {
                if scheme.as_deref() == Some("svn") {
                    if in_array_strict(
                        hostname.map(PhpMixed::string).unwrap_or(PhpMixed::Null),
                        self.get_with_flags("secure-svn-domains", 0)?.values(),
                    ) {
                        return Ok(());
                    }

                    return Err(TransportException::new(
                        format!(
                            "Your configuration does not allow connections to {}. See https://getcomposer.org/doc/06-config.md#secure-svn-domains for details.",
                            url
                        ),
                        0,
                    )
                    .into());
                }

                return Err(TransportException::new(
                    format!(
                        "Your configuration does not allow connections to {}. See https://getcomposer.org/doc/06-config.md#secure-http for details.",
                        url
                    ),
                    0,
                )
                .into());
            }
            if let Some(ref io) = io
                && let Some(ref hostname) = hostname
            {
                if !self.warned_hosts.contains_key(hostname) {
                    io.write_error3(
                            &format!(
                                "<warning>Warning: Accessing {} over {} which is an insecure protocol.</warning>",
                                hostname,
                                scheme.as_deref().unwrap_or_default()
                            ),
                            true,
                            io_interface::NORMAL,
                        );
                }
                self.warned_hosts.insert(hostname.clone(), true);
            }
        }

        if let Some(ref io) = io
            && let Some(ref hostname) = hostname
            && !self.ssl_verify_warned_hosts.contains_key(hostname)
        {
            let mut warning: Option<String> = None;
            let verify_peer = repo_options
                .get(B("ssl"))
                .and_then(|v| v.as_array())
                .and_then(|m| m.get(B("verify_peer")));
            if let Some(v) = verify_peer.and_then(|v| v.as_opt())
                && !v.to_bool()
            {
                warning = Some("verify_peer".to_string());
            }

            let verify_peer_name = repo_options
                .get(B("ssl"))
                .and_then(|v| v.as_array())
                .and_then(|m| m.get(B("verify_peer_name")));
            if let Some(v) = verify_peer_name.and_then(|v| v.as_opt())
                && !v.to_bool()
            {
                warning = match warning {
                    None => Some("verify_peer_name".to_string()),
                    Some(w) => Some(format!("{} and verify_peer_name", w)),
                };
            }

            if let Some(w) = warning {
                io.write_error3(
                    &format!(
                        "<warning>Warning: Accessing {} with {} disabled.</warning>",
                        hostname, w
                    ),
                    true,
                    io_interface::NORMAL,
                );
                self.ssl_verify_warned_hosts.insert(hostname.clone(), true);
            }
        }

        Ok(())
    }

    /// Used by long-running custom scripts in composer.json
    pub fn disable_process_timeout() {
        // Override global timeout set earlier by environment or config
        ProcessExecutor::set_timeout(0);
    }
}

/// Whether a string array key would have been coerced to an integer key by PHP.
///
/// PHP stores a string array key as an integer when it is a canonical decimal
/// representation: "0", or an optionally negative sequence of digits with no
/// leading zero and no leading plus sign, that fits in a platform integer.
pub(crate) fn is_php_integer_key(key: &[u8]) -> bool {
    canonical_int_key(key).is_some()
}
