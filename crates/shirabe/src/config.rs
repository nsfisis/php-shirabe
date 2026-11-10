//! ref: composer/src/Composer/Config.php sha1:7e0a18fcbf536c9e5003f72a666077a4559cb226

use shirabe_php_shim::{
    E_USER_DEPRECATED, Error, PhpMixed, PregMatches, RuntimeException, ToPhpString, TypeError,
    array_key_exists, array_merge, array_search_mixed, array_unique, bformat, boolval,
    canonical_int_key, empty, filter_var_url, get_debug_type, implode, in_array_loose,
    in_array_strict, intval, is_array, is_numeric_string, is_string, loose_eq, parse_url,
    php_regex, php_str, preg_is_match, preg_match, preg_replace_callback, rtrim_chars, strtolower,
    strtoupper, strtr, substr, trigger_error, utf8_text,
};
mod config_source_interface;
mod json_config_source;

pub use config_source_interface::*;
pub use json_config_source::*;

use crate::io::{IOInterface, IOInterfaceWrite};
use bstr::{B, BStr, BString, ByteSlice as _};
use indexmap::IndexMap;

use crate::advisory::Auditor;
use crate::downloader::TransportException;
use crate::util::{Platform, ProcessExecutor, StoreAuth};

/// The value of the `discard-changes` setting.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DiscardChanges {
    Bool(bool),
    Stash,
}

/// The value of the `preferred-install` setting.
#[derive(Debug, Clone, PartialEq)]
pub enum PreferredInstall {
    /// One install method for every package.
    Method(BString),
    /// Install methods keyed by package name pattern.
    PerPackage(IndexMap<BString, PhpMixed>),
    /// A value of neither type the schema allows.
    Other(PhpMixed),
}

impl PreferredInstall {
    /// PHP's loose `==`, as the `switch` statements over the setting compare it.
    pub fn loosely_equals(&self, method: impl AsRef<[u8]>) -> bool {
        let method = PhpMixed::string(BString::from(method.as_ref()));
        match self {
            Self::Method(value) => loose_eq(&PhpMixed::string(value.clone()), &method),
            Self::PerPackage(map) => loose_eq(&PhpMixed::Array(map.clone()), &method),
            Self::Other(value) => loose_eq(value, &method),
        }
    }
}

/// The value of the `allow-plugins` setting.
#[derive(Debug, Clone, PartialEq)]
pub enum AllowPlugins {
    Bool(bool),
    /// Whether to allow each plugin, keyed by package name pattern.
    Rules(IndexMap<BString, PhpMixed>),
}

/// The value of the `use-parent-dir` setting.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UseParentDir {
    Bool(bool),
    Prompt,
}

/// The value of the `platform-check` setting.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PlatformCheck {
    Disabled,
    /// Checks the PHP version and the extensions.
    Full,
    /// Checks the PHP version only.
    PhpOnly,
}

/// The value of the `bump-after-update` setting, or of the update command's
/// `--bump-after-update` option.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BumpAfterUpdate {
    Disabled,
    Dev,
    NoDev,
    All,
}

impl BumpAfterUpdate {
    pub fn from_value(value: &PhpMixed) -> Self {
        match value {
            PhpMixed::Bool(false) => Self::Disabled,
            PhpMixed::String(s) if s == "dev" => Self::Dev,
            PhpMixed::String(s) if s == "no-dev" => Self::NoDev,
            _ => Self::All,
        }
    }
}

#[derive(Debug)]
pub struct Config {
    config: IndexMap<BString, PhpMixed>,
    base_dir: Option<BString>,
    repositories: IndexMap<BString, PhpMixed>,
    config_source: Option<Box<dyn ConfigSourceInterface>>,
    auth_config_source: Option<Box<dyn ConfigSourceInterface>>,
    local_auth_config_source: Option<Box<dyn ConfigSourceInterface>>,
    use_environment: bool,
    warned_hosts: IndexMap<BString, bool>,
    ssl_verify_warned_hosts: IndexMap<BString, bool>,
    source_of_config_value: std::cell::RefCell<IndexMap<BString, BString>>,
}

impl Config {
    pub const SOURCE_DEFAULT: &'static BStr = php_str(b"default");
    pub const SOURCE_COMMAND: &'static BStr = php_str(b"command");
    pub const SOURCE_UNKNOWN: &'static BStr = php_str(b"unknown");

    pub const RELATIVE_PATHS: i64 = 1;

    pub fn default_config() -> IndexMap<BString, PhpMixed> {
        let mut c: IndexMap<BString, PhpMixed> = IndexMap::new();
        c.insert("process-timeout".into(), PhpMixed::Int(300));
        c.insert("use-include-path".into(), PhpMixed::Bool(false));
        c.insert("allow-plugins".into(), PhpMixed::Array(IndexMap::new()));
        c.insert(
            "use-parent-dir".into(),
            PhpMixed::string(BString::from("prompt")),
        );
        c.insert(
            "preferred-install".into(),
            PhpMixed::string(BString::from("dist")),
        );
        let mut audit: IndexMap<BString, PhpMixed> = IndexMap::new();
        audit.insert("ignore".into(), PhpMixed::Array(IndexMap::new()));
        audit.insert(
            "abandoned".into(),
            PhpMixed::string(Auditor::ABANDONED_FAIL.to_php_string()),
        );
        c.insert("audit".into(), PhpMixed::Array(audit));
        c.insert("notify-on-install".into(), PhpMixed::Bool(true));
        c.insert(
            "github-protocols".into(),
            PhpMixed::List(vec![
                PhpMixed::string(BString::from("https")),
                PhpMixed::string(BString::from("ssh")),
                PhpMixed::string(BString::from("git")),
            ]),
        );
        c.insert("gitlab-protocol".into(), PhpMixed::Null);
        c.insert(
            "vendor-dir".into(),
            PhpMixed::string(BString::from("vendor")),
        );
        c.insert(
            "bin-dir".into(),
            PhpMixed::string(BString::from("{$vendor-dir}/bin")),
        );
        c.insert(
            "cache-dir".into(),
            PhpMixed::string(BString::from("{$home}/cache")),
        );
        c.insert(
            "data-dir".into(),
            PhpMixed::string(BString::from("{$home}")),
        );
        c.insert(
            "cache-files-dir".into(),
            PhpMixed::string(BString::from("{$cache-dir}/files")),
        );
        c.insert(
            "cache-repo-dir".into(),
            PhpMixed::string(BString::from("{$cache-dir}/repo")),
        );
        c.insert(
            "cache-vcs-dir".into(),
            PhpMixed::string(BString::from("{$cache-dir}/vcs")),
        );
        c.insert("cache-ttl".into(), PhpMixed::Int(15552000)); // 6 months
        c.insert("cache-files-ttl".into(), PhpMixed::Null); // fallback to cache-ttl
        c.insert(
            "cache-files-maxsize".into(),
            PhpMixed::string(BString::from("300MiB")),
        );
        c.insert("cache-read-only".into(), PhpMixed::Bool(false));
        c.insert("bin-compat".into(), PhpMixed::string(BString::from("auto")));
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
            PhpMixed::List(vec![PhpMixed::string(BString::from("github.com"))]),
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
            PhpMixed::List(vec![PhpMixed::string(BString::from("gitlab.com"))]),
        );
        c.insert(
            "store-auths".into(),
            PhpMixed::string(BString::from("prompt")),
        );
        c.insert("platform".into(), PhpMixed::Array(IndexMap::new()));
        c.insert(
            "archive-format".into(),
            PhpMixed::string(BString::from("tar")),
        );
        c.insert("archive-dir".into(), PhpMixed::string(BString::from(".")));
        c.insert("htaccess-protect".into(), PhpMixed::Bool(true));
        c.insert("use-github-api".into(), PhpMixed::Bool(true));
        c.insert("lock".into(), PhpMixed::Bool(true));
        c.insert(
            "platform-check".into(),
            PhpMixed::string(BString::from("php-only")),
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
            PhpMixed::List(vec![PhpMixed::string(BString::from("codeberg.org"))]),
        );
        c.insert("forgejo-token".into(), PhpMixed::Array(IndexMap::new()));
        c
    }

    pub fn default_repositories() -> IndexMap<BString, PhpMixed> {
        let mut r: IndexMap<BString, PhpMixed> = IndexMap::new();
        let mut packagist: IndexMap<BString, PhpMixed> = IndexMap::new();
        packagist.insert("type".into(), PhpMixed::string(BString::from("composer")));
        packagist.insert(
            "url".into(),
            PhpMixed::string(BString::from("https://repo.packagist.org")),
        );
        r.insert("packagist.org".into(), PhpMixed::Array(packagist));
        r
    }

    pub fn new(use_environment: bool, base_dir: Option<BString>) -> Self {
        let this = Self {
            // load defaults
            config: Self::default_config(),
            repositories: Self::default_repositories(),
            use_environment,
            base_dir: base_dir.filter(|s| !s.is_empty()),
            config_source: None,
            auth_config_source: None,
            local_auth_config_source: None,
            warned_hosts: IndexMap::new(),
            ssl_verify_warned_hosts: IndexMap::new(),
            source_of_config_value: std::cell::RefCell::new(IndexMap::new()),
        };

        let config_clone = this.config.clone();
        for (config_key, config_value) in &config_clone {
            this.set_source_of_config_value(config_value, config_key, Self::SOURCE_DEFAULT);
        }

        let repositories_clone = this.repositories.clone();
        for (config_key, config_value) in &repositories_clone {
            this.set_source_of_config_value(
                config_value,
                &bformat!("repositories.{config_key}"),
                Self::SOURCE_DEFAULT,
            );
        }

        this
    }

    /// Changing this can break path resolution for relative config paths so do not call this without knowing what you are doing
    ///
    /// The $baseDir should be an absolute path and without trailing slash
    pub fn set_base_dir(&mut self, base_dir: Option<BString>) {
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
    pub fn merge(&mut self, config: &IndexMap<BString, PhpMixed>, source: impl AsRef<[u8]>) {
        let source = source.as_ref();
        // override defaults with given config
        let config_section = config.get(B("config")).cloned().unwrap_or(PhpMixed::Null);
        if !empty(&config_section) && is_array(&config_section) {
            let config_section_map = match config_section {
                PhpMixed::Array(m) => m,
                _ => IndexMap::new(),
            };
            for (key, val_box) in &config_section_map {
                let key_str = key.as_slice();
                let val = val_box.clone();
                if matches!(
                    key_str,
                    b"bitbucket-oauth"
                        | b"github-oauth"
                        | b"gitlab-oauth"
                        | b"gitlab-token"
                        | b"http-basic"
                        | b"bearer"
                        | b"client-certificate"
                        | b"forgejo-token"
                ) && self.config.contains_key(key)
                {
                    let existing = self.config.get(key).cloned().unwrap_or(PhpMixed::Null);
                    self.config
                        .insert(key.clone(), array_merge(existing, val.clone()));
                    self.set_source_of_config_value(&val, key_str, source);
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
                    self.set_source_of_config_value(&val, key_str, source);
                } else if matches!(key_str, b"gitlab-domains" | b"github-domains")
                    && self.config.contains_key(key)
                {
                    let existing = self.config.get(key).cloned().unwrap_or(PhpMixed::Null);
                    let merged = array_merge(existing, val.clone());
                    let unique_list: Vec<BString> = match &merged {
                        PhpMixed::List(l) => l
                            .iter()
                            .filter_map(|v| v.as_string().map(BStr::to_owned))
                            .collect(),
                        _ => vec![],
                    };
                    let deduped = array_unique(&unique_list);
                    self.config.insert(
                        key.clone(),
                        PhpMixed::List(deduped.into_iter().map(PhpMixed::string).collect()),
                    );
                    self.set_source_of_config_value(&val, key_str, source);
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
                                .insert(bformat!("{key}*"), source.into());
                        }
                        let cur = self.config.get(key).cloned().unwrap_or(PhpMixed::Null);
                        self.config
                            .insert(key.clone(), array_merge(cur, val.clone()));
                        self.set_source_of_config_value(&val, key_str, source);
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
                        self.set_source_of_config_value(&val, key_str, source);
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
                    self.set_source_of_config_value(&val, key_str, source);
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
                    self.set_source_of_config_value(&val, key_str, source);
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
                    .map(|(i, v)| (i.to_php_string(), v.clone()))
                    .collect(),
                _ => IndexMap::new(),
            };
            let new_repos: IndexMap<BString, PhpMixed> = new_repos_map.into_iter().rev().collect();
            for (name, repository) in &new_repos {
                // disable a repository by name
                // this is a code path, that will be used less as the next check will be preferred
                if matches!(repository, PhpMixed::Bool(false)) {
                    self.disable_repo_by_name(name.as_ref());
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
                    self.disable_repo_by_name(name.as_ref());
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
                    .unwrap_or_default();
                if is_composer
                    && preg_is_match(
                        php_regex!(r"{^https?://(?:[a-z0-9-.]+\.)?packagist.org(/|$)}"),
                        repo_url,
                    )
                {
                    self.disable_repo_by_name(BStr::new("packagist.org"));
                }

                // store repo
                let is_numeric_name = canonical_int_key(name).is_some();
                if is_numeric_name {
                    if !self.repositories.contains_key(name) {
                        self.repositories.insert(name.clone(), repository.clone());
                    } else {
                        // PHP: $this->repositories[] = $repository
                        // appending to numeric-keyed map
                        let next_idx = self.repositories.len();
                        self.repositories
                            .insert(next_idx.to_php_string(), repository.clone());
                    }
                    let found_key = array_search_mixed(
                        repository,
                        &PhpMixed::Array(self.repositories.clone()),
                        true,
                    )
                    .and_then(|v| v.as_string().map(BStr::to_owned))
                    .unwrap_or_default();
                    self.set_source_of_config_value(
                        repository,
                        &bformat!("repositories.{found_key}"),
                        source,
                    );
                } else if name == "packagist" {
                    // BC support for default "packagist" named repo
                    self.repositories
                        .insert(bformat!("{name}.org"), repository.clone());
                    self.set_source_of_config_value(
                        repository,
                        &bformat!("repositories.{name}.org"),
                        source,
                    );
                } else {
                    self.repositories.insert(name.clone(), repository.clone());
                    self.set_source_of_config_value(
                        repository,
                        &bformat!("repositories.{name}"),
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
    pub fn get(&self, key: impl AsRef<[u8]>) -> anyhow::Result<PhpMixed> {
        self.get_with_flags(key, 0)
    }

    pub fn get_with_flags(&self, key: impl AsRef<[u8]>, flags: i64) -> anyhow::Result<PhpMixed> {
        let key = key.as_ref();
        match key {
            // strings/paths with env var and {$refs} support
            b"vendor-dir" | b"bin-dir" | b"data-dir" | b"cache-dir" | b"cache-files-dir"
            | b"cache-repo-dir" | b"cache-vcs-dir" | b"cafile" | b"capath" => {
                Ok(PhpMixed::string(self.get_path_with_flags(key, flags)?))
            }
            b"process-timeout" => Ok(self.process_timeout()),

            // booleans with env var support
            b"cache-read-only" | b"htaccess-protect"
            // booleans without env var support
            | b"disable-tls" | b"secure-http" | b"use-github-api" | b"lock" => {
                Ok(PhpMixed::Bool(self.get_bool(key)))
            }

            // ints without env var support
            b"cache-ttl" => Ok(PhpMixed::Int(self.get_cache_ttl())),

            // numbers with kb/mb/gb support, without env var support
            b"cache-files-maxsize" => Ok(PhpMixed::Int(self.get_cache_files_maxsize()?)),

            // special cases below
            b"cache-files-ttl" => Ok(PhpMixed::Int(self.get_cache_files_ttl())),

            b"home" => Ok(PhpMixed::string(self.get_home_with_flags(flags)?)),

            b"bin-compat" => self.bin_compat(),

            b"discard-changes" => Ok(match self.get_discard_changes()? {
                DiscardChanges::Bool(value) => PhpMixed::Bool(value),
                DiscardChanges::Stash => PhpMixed::string(BString::from("stash")),
            }),

            b"github-protocols" => {
                let protos = self.github_protocols()?;
                Ok(
                    if protos.keys().enumerate().all(|(i, k)| *k == i.to_php_string()) {
                        PhpMixed::List(protos.into_values().collect())
                    } else {
                        PhpMixed::Array(protos)
                    },
                )
            }

            b"autoloader-suffix" => {
                let v = self.raw_value(key);
                if v.as_string() == Some("".into()) {
                    // we need to guarantee null or non-empty-string
                    return Ok(PhpMixed::Null);
                }

                self.process(v, flags)
            }

            b"audit" => self.audit(),

            _ => self.get_default(key, flags),
        }
    }

    /// `get()` of one of the path settings (`vendor-dir`, `bin-dir`, `data-dir`, `cache-dir`,
    /// `cache-files-dir`, `cache-repo-dir`, `cache-vcs-dir`, `cafile`, `capath`).
    pub fn get_path(&self, key: impl AsRef<[u8]>) -> anyhow::Result<BString> {
        self.get_path_with_flags(key, 0)
    }

    pub fn get_path_with_flags(
        &self,
        key: impl AsRef<[u8]>,
        flags: i64,
    ) -> anyhow::Result<BString> {
        let key = key.as_ref();
        assert!(
            matches!(
                key,
                b"vendor-dir"
                    | b"bin-dir"
                    | b"data-dir"
                    | b"cache-dir"
                    | b"cache-files-dir"
                    | b"cache-repo-dir"
                    | b"cache-vcs-dir"
                    | b"cafile"
                    | b"capath"
            ),
            "{:?} is not a path setting",
            key.as_bstr()
        );
        // convert foo-bar to COMPOSER_FOO_BAR and check if it exists since it overrides the local config
        let env = bformat!("COMPOSER_{}", strtoupper(strtr(key, "-", "_")));

        let val = self.get_composer_env(&env);
        let raw_val = match val {
            Some(val) => {
                let val = PhpMixed::string(val);
                self.set_source_of_config_value(&val, key, &env);
                val
            }
            None => self.raw_value(key),
        };

        let processed = self.process(raw_val, flags)?;
        let mut val_str = rtrim_chars(processed.to_php_string(), "/\\");
        val_str = Platform::expand_path(&val_str);

        if substr(key, -4, None) != "-dir" {
            return Ok(val_str);
        }

        Ok(if (flags & Self::RELATIVE_PATHS) == Self::RELATIVE_PATHS {
            val_str
        } else {
            self.realpath(val_str.as_ref())
        })
    }

    /// PHP's `max(0, ...)` over the `process-timeout` setting, which casts the environment
    /// variable but not the configured value.
    fn process_timeout(&self) -> PhpMixed {
        let env = "COMPOSER_PROCESS_TIMEOUT";
        if let Some(val) = self.get_composer_env(env) {
            let val = PhpMixed::string(val);
            self.set_source_of_config_value(&val, "process-timeout", env);
            return PhpMixed::Int(intval(&val).max(0));
        }

        let val = self.raw_value(b"process-timeout");
        let greater_than_zero = match &val {
            PhpMixed::Null => false,
            PhpMixed::Bool(b) => *b,
            PhpMixed::Int(i) => *i > 0,
            PhpMixed::Float(f) => *f > 0.0,
            PhpMixed::String(s) => {
                if is_numeric_string(s) {
                    utf8_text(s)
                        .expect("a numeric string is ASCII")
                        .trim()
                        .parse::<f64>()
                        .expect("a numeric string parses as a float")
                        > 0.0
                } else {
                    s.as_slice() > b"0".as_slice()
                }
            }
            PhpMixed::List(_) | PhpMixed::Array(_) | PhpMixed::Object(_) => true,
        };
        if greater_than_zero {
            val
        } else {
            PhpMixed::Int(0)
        }
    }

    /// `get('process-timeout')` as `ProcessExecutor::setTimeout()` receives it.
    pub fn get_process_timeout(&self) -> anyhow::Result<i64> {
        match self.process_timeout() {
            PhpMixed::Int(timeout) => Ok(timeout),
            other => Err(TypeError::new(bformat!("Composer\\Util\\ProcessExecutor::setTimeout(): Argument #1 ($timeout) must be of type int, {} given", get_debug_type(&other)))
            .into()),
        }
    }

    /// `get()` of one of the boolean settings that PHP casts (`cache-read-only`,
    /// `htaccess-protect`, `disable-tls`, `secure-http`, `use-github-api`, `lock`).
    pub fn get_bool(&self, key: impl AsRef<[u8]>) -> bool {
        let key = key.as_ref();
        let val = match key {
            // booleans with env var support
            b"cache-read-only" | b"htaccess-protect" => {
                // convert foo-bar to COMPOSER_FOO_BAR and check if it exists since it overrides the local config
                let env = bformat!("COMPOSER_{}", strtoupper(strtr(key, "-", "_")));

                match self.get_composer_env(&env) {
                    None => self.raw_value(key),
                    Some(val) => {
                        let val = PhpMixed::string(val);
                        self.set_source_of_config_value(&val, key, &env);
                        val
                    }
                }
            }

            // booleans without env var support
            b"disable-tls" | b"secure-http" | b"use-github-api" | b"lock" => {
                // special case for secure-http
                if key == b"secure-http" && self.get_bool("disable-tls") {
                    return false;
                }

                self.raw_value(key)
            }

            _ => panic!(
                "{:?} is not a boolean setting cast by Config",
                key.as_bstr()
            ),
        };

        val.as_string() != Some("false".into()) && val.to_bool()
    }

    pub fn get_cache_ttl(&self) -> i64 {
        intval(&self.raw_value(b"cache-ttl")).max(0)
    }

    pub fn get_cache_files_maxsize(&self) -> anyhow::Result<i64> {
        let key = "cache-files-maxsize";
        let raw = self.raw_value(key).to_php_string();
        let Some(matches) = preg_match(
            php_regex!(r"/^\s*([0-9.]+)\s*(?:([kmg])(?:i?b)?)?\s*$/i"),
            &raw,
        ) else {
            return Err(RuntimeException::new(bformat!(
                "Could not parse the value of '{key}': {raw}"
            ))
            .into());
        };
        let mut size = utf8_text(matches.get(1).expect("the size group always participates"))
            .and_then(|size| size.parse::<f64>().ok())
            .unwrap_or(0.0);
        if let Some(unit) = matches.get(2) {
            match strtolower(unit).as_slice() {
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
                _ => unreachable!(),
            }
        }

        Ok(intval(&PhpMixed::Float(size)).max(0))
    }

    pub fn get_cache_files_ttl(&self) -> i64 {
        let v = self.raw_value(b"cache-files-ttl");
        if !v.is_null() {
            return intval(&v).max(0);
        }

        self.get_cache_ttl()
    }

    pub fn get_home(&self) -> anyhow::Result<BString> {
        self.get_home_with_flags(0)
    }

    fn get_home_with_flags(&self, flags: i64) -> anyhow::Result<BString> {
        let v = self.raw_value(b"home");
        let Some(home) = v.as_string() else {
            return Err(TypeError::new(bformat!("Composer\\Util\\Platform::expandPath(): Argument #1 ($path) must be of type string, {} given", get_debug_type(&v)))
            .into());
        };
        let processed = self.process(PhpMixed::string(Platform::expand_path(home)), flags)?;
        Ok(rtrim_chars(processed.to_php_string(), "/\\"))
    }

    fn bin_compat(&self) -> anyhow::Result<PhpMixed> {
        let value = match self.get_composer_env("COMPOSER_BIN_COMPAT") {
            Some(env) if boolval(&env) => PhpMixed::string(env),
            _ => self.raw_value(b"bin-compat"),
        };

        if !in_array_loose(
            value.clone(),
            &[
                PhpMixed::string(BString::from("auto")),
                PhpMixed::string(BString::from("full")),
                PhpMixed::string(BString::from("proxy")),
                PhpMixed::string(BString::from("symlink")),
            ],
        ) {
            return Err(RuntimeException::new(bformat!(
                "Invalid value for 'bin-compat': {}. Expected auto, full or proxy",
                value.to_php_string()
            ))
            .into());
        }

        if value.as_string() == Some("symlink".into()) {
            trigger_error(
                "config.bin-compat \"symlink\" is deprecated since Composer 2.2, use auto, full (for Windows compatibility) or proxy instead.",
                E_USER_DEPRECATED,
            );
        }

        Ok(value)
    }

    /// `get('bin-compat')` as `BinaryInstaller` receives it.
    pub fn get_bin_compat(&self) -> anyhow::Result<BString> {
        let value = self.bin_compat()?;
        match value {
            PhpMixed::String(value) => Ok(value),
            other => Err(TypeError::new(bformat!("Composer\\Installer\\BinaryInstaller::__construct(): Argument #3 ($binCompat) must be of type string, {} given", get_debug_type(&other)))
            .into()),
        }
    }

    pub fn get_discard_changes(&self) -> anyhow::Result<DiscardChanges> {
        if let Some(env) = self.get_composer_env("COMPOSER_DISCARD_CHANGES") {
            if !matches!(env.as_slice(), b"stash" | b"true" | b"false" | b"1" | b"0") {
                return Err(RuntimeException::new(bformat!("Invalid value for COMPOSER_DISCARD_CHANGES: {env}. Expected 1, 0, true, false or stash"))
                .into());
            }
            if env == "stash" {
                return Ok(DiscardChanges::Stash);
            }

            // convert string value to bool
            return Ok(DiscardChanges::Bool(env != "false" && boolval(&env)));
        }

        match self.raw_value(b"discard-changes") {
            PhpMixed::Bool(value) => Ok(DiscardChanges::Bool(value)),
            PhpMixed::String(s) if s == "stash" => Ok(DiscardChanges::Stash),
            other => Err(RuntimeException::new(bformat!(
                "Invalid value for 'discard-changes': {}. Expected true, false or stash",
                other.to_php_string()
            ))
            .into()),
        }
    }

    fn github_protocols(&self) -> anyhow::Result<IndexMap<BString, PhpMixed>> {
        let mut protos = match self.raw_value(b"github-protocols") {
            PhpMixed::List(list) => list
                .into_iter()
                .enumerate()
                .map(|(i, v)| (i.to_php_string(), v))
                .collect(),
            PhpMixed::Array(map) => map,
            other => {
                return Err(TypeError::new(bformat!(
                    "array_search(): Argument #2 ($haystack) must be of type array, {} given",
                    get_debug_type(&other)
                ))
                .into());
            }
        };
        if self.raw_value(b"secure-http").to_bool()
            && let Some(index) = protos
                .iter()
                .find(|(_, v)| loose_eq(v, &PhpMixed::string(BString::from("git"))))
                .map(|(k, _)| k.clone())
        {
            protos.shift_remove(&index);
        }
        if protos.values().next().and_then(|v| v.as_string()) == Some("http".into()) {
            return Err(RuntimeException::new(BString::from("The http protocol for github is not available anymore, update your config's github-protocols to use \"https\", \"git\" or \"ssh\""))
                .into());
        }

        Ok(protos)
    }

    /// `get('github-protocols')`, with each protocol converted to a string as the consumers
    /// concatenate or strictly compare them with strings.
    pub fn get_github_protocols(&self) -> anyhow::Result<Vec<BString>> {
        Ok(self
            .github_protocols()?
            .values()
            .map(ToPhpString::to_php_string)
            .collect())
    }

    /// `get('autoloader-suffix')` as `AutoloadGenerator` uses it: `null`, or the suffix
    /// that `getAutoloadFile()` receives as a string.
    pub fn get_autoloader_suffix(&self) -> anyhow::Result<Option<BString>> {
        match self.get_with_flags("autoloader-suffix", 0)? {
            PhpMixed::Null => Ok(None),
            PhpMixed::String(suffix) => Ok(Some(suffix)),
            other => Err(TypeError::new(bformat!("Composer\\Autoload\\AutoloadGenerator::getAutoloadFile(): Argument #2 ($suffix) must be of type string, {} given", get_debug_type(&other)))
            .into()),
        }
    }

    fn audit(&self) -> anyhow::Result<PhpMixed> {
        let mut result = self.raw_value(b"audit");
        if let Some(abandoned_env) = self.get_composer_env("COMPOSER_AUDIT_ABANDONED") {
            if !Auditor::ABANDONEDS.iter().any(|c| abandoned_env == *c) {
                let valid_choices: Vec<BString> = Auditor::ABANDONEDS
                    .iter()
                    .map(|s| s.to_php_string())
                    .collect();
                return Err(RuntimeException::new(bformat!("Invalid value for COMPOSER_AUDIT_ABANDONED: {abandoned_env}. Expected one of {}.", implode(", ", &valid_choices)))
                .into());
            }
            Self::audit_map(&mut result)?
                .insert("abandoned".into(), PhpMixed::string(abandoned_env));
        }

        if let Some(env) = self.get_composer_env("COMPOSER_SECURITY_BLOCKING_ABANDONED") {
            if !matches!(env.as_slice(), b"0" | b"1") {
                return Err(RuntimeException::new(bformat!("Invalid value for COMPOSER_SECURITY_BLOCKING_ABANDONED: {env}. Expected 0 or 1."))
                .into());
            }
            Self::audit_map(&mut result)?
                .insert("block-abandoned".into(), PhpMixed::Bool(env == "1"));
        }

        Ok(result)
    }

    fn audit_map(result: &mut PhpMixed) -> anyhow::Result<&mut IndexMap<BString, PhpMixed>> {
        match result {
            // PHP auto-vivifies an array on assignment to an offset of null or false
            PhpMixed::Null | PhpMixed::Bool(false) => *result = PhpMixed::Array(IndexMap::new()),
            PhpMixed::List(list) => {
                *result = PhpMixed::Array(
                    std::mem::take(list)
                        .into_iter()
                        .enumerate()
                        .map(|(i, v)| (i.to_php_string(), v))
                        .collect(),
                )
            }
            _ => {}
        }
        match result {
            PhpMixed::Array(map) => Ok(map),
            PhpMixed::String(_) => {
                Err(Error::new("Cannot access offset of type string on string").into())
            }
            _ => Err(Error::new("Cannot use a scalar value as an array").into()),
        }
    }

    /// `get('audit')` as `AuditConfig::fromConfig()` reads it.
    pub fn get_audit(&self) -> anyhow::Result<IndexMap<BString, PhpMixed>> {
        let mut result = self.audit()?;
        Ok(Self::audit_map(&mut result)?.clone())
    }

    /// `$this->config[$key]`, `null` for a missing key.
    fn raw_value(&self, key: impl AsRef<[u8]>) -> PhpMixed {
        self.config
            .get(B(key.as_ref()))
            .cloned()
            .unwrap_or(PhpMixed::Null)
    }

    /// The `default:` branch of `get()`: the value neither cast nor validated.
    fn get_default(&self, key: &[u8], flags: i64) -> anyhow::Result<PhpMixed> {
        match self.config.get(B(key)) {
            None | Some(PhpMixed::Null) => Ok(PhpMixed::Null),
            Some(v) => self.process(v.clone(), flags),
        }
    }

    // TODO(error-model): PHP raises the consumer's own error instead, a TypeError or the
    // ErrorException that Composer's error handler makes of a warning, with its own message.
    fn type_error(key: &[u8], expected: impl AsRef<[u8]>, value: &PhpMixed) -> anyhow::Error {
        TypeError::new(bformat!(
            "The value of config.{} must be of type {}, {} given",
            key.as_bstr(),
            expected.as_ref().as_bstr(),
            get_debug_type(value)
        ))
        .into()
    }

    /// `get()` of a setting that consumers only test for truthiness (`if ($config->get(...))`,
    /// `$option || $config->get(...)`).
    pub fn get_truthy(&self, key: impl AsRef<[u8]>) -> anyhow::Result<bool> {
        let key = key.as_ref();
        assert!(
            matches!(
                key,
                b"notify-on-install"
                    | b"sort-packages"
                    | b"optimize-autoloader"
                    | b"classmap-authoritative"
                    | b"apcu-autoloader"
                    | b"update-with-minimal-changes"
                    | b"allow-missing-requirements"
            ),
            "{:?} is not a boolean setting",
            key.as_bstr()
        );
        Ok(self.get_default(key, 0)?.to_bool())
    }

    /// `get()` of a boolean setting that a consumer passes to a `bool` parameter, where PHP's
    /// strict types reject any other type.
    pub fn get_strict_bool(&self, key: impl AsRef<[u8]>) -> anyhow::Result<bool> {
        let key = key.as_ref();
        assert!(
            matches!(
                key,
                b"use-include-path"
                    | b"optimize-autoloader"
                    | b"classmap-authoritative"
                    | b"apcu-autoloader"
            ),
            "{:?} is not a boolean setting",
            key.as_bstr()
        );
        match self.get_default(key, 0)? {
            PhpMixed::Bool(value) => Ok(value),
            other => Err(Self::type_error(key, "bool", &other)),
        }
    }

    /// `get('prepend-autoloader') !== false`
    pub fn get_prepend_autoloader(&self) -> anyhow::Result<bool> {
        Ok(!matches!(
            self.get_default(b"prepend-autoloader", 0)?,
            PhpMixed::Bool(false)
        ))
    }

    /// `get('github-expose-hostname') === true`
    pub fn get_github_expose_hostname(&self) -> anyhow::Result<bool> {
        Ok(matches!(
            self.get_default(b"github-expose-hostname", 0)?,
            PhpMixed::Bool(true)
        ))
    }

    /// `get('gitlab-protocol')` when it is a string; consumers ignore any other value.
    pub fn get_gitlab_protocol(&self) -> anyhow::Result<Option<BString>> {
        Ok(match self.get_default(b"gitlab-protocol", 0)? {
            PhpMixed::String(protocol) => Some(protocol),
            _ => None,
        })
    }

    /// `get()` of a string setting that a consumer passes to a `string` parameter
    /// (`archive-format`, `archive-dir`).
    pub fn get_string(&self, key: impl AsRef<[u8]>) -> anyhow::Result<BString> {
        let key = key.as_ref();
        assert!(
            matches!(key, b"archive-format" | b"archive-dir"),
            "{:?} is not a string setting",
            key.as_bstr()
        );
        match self.get_default(key, 0)? {
            PhpMixed::String(value) => Ok(value),
            other => Err(Self::type_error(key, "string", &other)),
        }
    }

    /// The values of a list setting (`github-domains`, `gitlab-domains`, `forgejo-domains`,
    /// `secure-svn-domains`). Consumers pass it to `in_array()`, which rejects a non-array, and
    /// compare its elements with PHP semantics, so the elements keep their type.
    pub fn get_list(&self, key: impl AsRef<[u8]>) -> anyhow::Result<Vec<PhpMixed>> {
        let key = key.as_ref();
        assert!(
            matches!(
                key,
                b"github-domains" | b"gitlab-domains" | b"forgejo-domains" | b"secure-svn-domains"
            ),
            "{:?} is not a list setting",
            key.as_bstr()
        );
        match self.get_default(key, 0)? {
            PhpMixed::List(list) => Ok(list),
            PhpMixed::Array(map) => Ok(map.into_values().collect()),
            other => Err(Self::type_error(key, "array", &other)),
        }
    }

    /// A map setting (`platform`, and the per-host authentication and header settings).
    pub fn get_map(&self, key: impl AsRef<[u8]>) -> anyhow::Result<IndexMap<BString, PhpMixed>> {
        let key = key.as_ref();
        Self::assert_map_key(key);
        Self::to_map(key, self.get_default(key, 0)?)
    }

    /// `get($key) ?: []` of a map setting.
    pub fn get_map_or_empty(
        &self,
        key: impl AsRef<[u8]>,
    ) -> anyhow::Result<IndexMap<BString, PhpMixed>> {
        let key = key.as_ref();
        Self::assert_map_key(key);
        let value = self.get_default(key, 0)?;
        if !value.to_bool() {
            return Ok(IndexMap::new());
        }
        Self::to_map(key, value)
    }

    fn assert_map_key(key: &[u8]) {
        assert!(
            matches!(
                key,
                b"platform"
                    | b"bitbucket-oauth"
                    | b"github-oauth"
                    | b"gitlab-oauth"
                    | b"gitlab-token"
                    | b"http-basic"
                    | b"bearer"
                    | b"custom-headers"
                    | b"client-certificate"
                    | b"forgejo-token"
            ),
            "{:?} is not a map setting",
            key.as_bstr()
        );
    }

    fn to_map(key: &[u8], value: PhpMixed) -> anyhow::Result<IndexMap<BString, PhpMixed>> {
        match value {
            PhpMixed::Array(map) => Ok(map),
            PhpMixed::List(list) => Ok(list
                .into_iter()
                .enumerate()
                .map(|(i, v)| (i.to_php_string(), v))
                .collect()),
            other => Err(Self::type_error(key, "array", &other)),
        }
    }

    pub fn get_preferred_install(&self) -> anyhow::Result<PreferredInstall> {
        Ok(match self.get_default(b"preferred-install", 0)? {
            PhpMixed::String(method) => PreferredInstall::Method(method),
            PhpMixed::Array(map) => PreferredInstall::PerPackage(map),
            PhpMixed::List(list) => PreferredInstall::PerPackage(
                list.into_iter()
                    .enumerate()
                    .map(|(i, v)| (i.to_php_string(), v))
                    .collect(),
            ),
            other => PreferredInstall::Other(other),
        })
    }

    /// `get('allow-plugins')` as `PluginManager::parseAllowedPlugins()` receives it.
    pub fn get_allow_plugins(&self) -> anyhow::Result<AllowPlugins> {
        let key = b"allow-plugins";
        match self.get_default(key, 0)? {
            PhpMixed::Bool(value) => Ok(AllowPlugins::Bool(value)),
            value @ (PhpMixed::Array(_) | PhpMixed::List(_)) => {
                Ok(AllowPlugins::Rules(Self::to_map(key, value)?))
            }
            other => Err(Self::type_error(key, "array|bool", &other)),
        }
    }

    /// `get('store-auths')` as `AuthHelper::storeAuth()` compares it: `=== true`, `=== 'prompt'`,
    /// and nothing stored for any other value.
    pub fn get_store_auths(&self) -> anyhow::Result<StoreAuth> {
        Ok(match self.get_default(b"store-auths", 0)? {
            PhpMixed::Bool(true) => StoreAuth::Bool(true),
            PhpMixed::String(s) if s == "prompt" => StoreAuth::Prompt,
            _ => StoreAuth::Bool(false),
        })
    }

    /// `get('use-parent-dir')` as the application compares it: `=== true`, `=== false`, and
    /// prompting for any other value.
    pub fn get_use_parent_dir(&self) -> anyhow::Result<UseParentDir> {
        Ok(match self.get_default(b"use-parent-dir", 0)? {
            PhpMixed::Bool(value) => UseParentDir::Bool(value),
            _ => UseParentDir::Prompt,
        })
    }

    /// `get('platform-check')` as `AutoloadGenerator` compares it: `!== false` enables the check,
    /// `=== true` extends it to extensions, and any other value checks the PHP version only.
    pub fn get_platform_check(&self) -> anyhow::Result<PlatformCheck> {
        Ok(match self.get_default(b"platform-check", 0)? {
            PhpMixed::Bool(false) => PlatformCheck::Disabled,
            PhpMixed::Bool(true) => PlatformCheck::Full,
            _ => PlatformCheck::PhpOnly,
        })
    }

    /// `get('bump-after-update')` as the update command compares it: `false !== ...` runs the
    /// bump, `=== 'dev'` and `=== 'no-dev'` restrict it, and any other value bumps everything.
    pub fn get_bump_after_update(&self) -> anyhow::Result<BumpAfterUpdate> {
        Ok(BumpAfterUpdate::from_value(
            &self.get_default(b"bump-after-update", 0)?,
        ))
    }

    pub fn all(&self, flags: i64) -> anyhow::Result<IndexMap<BString, PhpMixed>> {
        let mut all: IndexMap<BString, PhpMixed> = IndexMap::new();
        all.insert(
            "repositories".into(),
            PhpMixed::Array(self.get_repositories()),
        );
        let keys: Vec<BString> = self.config.keys().cloned().collect();
        let mut config_section: IndexMap<BString, PhpMixed> = IndexMap::new();
        for key in keys {
            let value = self.get_with_flags(&key, flags)?;
            config_section.insert(key, value);
        }
        all.insert("config".into(), PhpMixed::Array(config_section));

        Ok(all)
    }

    pub fn get_source_of_value(&self, key: impl AsRef<[u8]>) -> anyhow::Result<BString> {
        let key = key.as_ref();
        self.get(key)?;

        Ok(self
            .source_of_config_value
            .borrow()
            .get(B(key))
            .cloned()
            .unwrap_or_else(|| Self::SOURCE_UNKNOWN.into()))
    }

    fn set_source_of_config_value(
        &self,
        config_value: &PhpMixed,
        path: impl AsRef<[u8]>,
        source: impl AsRef<[u8]>,
    ) {
        let path = path.as_ref();
        let source = source.as_ref();
        self.source_of_config_value
            .borrow_mut()
            .insert(path.into(), source.into());

        if is_array(config_value) {
            let map = match config_value {
                PhpMixed::Array(m) => m
                    .iter()
                    .map(|(k, v)| (k.clone(), v.clone()))
                    .collect::<Vec<_>>(),
                _ => vec![],
            };
            for (key, value) in map {
                self.set_source_of_config_value(&value, &bformat!("{path}.{key}"), source);
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
    pub fn has(&self, key: impl AsRef<[u8]>) -> bool {
        let key = key.as_ref();
        array_key_exists(B(key), &self.config)
    }

    /// Replaces {$refs} inside a config string
    fn process(&self, value: PhpMixed, flags: i64) -> anyhow::Result<PhpMixed> {
        if !is_string(&value) {
            return Ok(value);
        }

        let result = preg_replace_callback(
            php_regex!(r"#\{\$(.+)\}#"),
            |m: &PregMatches| -> anyhow::Result<BString> {
                Ok(self
                    .get_with_flags(m.get(1).unwrap_or_default(), flags)?
                    .to_php_string())
            },
            value.as_string().unwrap_or_default(),
        )?;
        Ok(PhpMixed::string(result))
    }

    /// Turns relative paths in absolute paths without realpath()
    ///
    /// Since the dirs might not exist yet we can not call realpath or it will fail.
    fn realpath(&self, path: &BStr) -> BString {
        if preg_is_match(php_regex!(r"{^(?:/|[a-z]:|[a-z0-9.]+://|\\\\\\\\)}i"), path) {
            return path.into();
        }

        match &self.base_dir {
            Some(base) => bformat!("{base}/{path}"),
            None => path.into(),
        }
    }

    /// Reads the value of a Composer environment variable
    ///
    /// This should be used to read COMPOSER_ environment variables
    /// that overload config values.
    fn get_composer_env(&self, var: impl AsRef<[u8]>) -> Option<BString> {
        if self.use_environment {
            return Platform::get_env(var);
        }

        None
    }

    fn disable_repo_by_name(&mut self, name: &BStr) {
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
        url: &BStr,
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
        if matches!(
            scheme.as_ref().map(|scheme| scheme.as_slice()),
            Some(b"http" | b"git" | b"ftp" | b"svn")
        ) {
            if self.get_bool("secure-http") {
                if scheme.as_ref().is_some_and(|scheme| scheme == "svn") {
                    if in_array_strict(
                        hostname.map(PhpMixed::string).unwrap_or(PhpMixed::Null),
                        &self.get_list("secure-svn-domains")?,
                    ) {
                        return Ok(());
                    }

                    return Err(TransportException::new(
                        bformat!("Your configuration does not allow connections to {url}. See https://getcomposer.org/doc/06-config.md#secure-svn-domains for details."),
                        0,
                    )
                    .into());
                }

                return Err(TransportException::new(
                    bformat!("Your configuration does not allow connections to {url}. See https://getcomposer.org/doc/06-config.md#secure-http for details."),
                    0,
                )
                .into());
            }
            if let Some(ref io) = io
                && let Some(ref hostname) = hostname
            {
                if !self.warned_hosts.contains_key(hostname) {
                    io.write_error(bformat!("<warning>Warning: Accessing {hostname} over {} which is an insecure protocol.</warning>", scheme
                            .as_ref()
                            .map_or(B(""), |scheme| scheme.as_slice())
                            .as_bstr()));
                }
                self.warned_hosts.insert(hostname.clone(), true);
            }
        }

        if let Some(ref io) = io
            && let Some(ref hostname) = hostname
            && !self.ssl_verify_warned_hosts.contains_key(hostname)
        {
            let mut warning: Option<BString> = None;
            let verify_peer = repo_options
                .get(B("ssl"))
                .and_then(|v| v.as_array())
                .and_then(|m| m.get(B("verify_peer")));
            if let Some(v) = verify_peer.and_then(|v| v.as_opt())
                && !v.to_bool()
            {
                warning = Some(BString::from("verify_peer"));
            }

            let verify_peer_name = repo_options
                .get(B("ssl"))
                .and_then(|v| v.as_array())
                .and_then(|m| m.get(B("verify_peer_name")));
            if let Some(v) = verify_peer_name.and_then(|v| v.as_opt())
                && !v.to_bool()
            {
                warning = match warning {
                    None => Some(BString::from("verify_peer_name")),
                    Some(w) => Some(bformat!("{w} and verify_peer_name")),
                };
            }

            if let Some(w) = warning {
                io.write_error(bformat!(
                    "<warning>Warning: Accessing {hostname} with {w} disabled.</warning>"
                ));
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
