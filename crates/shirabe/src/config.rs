//! ref: composer/src/Composer/Config.php

pub mod config_source_interface;
pub mod json_config_source;

use crate::io::io_interface;
use anyhow::Result;
use indexmap::IndexMap;
use shirabe_external_packages::composer::pcre::preg::Preg;
use shirabe_php_shim::{
    E_USER_DEPRECATED, FILTER_VALIDATE_URL, PHP_URL_HOST, PHP_URL_SCHEME, PhpMixed,
    RuntimeException, array_key_exists, array_merge_recursive, array_reverse, array_search_mixed,
    array_unique, current, empty, filter_var, implode, in_array, is_array, is_int, is_string, key,
    max, parse_url, reset, rtrim, strtolower, strtoupper, strtr, substr, trigger_error,
};

use crate::advisory::auditor::Auditor;
use crate::config::config_source_interface::ConfigSourceInterface;
use crate::downloader::transport_exception::TransportException;
use crate::io::io_interface::IOInterface;
use crate::util::platform::Platform;
use crate::util::process_executor::ProcessExecutor;

#[derive(Debug)]
pub struct Config {
    /// @var array<string, mixed>
    config: IndexMap<String, PhpMixed>,
    /// @var ?non-empty-string
    base_dir: Option<String>,
    /// @var array<int|string, mixed>
    repositories: IndexMap<String, PhpMixed>,
    config_source: Option<Box<dyn ConfigSourceInterface>>,
    auth_config_source: Option<Box<dyn ConfigSourceInterface>>,
    local_auth_config_source: Option<Box<dyn ConfigSourceInterface>>,
    use_environment: bool,
    /// @var array<string, true>
    warned_hosts: IndexMap<String, bool>,
    /// @var array<string, true>
    ssl_verify_warned_hosts: IndexMap<String, bool>,
    /// @var array<string, string>
    source_of_config_value: IndexMap<String, String>,
}

impl Config {
    pub const SOURCE_DEFAULT: &'static str = "default";
    pub const SOURCE_COMMAND: &'static str = "command";
    pub const SOURCE_UNKNOWN: &'static str = "unknown";

    pub const RELATIVE_PATHS: i64 = 1;

    /// @var array<string, mixed>
    pub fn default_config() -> IndexMap<String, PhpMixed> {
        let mut c: IndexMap<String, PhpMixed> = IndexMap::new();
        c.insert("process-timeout".to_string(), PhpMixed::Int(300));
        c.insert("use-include-path".to_string(), PhpMixed::Bool(false));
        c.insert(
            "allow-plugins".to_string(),
            PhpMixed::Array(IndexMap::new()),
        );
        c.insert(
            "use-parent-dir".to_string(),
            PhpMixed::String("prompt".to_string()),
        );
        c.insert(
            "preferred-install".to_string(),
            PhpMixed::String("dist".to_string()),
        );
        let mut audit: IndexMap<String, Box<PhpMixed>> = IndexMap::new();
        audit.insert(
            "ignore".to_string(),
            Box::new(PhpMixed::Array(IndexMap::new())),
        );
        audit.insert(
            "abandoned".to_string(),
            Box::new(PhpMixed::String(Auditor::ABANDONED_FAIL.to_string())),
        );
        c.insert("audit".to_string(), PhpMixed::Array(audit));
        c.insert("notify-on-install".to_string(), PhpMixed::Bool(true));
        c.insert(
            "github-protocols".to_string(),
            PhpMixed::List(vec![
                Box::new(PhpMixed::String("https".to_string())),
                Box::new(PhpMixed::String("ssh".to_string())),
                Box::new(PhpMixed::String("git".to_string())),
            ]),
        );
        c.insert("gitlab-protocol".to_string(), PhpMixed::Null);
        c.insert(
            "vendor-dir".to_string(),
            PhpMixed::String("vendor".to_string()),
        );
        c.insert(
            "bin-dir".to_string(),
            PhpMixed::String("{$vendor-dir}/bin".to_string()),
        );
        c.insert(
            "cache-dir".to_string(),
            PhpMixed::String("{$home}/cache".to_string()),
        );
        c.insert(
            "data-dir".to_string(),
            PhpMixed::String("{$home}".to_string()),
        );
        c.insert(
            "cache-files-dir".to_string(),
            PhpMixed::String("{$cache-dir}/files".to_string()),
        );
        c.insert(
            "cache-repo-dir".to_string(),
            PhpMixed::String("{$cache-dir}/repo".to_string()),
        );
        c.insert(
            "cache-vcs-dir".to_string(),
            PhpMixed::String("{$cache-dir}/vcs".to_string()),
        );
        c.insert("cache-ttl".to_string(), PhpMixed::Int(15552000)); // 6 months
        c.insert("cache-files-ttl".to_string(), PhpMixed::Null); // fallback to cache-ttl
        c.insert(
            "cache-files-maxsize".to_string(),
            PhpMixed::String("300MiB".to_string()),
        );
        c.insert("cache-read-only".to_string(), PhpMixed::Bool(false));
        c.insert(
            "bin-compat".to_string(),
            PhpMixed::String("auto".to_string()),
        );
        c.insert("discard-changes".to_string(), PhpMixed::Bool(false));
        c.insert("autoloader-suffix".to_string(), PhpMixed::Null);
        c.insert("sort-packages".to_string(), PhpMixed::Bool(false));
        c.insert("optimize-autoloader".to_string(), PhpMixed::Bool(false));
        c.insert("classmap-authoritative".to_string(), PhpMixed::Bool(false));
        c.insert("apcu-autoloader".to_string(), PhpMixed::Bool(false));
        c.insert("prepend-autoloader".to_string(), PhpMixed::Bool(true));
        c.insert(
            "update-with-minimal-changes".to_string(),
            PhpMixed::Bool(false),
        );
        c.insert(
            "github-domains".to_string(),
            PhpMixed::List(vec![Box::new(PhpMixed::String("github.com".to_string()))]),
        );
        c.insert(
            "bitbucket-expose-hostname".to_string(),
            PhpMixed::Bool(true),
        );
        c.insert("disable-tls".to_string(), PhpMixed::Bool(false));
        c.insert("secure-http".to_string(), PhpMixed::Bool(true));
        c.insert("secure-svn-domains".to_string(), PhpMixed::List(vec![]));
        c.insert("cafile".to_string(), PhpMixed::Null);
        c.insert("capath".to_string(), PhpMixed::Null);
        c.insert("github-expose-hostname".to_string(), PhpMixed::Bool(true));
        c.insert(
            "gitlab-domains".to_string(),
            PhpMixed::List(vec![Box::new(PhpMixed::String("gitlab.com".to_string()))]),
        );
        c.insert(
            "store-auths".to_string(),
            PhpMixed::String("prompt".to_string()),
        );
        c.insert("platform".to_string(), PhpMixed::Array(IndexMap::new()));
        c.insert(
            "archive-format".to_string(),
            PhpMixed::String("tar".to_string()),
        );
        c.insert("archive-dir".to_string(), PhpMixed::String(".".to_string()));
        c.insert("htaccess-protect".to_string(), PhpMixed::Bool(true));
        c.insert("use-github-api".to_string(), PhpMixed::Bool(true));
        c.insert("lock".to_string(), PhpMixed::Bool(true));
        c.insert(
            "platform-check".to_string(),
            PhpMixed::String("php-only".to_string()),
        );
        c.insert(
            "bitbucket-oauth".to_string(),
            PhpMixed::Array(IndexMap::new()),
        );
        c.insert("github-oauth".to_string(), PhpMixed::Array(IndexMap::new()));
        c.insert("gitlab-oauth".to_string(), PhpMixed::Array(IndexMap::new()));
        c.insert("gitlab-token".to_string(), PhpMixed::Array(IndexMap::new()));
        c.insert("http-basic".to_string(), PhpMixed::Array(IndexMap::new()));
        c.insert("bearer".to_string(), PhpMixed::Array(IndexMap::new()));
        c.insert(
            "custom-headers".to_string(),
            PhpMixed::Array(IndexMap::new()),
        );
        c.insert("bump-after-update".to_string(), PhpMixed::Bool(false));
        c.insert(
            "allow-missing-requirements".to_string(),
            PhpMixed::Bool(false),
        );
        c.insert(
            "client-certificate".to_string(),
            PhpMixed::Array(IndexMap::new()),
        );
        c.insert(
            "forgejo-domains".to_string(),
            PhpMixed::List(vec![Box::new(PhpMixed::String("codeberg.org".to_string()))]),
        );
        c.insert(
            "forgejo-token".to_string(),
            PhpMixed::Array(IndexMap::new()),
        );
        c
    }

    /// @var array<string, mixed>
    pub fn default_repositories() -> IndexMap<String, PhpMixed> {
        let mut r: IndexMap<String, PhpMixed> = IndexMap::new();
        let mut packagist: IndexMap<String, Box<PhpMixed>> = IndexMap::new();
        packagist.insert(
            "type".to_string(),
            Box::new(PhpMixed::String("composer".to_string())),
        );
        packagist.insert(
            "url".to_string(),
            Box::new(PhpMixed::String("https://repo.packagist.org".to_string())),
        );
        r.insert("packagist.org".to_string(), PhpMixed::Array(packagist));
        r
    }

    /// @param bool    $useEnvironment Use COMPOSER_ environment variables to replace config settings
    /// @param ?string $baseDir        Optional base directory of the config
    pub fn new(use_environment: bool, base_dir: Option<String>) -> Self {
        let mut this = Self {
            // load defaults
            config: Self::default_config(),
            repositories: Self::default_repositories(),
            use_environment,
            base_dir: base_dir.filter(|s| is_string(&PhpMixed::String(s.clone())) && !s.is_empty()),
            config_source: None,
            auth_config_source: None,
            local_auth_config_source: None,
            warned_hosts: IndexMap::new(),
            ssl_verify_warned_hosts: IndexMap::new(),
            source_of_config_value: IndexMap::new(),
        };

        let config_clone = this.config.clone();
        for (config_key, config_value) in &config_clone {
            this.set_source_of_config_value(config_value, config_key, Self::SOURCE_DEFAULT);
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

    pub fn set_auth_config_source(&mut self, source: Box<dyn ConfigSourceInterface>) {
        self.auth_config_source = Some(source);
    }

    pub fn get_auth_config_source(&self) -> &dyn ConfigSourceInterface {
        self.auth_config_source.as_ref().unwrap().as_ref()
    }

    pub fn set_local_auth_config_source(&mut self, source: Box<dyn ConfigSourceInterface>) {
        self.local_auth_config_source = Some(source);
    }

    pub fn get_local_auth_config_source(&self) -> Option<&dyn ConfigSourceInterface> {
        self.local_auth_config_source.as_deref()
    }

    /// Merges new config values with the existing ones (overriding)
    ///
    /// @param array{config?: array<string, mixed>, repositories?: array<mixed>} $config
    pub fn merge(&mut self, config: &IndexMap<String, PhpMixed>, source: &str) {
        // override defaults with given config
        let config_section = config.get("config").cloned().unwrap_or(PhpMixed::Null);
        if !empty(&config_section) && is_array(config_section.clone()) {
            let config_section_map = match config_section {
                PhpMixed::Array(m) => m,
                _ => IndexMap::new(),
            };
            for (key, val_box) in &config_section_map {
                let val = (**val_box).clone();
                if in_array(
                    PhpMixed::String(key.clone()),
                    &PhpMixed::List(vec![
                        Box::new(PhpMixed::String("bitbucket-oauth".to_string())),
                        Box::new(PhpMixed::String("github-oauth".to_string())),
                        Box::new(PhpMixed::String("gitlab-oauth".to_string())),
                        Box::new(PhpMixed::String("gitlab-token".to_string())),
                        Box::new(PhpMixed::String("http-basic".to_string())),
                        Box::new(PhpMixed::String("bearer".to_string())),
                        Box::new(PhpMixed::String("client-certificate".to_string())),
                        Box::new(PhpMixed::String("forgejo-token".to_string())),
                    ]),
                    true,
                ) && self.config.contains_key(key)
                {
                    let existing = self.config.get(key).cloned().unwrap_or(PhpMixed::Null);
                    self.config.insert(
                        key.clone(),
                        array_merge_recursive(vec![existing, val.clone()]),
                    );
                    self.set_source_of_config_value(&val, key, source);
                } else if in_array(
                    PhpMixed::String(key.clone()),
                    &PhpMixed::List(vec![Box::new(PhpMixed::String(
                        "allow-plugins".to_string(),
                    ))]),
                    true,
                ) && self.config.contains_key(key)
                    && is_array(self.config.get(key).cloned().unwrap_or(PhpMixed::Null))
                    && is_array(val.clone())
                {
                    // merging $val first to get the local config on top of the global one, then appending the global config,
                    // then merging local one again to make sure the values from local win over global ones for keys present in both
                    let existing = self.config.get(key).cloned().unwrap_or(PhpMixed::Null);
                    self.config.insert(
                        key.clone(),
                        array_merge_recursive(vec![val.clone(), existing, val.clone()]),
                    );
                    self.set_source_of_config_value(&val, key, source);
                } else if in_array(
                    PhpMixed::String(key.clone()),
                    &PhpMixed::List(vec![
                        Box::new(PhpMixed::String("gitlab-domains".to_string())),
                        Box::new(PhpMixed::String("github-domains".to_string())),
                    ]),
                    true,
                ) && self.config.contains_key(key)
                {
                    let existing = self.config.get(key).cloned().unwrap_or(PhpMixed::Null);
                    let merged = array_merge_recursive(vec![existing, val.clone()]);
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
                        PhpMixed::List(
                            deduped
                                .into_iter()
                                .map(|s| Box::new(PhpMixed::String(s)))
                                .collect(),
                        ),
                    );
                    self.set_source_of_config_value(&val, key, source);
                } else if key == "preferred-install" && self.config.contains_key(key) {
                    let mut val = val.clone();
                    let existing = self.config.get(key).cloned().unwrap_or(PhpMixed::Null);
                    if is_array(val.clone()) || is_array(existing.clone()) {
                        if is_string(&val) {
                            let mut m = IndexMap::new();
                            m.insert("*".to_string(), Box::new(val.clone()));
                            val = PhpMixed::Array(m);
                        }
                        let existing = self.config.get(key).cloned().unwrap_or(PhpMixed::Null);
                        if is_string(&existing) {
                            let mut m = IndexMap::new();
                            m.insert("*".to_string(), Box::new(existing));
                            self.config.insert(key.clone(), PhpMixed::Array(m));
                            self.source_of_config_value
                                .insert(format!("{}*", key), source.to_string());
                        }
                        let cur = self.config.get(key).cloned().unwrap_or(PhpMixed::Null);
                        self.config
                            .insert(key.clone(), array_merge_recursive(vec![cur, val.clone()]));
                        self.set_source_of_config_value(&val, key, source);
                        // the full match pattern needs to be last
                        let has_wildcard = matches!(
                            self.config.get(key),
                            Some(PhpMixed::Array(m)) if m.contains_key("*")
                        );
                        if has_wildcard {
                            if let Some(PhpMixed::Array(m)) = self.config.get_mut(key) {
                                if let Some(wildcard) = m.shift_remove("*") {
                                    m.insert("*".to_string(), wildcard);
                                }
                            }
                        }
                    } else {
                        self.config.insert(key.clone(), val.clone());
                        self.set_source_of_config_value(&val, key, source);
                    }
                } else if key == "audit" {
                    let current_ignores = self
                        .config
                        .get("audit")
                        .and_then(|v| v.as_array())
                        .and_then(|m| m.get("ignore"))
                        .cloned()
                        .map(|b| *b)
                        .unwrap_or(PhpMixed::List(vec![]));
                    let merged = array_merge_recursive(vec![
                        self.config.get("audit").cloned().unwrap_or(PhpMixed::Null),
                        val.clone(),
                    ]);
                    self.config.insert(key.clone(), merged);
                    self.set_source_of_config_value(&val, key, source);
                    let val_ignore = match &val {
                        PhpMixed::Array(m) => m
                            .get("ignore")
                            .cloned()
                            .map(|b| *b)
                            .unwrap_or(PhpMixed::List(vec![])),
                        _ => PhpMixed::List(vec![]),
                    };
                    let new_ignores = array_merge_recursive(vec![current_ignores, val_ignore]);
                    if let Some(PhpMixed::Array(audit)) = self.config.get_mut("audit") {
                        audit.insert("ignore".to_string(), Box::new(new_ignores));
                    }
                } else {
                    self.config.insert(key.clone(), val.clone());
                    self.set_source_of_config_value(&val, key, source);
                }
            }
        }

        let repositories_section = config
            .get("repositories")
            .cloned()
            .unwrap_or(PhpMixed::Null);
        if !empty(&repositories_section) && is_array(repositories_section.clone()) {
            self.repositories = array_reverse(&self.repositories, true);
            let new_repos_map = match &repositories_section {
                PhpMixed::Array(m) => m.iter().map(|(k, v)| (k.clone(), (**v).clone())).collect(),
                _ => IndexMap::new(),
            };
            let new_repos = array_reverse(&new_repos_map, true);
            for (name, repository) in &new_repos {
                // disable a repository by name
                // this is a code path, that will be used less as the next check will be preferred
                if matches!(repository, PhpMixed::Bool(false)) {
                    self.disable_repo_by_name(&name.to_string());
                    continue;
                }

                // disable a repository with an anonymous {"name": false} repo
                if is_array(repository.clone())
                    && repository.as_array().map(|m| m.len()).unwrap_or(0) == 1
                    && matches!(current(repository.clone()), PhpMixed::Bool(false))
                {
                    self.disable_repo_by_name(&key(repository.clone()).unwrap_or_default());
                    continue;
                }

                // auto-deactivate the default packagist.org repo if it gets redefined
                let is_composer = repository
                    .as_array()
                    .and_then(|m| m.get("type"))
                    .and_then(|v| v.as_string())
                    == Some("composer");
                let repo_url = repository
                    .as_array()
                    .and_then(|m| m.get("url"))
                    .and_then(|v| v.as_string())
                    .unwrap_or("")
                    .to_string();
                if is_composer
                    && Preg::is_match(
                        r"{^https?://(?:[a-z0-9-.]+\.)?packagist.org(/|$)}",
                        &repo_url,
                    )
                    .unwrap_or(false)
                {
                    self.disable_repo_by_name("packagist.org");
                }

                // store repo
                // TODO(phase-b): is_int($name) where $name is an IndexMap key (PHP string-or-int)
                let is_numeric_name = name.parse::<i64>().is_ok();
                if is_numeric_name {
                    if !self.repositories.contains_key(name) {
                        self.repositories.insert(name.clone(), repository.clone());
                    } else {
                        // PHP: $this->repositories[] = $repository
                        // appending to numeric-keyed map
                        let next_idx = self.repositories.len();
                        self.repositories
                            .insert(next_idx.to_string(), repository.clone());
                    }
                    let found_key = array_search_mixed(
                        repository,
                        &PhpMixed::Array(
                            self.repositories
                                .iter()
                                .map(|(k, v)| (k.clone(), Box::new(v.clone())))
                                .collect(),
                        ),
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
                        .insert(format!("{}.org", name), repository.clone());
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
            self.repositories = array_reverse(&self.repositories, true);
        }
    }

    /// @return array<int|string, mixed>
    pub fn get_repositories(&self) -> IndexMap<String, PhpMixed> {
        self.repositories.clone()
    }

    /// Returns a setting
    ///
    /// @param  int               $flags Options (see class constants)
    /// @throws \RuntimeException
    ///
    /// @return mixed
    pub fn get(&mut self, key: &str) -> PhpMixed {
        self.get_with_flags(key, 0).unwrap_or(PhpMixed::Null)
    }

    pub fn get_with_flags(&mut self, key: &str, flags: i64) -> Result<PhpMixed> {
        match key {
            // strings/paths with env var and {$refs} support
            "vendor-dir" | "bin-dir" | "process-timeout" | "data-dir" | "cache-dir"
            | "cache-files-dir" | "cache-repo-dir" | "cache-vcs-dir" | "cafile" | "capath" => {
                // convert foo-bar to COMPOSER_FOO_BAR and check if it exists since it overrides the local config
                let env = format!("COMPOSER_{}", strtoupper(&strtr(key, "-", "_")));

                let val = self.get_composer_env(&env);
                if !matches!(val, PhpMixed::Bool(false)) {
                    self.set_source_of_config_value(&val, key, &env);
                }

                if key == "process-timeout" {
                    let raw = if matches!(val, PhpMixed::Bool(false)) {
                        self.config.get(key).cloned().unwrap_or(PhpMixed::Null)
                    } else {
                        val.clone()
                    };
                    return Ok(PhpMixed::Int(max(0, raw.as_int().unwrap_or(0))));
                }

                let raw_val = if matches!(val, PhpMixed::Bool(false)) {
                    self.config.get(key).cloned().unwrap_or(PhpMixed::Null)
                } else {
                    val
                };
                let processed = self.process(raw_val, flags);
                let mut val_str = rtrim(processed.as_string().unwrap_or(""), Some("/\\"));
                val_str = Platform::expand_path(&val_str);

                if substr(key, -4, None) != "-dir" {
                    return Ok(PhpMixed::String(val_str));
                }

                Ok(PhpMixed::String(
                    if (flags & Self::RELATIVE_PATHS) == Self::RELATIVE_PATHS {
                        val_str
                    } else {
                        self.realpath(&val_str)
                    },
                ))
            }

            // booleans with env var support
            "cache-read-only" | "htaccess-protect" => {
                // convert foo-bar to COMPOSER_FOO_BAR and check if it exists since it overrides the local config
                let env = format!("COMPOSER_{}", strtoupper(&strtr(key, "-", "_")));

                let val = self.get_composer_env(&env);
                let val = if matches!(val, PhpMixed::Bool(false)) {
                    self.config.get(key).cloned().unwrap_or(PhpMixed::Null)
                } else {
                    self.set_source_of_config_value(&val, key, &env);
                    val
                };

                Ok(PhpMixed::Bool(
                    val.as_string() != Some("false")
                        && val.as_bool().unwrap_or_else(|| !val.is_null()),
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

                let v = self.config.get(key).cloned().unwrap_or(PhpMixed::Null);
                Ok(PhpMixed::Bool(
                    v.as_string() != Some("false") && v.as_bool().unwrap_or(false),
                ))
            }

            // ints without env var support
            "cache-ttl" => Ok(PhpMixed::Int(max(
                0,
                self.config.get(key).and_then(|v| v.as_int()).unwrap_or(0),
            ))),

            // numbers with kb/mb/gb support, without env var support
            "cache-files-maxsize" => {
                let raw = self
                    .config
                    .get(key)
                    .and_then(|v| v.as_string())
                    .unwrap_or("")
                    .to_string();
                let matches = Preg::is_match_strict_groups(
                    r"/^\s*([0-9.]+)\s*(?:([kmg])(?:i?b)?)?\s*$/i",
                    &raw,
                );
                let matches = match matches {
                    Some(m) => m,
                    None => {
                        return Err(RuntimeException {
                            message: format!("Could not parse the value of '{}': {}", key, raw),
                            code: 0,
                        }
                        .into());
                    }
                };
                let mut size = matches
                    .get(1)
                    .cloned()
                    .unwrap_or_default()
                    .parse::<f64>()
                    .unwrap_or(0.0);
                let unit = matches.get(2).cloned();
                if let Some(unit) = unit {
                    match strtolower(&unit).as_str() {
                        "g" => {
                            size *= 1024.0;
                            size *= 1024.0;
                            size *= 1024.0;
                        }
                        "m" => {
                            size *= 1024.0;
                            size *= 1024.0;
                        }
                        "k" => {
                            size *= 1024.0;
                        }
                        _ => {}
                    }
                }

                Ok(PhpMixed::Int(max(0, size as i64)))
            }

            // special cases below
            "cache-files-ttl" => {
                let v = self.config.get(key).cloned();
                if let Some(v) = v {
                    if !v.is_null() {
                        return Ok(PhpMixed::Int(max(0, v.as_int().unwrap_or(0))));
                    }
                }

                self.get_with_flags("cache-ttl", 0)
            }

            "home" => {
                let v = self.config.get(key).cloned().unwrap_or(PhpMixed::Null);
                let expanded = Platform::expand_path(v.as_string().unwrap_or(""));
                let processed = self.process(PhpMixed::String(expanded), flags);
                Ok(PhpMixed::String(rtrim(
                    processed.as_string().unwrap_or(""),
                    Some("/\\"),
                )))
            }

            "bin-compat" => {
                let env_val = self.get_composer_env("COMPOSER_BIN_COMPAT");
                let value = match env_val {
                    PhpMixed::Bool(false) | PhpMixed::Null => self
                        .config
                        .get(key)
                        .and_then(|v| v.as_string())
                        .unwrap_or("")
                        .to_string(),
                    other => other.as_string().unwrap_or("").to_string(),
                };

                if !in_array(
                    PhpMixed::String(value.clone()),
                    &PhpMixed::List(vec![
                        Box::new(PhpMixed::String("auto".to_string())),
                        Box::new(PhpMixed::String("full".to_string())),
                        Box::new(PhpMixed::String("proxy".to_string())),
                        Box::new(PhpMixed::String("symlink".to_string())),
                    ]),
                    false,
                ) {
                    return Err(RuntimeException {
                        message: format!(
                            "Invalid value for 'bin-compat': {}. Expected auto, full or proxy",
                            value
                        ),
                        code: 0,
                    }
                    .into());
                }

                if value == "symlink" {
                    trigger_error(
                        "config.bin-compat \"symlink\" is deprecated since Composer 2.2, use auto, full (for Windows compatibility) or proxy instead.",
                        E_USER_DEPRECATED,
                    );
                }

                Ok(PhpMixed::String(value))
            }

            "discard-changes" => {
                let env = self.get_composer_env("COMPOSER_DISCARD_CHANGES");
                if !matches!(env, PhpMixed::Bool(false)) {
                    let env_str = env.as_string().unwrap_or("").to_string();
                    if !in_array(
                        PhpMixed::String(env_str.clone()),
                        &PhpMixed::List(vec![
                            Box::new(PhpMixed::String("stash".to_string())),
                            Box::new(PhpMixed::String("true".to_string())),
                            Box::new(PhpMixed::String("false".to_string())),
                            Box::new(PhpMixed::String("1".to_string())),
                            Box::new(PhpMixed::String("0".to_string())),
                        ]),
                        true,
                    ) {
                        return Err(RuntimeException {
                            message: format!(
                                "Invalid value for COMPOSER_DISCARD_CHANGES: {}. Expected 1, 0, true, false or stash",
                                env_str
                            ),
                            code: 0,
                        }
                        .into());
                    }
                    if env_str == "stash" {
                        return Ok(PhpMixed::String("stash".to_string()));
                    }

                    // convert string value to bool
                    return Ok(PhpMixed::Bool(
                        env_str != "false" && !env_str.is_empty() && env_str != "0",
                    ));
                }

                let val = self.config.get(key).cloned().unwrap_or(PhpMixed::Null);
                let allowed = matches!(&val, PhpMixed::Bool(_)) || val.as_string() == Some("stash");
                if !allowed {
                    return Err(RuntimeException {
                        message: format!(
                            "Invalid value for 'discard-changes': {:?}. Expected true, false or stash",
                            val
                        ),
                        code: 0,
                    }
                    .into());
                }

                Ok(val)
            }

            "github-protocols" => {
                let mut protos: Vec<String> = self
                    .config
                    .get("github-protocols")
                    .and_then(|v| v.as_list())
                    .map(|l| {
                        l.iter()
                            .filter_map(|v| v.as_string().map(|s| s.to_string()))
                            .collect()
                    })
                    .unwrap_or_default();
                let secure_http = self
                    .config
                    .get("secure-http")
                    .and_then(|v| v.as_bool())
                    .unwrap_or(false);
                if secure_http {
                    let map: IndexMap<String, String> = protos
                        .iter()
                        .enumerate()
                        .map(|(i, s)| (i.to_string(), s.clone()))
                        .collect();
                    let found = array_search_mixed(
                        &PhpMixed::String("git".to_string()),
                        &PhpMixed::Array(
                            map.into_iter()
                                .map(|(k, v)| (k, Box::new(PhpMixed::String(v))))
                                .collect(),
                        ),
                        false,
                    );
                    if let Some(idx_val) = found {
                        let idx = idx_val
                            .as_string()
                            .unwrap_or("")
                            .parse::<usize>()
                            .unwrap_or(usize::MAX);
                        if idx < protos.len() {
                            protos.remove(idx);
                        }
                    }
                }
                let first = reset(&protos);
                if first.as_deref() == Some("http") {
                    return Err(RuntimeException {
                        message: "The http protocol for github is not available anymore, update your config's github-protocols to use \"https\", \"git\" or \"ssh\"".to_string(),
                        code: 0,
                    }
                    .into());
                }

                Ok(PhpMixed::List(
                    protos
                        .into_iter()
                        .map(|s| Box::new(PhpMixed::String(s)))
                        .collect(),
                ))
            }

            "autoloader-suffix" => {
                let v = self.config.get(key).cloned().unwrap_or(PhpMixed::Null);
                if v.as_string() == Some("") {
                    // we need to guarantee null or non-empty-string
                    return Ok(PhpMixed::Null);
                }

                Ok(self.process(v, flags))
            }

            "audit" => {
                let mut result = self.config.get(key).cloned().unwrap_or(PhpMixed::Null);
                let abandoned_env = self.get_composer_env("COMPOSER_AUDIT_ABANDONED");
                if !matches!(abandoned_env, PhpMixed::Bool(false)) {
                    let abandoned_env_str = abandoned_env.as_string().unwrap_or("").to_string();
                    let valid_choices: Vec<String> =
                        Auditor::ABANDONEDS.iter().map(|s| s.to_string()).collect();
                    if !in_array(
                        PhpMixed::String(abandoned_env_str.clone()),
                        &PhpMixed::List(
                            valid_choices
                                .iter()
                                .map(|s| Box::new(PhpMixed::String(s.clone())))
                                .collect(),
                        ),
                        true,
                    ) {
                        return Err(RuntimeException {
                            message: format!(
                                "Invalid value for COMPOSER_AUDIT_ABANDONED: {}. Expected one of {}.",
                                abandoned_env_str,
                                implode(", ", &valid_choices),
                            ),
                            code: 0,
                        }
                        .into());
                    }
                    if let PhpMixed::Array(ref mut m) = result {
                        m.insert(
                            "abandoned".to_string(),
                            Box::new(PhpMixed::String(abandoned_env_str)),
                        );
                    }
                }

                let block_abandoned_env =
                    self.get_composer_env("COMPOSER_SECURITY_BLOCKING_ABANDONED");
                if !matches!(block_abandoned_env, PhpMixed::Bool(false)) {
                    let env_str = block_abandoned_env.as_string().unwrap_or("").to_string();
                    if !in_array(
                        PhpMixed::String(env_str.clone()),
                        &PhpMixed::List(vec![
                            Box::new(PhpMixed::String("0".to_string())),
                            Box::new(PhpMixed::String("1".to_string())),
                        ]),
                        true,
                    ) {
                        return Err(RuntimeException {
                            message: format!(
                                "Invalid value for COMPOSER_SECURITY_BLOCKING_ABANDONED: {}. Expected 0 or 1.",
                                env_str
                            ),
                            code: 0,
                        }
                        .into());
                    }
                    if let PhpMixed::Array(ref mut m) = result {
                        m.insert(
                            "block-abandoned".to_string(),
                            Box::new(PhpMixed::Bool(env_str == "1")),
                        );
                    }
                }

                Ok(result)
            }

            _ => {
                if !self.config.contains_key(key) {
                    return Ok(PhpMixed::Null);
                }

                let v = self.config.get(key).cloned().unwrap_or(PhpMixed::Null);
                Ok(self.process(v, flags))
            }
        }
    }

    /// @return array<string, mixed[]>
    pub fn all(&mut self, flags: i64) -> Result<IndexMap<String, PhpMixed>> {
        let mut all: IndexMap<String, PhpMixed> = IndexMap::new();
        all.insert(
            "repositories".to_string(),
            PhpMixed::Array(
                self.get_repositories()
                    .into_iter()
                    .map(|(k, v)| (k, Box::new(v)))
                    .collect(),
            ),
        );
        let keys: Vec<String> = self.config.keys().cloned().collect();
        let mut config_section: IndexMap<String, Box<PhpMixed>> = IndexMap::new();
        for key in keys {
            config_section.insert(key.clone(), Box::new(self.get_with_flags(&key, flags)?));
        }
        all.insert("config".to_string(), PhpMixed::Array(config_section));

        Ok(all)
    }

    pub fn get_source_of_value(&mut self, key: &str) -> String {
        let _ = self.get(key);

        self.source_of_config_value
            .get(key)
            .cloned()
            .unwrap_or_else(|| Self::SOURCE_UNKNOWN.to_string())
    }

    /// @param mixed  $configValue
    fn set_source_of_config_value(&mut self, config_value: &PhpMixed, path: &str, source: &str) {
        self.source_of_config_value
            .insert(path.to_string(), source.to_string());

        if is_array(config_value.clone()) {
            let map = match config_value {
                PhpMixed::Array(m) => m
                    .iter()
                    .map(|(k, v)| (k.clone(), (**v).clone()))
                    .collect::<Vec<_>>(),
                _ => vec![],
            };
            for (key, value) in map {
                self.set_source_of_config_value(&value, &format!("{}.{}", path, key), source);
            }
        }
    }

    /// @return array<string, mixed[]>
    pub fn raw(&self) -> IndexMap<String, PhpMixed> {
        let mut result: IndexMap<String, PhpMixed> = IndexMap::new();
        result.insert(
            "repositories".to_string(),
            PhpMixed::Array(
                self.get_repositories()
                    .into_iter()
                    .map(|(k, v)| (k, Box::new(v)))
                    .collect(),
            ),
        );
        result.insert(
            "config".to_string(),
            PhpMixed::Array(
                self.config
                    .iter()
                    .map(|(k, v)| (k.clone(), Box::new(v.clone())))
                    .collect(),
            ),
        );
        result
    }

    /// Checks whether a setting exists
    pub fn has(&self, key: &str) -> bool {
        array_key_exists(key, &self.config)
    }

    /// Replaces {$refs} inside a config string
    ///
    /// @param  string|mixed $value a config string that can contain {$refs-to-other-config}
    /// @param  int          $flags Options (see class constants)
    ///
    /// @return string|mixed
    fn process(&mut self, value: PhpMixed, flags: i64) -> PhpMixed {
        if !is_string(&value) {
            return value;
        }

        let value_str = value.as_string().unwrap_or("").to_string();
        // TODO(phase-b): Preg::replace_callback with a closure that calls &mut self.get_with_flags
        let mut result = value_str.clone();
        if let Some(m) = Preg::is_match_strict_groups(r"#\{\$(.+)\}#", &value_str) {
            let key_match = m.get(1).cloned().unwrap_or_default();
            let replacement = self
                .get_with_flags(&key_match, flags)
                .ok()
                .and_then(|v| v.as_string().map(|s| s.to_string()))
                .unwrap_or_default();
            result = result.replace(&format!("{{${}}}", key_match), &replacement);
        }
        PhpMixed::String(result)
    }

    /// Turns relative paths in absolute paths without realpath()
    ///
    /// Since the dirs might not exist yet we can not call realpath or it will fail.
    fn realpath(&self, path: &str) -> String {
        if Preg::is_match(r"{^(?:/|[a-z]:|[a-z0-9.]+://|\\\\\\\\)}i", path).unwrap_or(false) {
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
    ///
    /// @param non-empty-string $var
    ///
    /// @return string|false
    fn get_composer_env(&self, var: &str) -> PhpMixed {
        if self.use_environment {
            return match Platform::get_env(var) {
                Some(v) => PhpMixed::String(v),
                None => PhpMixed::Bool(false),
            };
        }

        PhpMixed::Bool(false)
    }

    fn disable_repo_by_name(&mut self, name: &str) {
        if self.repositories.contains_key(name) {
            self.repositories.shift_remove(name);
        } else if name == "packagist" {
            // BC support for default "packagist" named repo
            self.repositories.shift_remove("packagist.org");
        }
    }

    /// Validates that the passed URL is allowed to be used by current config, or throws an exception.
    pub fn prohibit_url_by_config(
        &mut self,
        url: &str,
        io: Option<&dyn IOInterface>,
        repo_options: &IndexMap<String, PhpMixed>,
    ) -> Result<()> {
        // Return right away if the URL is malformed or custom (see issue #5173), but only for non-HTTP(S) URLs
        if !filter_var(url, FILTER_VALIDATE_URL)
            && !Preg::is_match(r"{^https?://}", url).unwrap_or(false)
        {
            return Ok(());
        }

        // Extract scheme and throw exception on known insecure protocols
        let scheme = parse_url(url, PHP_URL_SCHEME)
            .as_string()
            .map(|s| s.to_string());
        let hostname = parse_url(url, PHP_URL_HOST)
            .as_string()
            .map(|s| s.to_string());
        if in_array(
            scheme
                .clone()
                .map(PhpMixed::String)
                .unwrap_or(PhpMixed::Null),
            &PhpMixed::List(vec![
                Box::new(PhpMixed::String("http".to_string())),
                Box::new(PhpMixed::String("git".to_string())),
                Box::new(PhpMixed::String("ftp".to_string())),
                Box::new(PhpMixed::String("svn".to_string())),
            ]),
            false,
        ) {
            if self.get_with_flags("secure-http", 0)?.as_bool() == Some(true) {
                if scheme.as_deref() == Some("svn") {
                    if in_array(
                        hostname
                            .clone()
                            .map(PhpMixed::String)
                            .unwrap_or(PhpMixed::Null),
                        &self.get_with_flags("secure-svn-domains", 0)?,
                        true,
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
            if let Some(io) = io {
                if let Some(ref hostname) = hostname {
                    if !self.warned_hosts.contains_key(hostname) {
                        io.write_error(
                            PhpMixed::String(format!(
                                "<warning>Warning: Accessing {} over {} which is an insecure protocol.</warning>",
                                hostname,
                                scheme.as_deref().unwrap_or("")
                            )),
                            true,
                            io_interface::NORMAL,
                        );
                    }
                    self.warned_hosts.insert(hostname.clone(), true);
                }
            }
        }

        if let Some(io) = io {
            if let Some(ref hostname) = hostname {
                if !self.ssl_verify_warned_hosts.contains_key(hostname) {
                    let mut warning: Option<String> = None;
                    let verify_peer = repo_options
                        .get("ssl")
                        .and_then(|v| v.as_array())
                        .and_then(|m| m.get("verify_peer"));
                    if let Some(v) = verify_peer {
                        if v.as_bool() == Some(false) {
                            warning = Some("verify_peer".to_string());
                        }
                    }

                    let verify_peer_name = repo_options
                        .get("ssl")
                        .and_then(|v| v.as_array())
                        .and_then(|m| m.get("verify_peer_name"));
                    if let Some(v) = verify_peer_name {
                        if v.as_bool() == Some(false) {
                            warning = match warning {
                                None => Some("verify_peer_name".to_string()),
                                Some(w) => Some(format!("{} and verify_peer_name", w)),
                            };
                        }
                    }

                    if let Some(w) = warning {
                        io.write_error(
                            PhpMixed::String(format!(
                                "<warning>Warning: Accessing {} with {} disabled.</warning>",
                                hostname, w
                            )),
                            true,
                            io_interface::NORMAL,
                        );
                        self.ssl_verify_warned_hosts.insert(hostname.clone(), true);
                    }
                }
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
