//! ref: composer/src/Composer/Repository/PlatformRepository.php

use crate::composer;
use crate::package::CompletePackage;
use crate::package::CompletePackageHandle;
use crate::package::CompletePackageInterface;
use crate::package::CompletePackageInterfaceHandle;
use crate::package::Link;
use crate::package::PackageInterface;
use crate::package::PackageInterfaceHandle;
use crate::package::version::VersionParser;
use crate::platform::HhvmDetector;
use crate::platform::HhvmDetectorInterface;
use crate::platform::Runtime;
use crate::platform::RuntimeInterface;
use crate::platform::Version;
use crate::plugin::plugin_interface::{self};
use crate::repository::ArrayRepository;
use crate::repository::RepositoryInterface;
use crate::util::Silencer;
use indexmap::IndexMap;
use shirabe_external_packages::composer::pcre::{CaptureKey, Preg};
use shirabe_external_packages::composer::xdebug_handler::XdebugHandler;
use shirabe_php_shim::{
    InvalidArgumentException, PhpMixed, UnexpectedValueException, array_map_str_fn,
    array_slice_strs, explode, get_class, implode, in_array, is_string, php_regex, str_replace,
    str_starts_with, strpos, strtolower, var_export,
};
use shirabe_semver::constraint::SimpleConstraint;
use std::sync::{LazyLock, Mutex};

static LAST_SEEN_PLATFORM_PHP: LazyLock<Mutex<Option<String>>> = LazyLock::new(|| Mutex::new(None));

static IS_PLATFORM_PACKAGE_CACHE: LazyLock<Mutex<IndexMap<String, bool>>> =
    LazyLock::new(|| Mutex::new(IndexMap::new()));

#[derive(Debug)]
pub struct PlatformOverride {
    pub name: String,
    /// `PhpMixed::String(_)` or `PhpMixed::Bool(false)`.
    pub version: PhpMixed,
}

#[derive(Debug)]
pub struct PlatformRepository {
    pub(crate) inner: ArrayRepository,
    pub(crate) version_parser: Option<VersionParser>,
    pub(crate) overrides: IndexMap<String, PlatformOverride>,
    pub(crate) disabled_packages: IndexMap<String, CompletePackageInterfaceHandle>,
    pub(crate) runtime: Box<dyn RuntimeInterface>,
    pub(crate) hhvm_detector: Box<dyn HhvmDetectorInterface>,
}

impl PlatformRepository {
    const PLATFORM_PACKAGE_REGEX: &'static str = "{^(?:php(?:-64bit|-ipv6|-zts|-debug)?|hhvm|(?:ext|lib)-[a-z0-9](?:[_.-]?[a-z0-9]+)*|composer(?:-(?:plugin|runtime)-api)?)$}iD";

    pub fn new(
        packages: Vec<PackageInterfaceHandle>,
        overrides: IndexMap<String, PhpMixed>,
    ) -> anyhow::Result<Self> {
        Self::new4(packages, overrides, None, None)
    }

    pub fn new4(
        packages: Vec<PackageInterfaceHandle>,
        overrides: IndexMap<String, PhpMixed>,
        runtime: Option<Box<dyn RuntimeInterface>>,
        hhvm_detector: Option<Box<dyn HhvmDetectorInterface>>,
    ) -> anyhow::Result<Self> {
        let runtime: Box<dyn RuntimeInterface> = runtime.unwrap_or_else(|| Box::new(Runtime));
        let hhvm_detector: Box<dyn HhvmDetectorInterface> =
            hhvm_detector.unwrap_or_else(|| Box::new(HhvmDetector::new(None, None)));
        let mut overrides_map: IndexMap<String, PlatformOverride> = IndexMap::new();
        for (name, version) in overrides {
            if !is_string(&version) && !matches!(version, PhpMixed::Bool(false)) {
                return Err(anyhow::anyhow!(UnexpectedValueException {
                    message: format!(
                        "config.platform.{} should be a string or false, but got {} {}",
                        name,
                        shirabe_php_shim::get_debug_type(&version),
                        var_export(&version, true)
                    ),
                    code: 0,
                }));
            }
            if name == "php" && matches!(version, PhpMixed::Bool(false)) {
                return Err(anyhow::anyhow!(UnexpectedValueException {
                    message: format!(
                        "config.platform.{} cannot be set to false as you cannot disable php entirely.",
                        name
                    ),
                    code: 0,
                }));
            }
            overrides_map.insert(
                strtolower(&name),
                PlatformOverride {
                    name: name.clone(),
                    version,
                },
            );
        }
        Ok(Self {
            inner: ArrayRepository::new(packages)?,
            version_parser: None,
            overrides: overrides_map,
            disabled_packages: IndexMap::new(),
            runtime,
            hhvm_detector,
        })
    }

    pub fn get_repo_name(&self) -> String {
        "platform repo".to_string()
    }

    pub fn is_platform_package_disabled(&self, name: &str) -> bool {
        self.disabled_packages.contains_key(name)
    }

    pub fn get_disabled_packages(&self) -> &IndexMap<String, CompletePackageInterfaceHandle> {
        &self.disabled_packages
    }

    fn ensure_initialized(&mut self) -> anyhow::Result<()> {
        if !self.inner.is_initialized() {
            self.initialize()?;
        }
        Ok(())
    }

    pub(crate) fn initialize(&mut self) -> anyhow::Result<()> {
        self.inner.initialize();

        let mut libraries: IndexMap<String, bool> = IndexMap::new();

        self.version_parser = Some(VersionParser::new());

        // Add each of the override versions as options.
        // Later we might even replace the extensions instead.
        let overrides: Vec<PlatformOverride> = self
            .overrides
            .values()
            .map(|o| PlatformOverride {
                name: o.name.clone(),
                version: o.version.clone(),
            })
            .collect();
        for r#override in &overrides {
            // Check that it's a platform package.
            if !Self::is_platform_package(&r#override.name) {
                return Err(anyhow::anyhow!(InvalidArgumentException {
                    message: format!(
                        "Invalid platform package name in config.platform: {}",
                        r#override.name
                    ),
                    code: 0,
                }));
            }

            if !matches!(r#override.version, PhpMixed::Bool(false)) {
                self.add_overridden_package(r#override, None)?;
            }
        }

        let mut pretty_version = composer::get_version();
        let mut version = self
            .version_parser
            .as_ref()
            .unwrap()
            .normalize(&pretty_version, None)?;
        let mut composer = CompletePackage::new(
            "composer".to_string(),
            version.clone(),
            pretty_version.clone(),
        );
        composer.set_description("Composer package".to_string());
        self.add_package(CompletePackageHandle::from_complete_package(composer).into())?;

        pretty_version = plugin_interface::PLUGIN_API_VERSION.to_string();
        version = self
            .version_parser
            .as_ref()
            .unwrap()
            .normalize(&pretty_version, None)?;
        let mut composer_plugin_api = CompletePackage::new(
            "composer-plugin-api".to_string(),
            version.clone(),
            pretty_version.clone(),
        );
        composer_plugin_api.set_description("The Composer Plugin API".to_string());
        self.add_package(CompletePackageHandle::from_complete_package(composer_plugin_api).into())?;

        pretty_version = composer::RUNTIME_API_VERSION.to_string();
        version = self
            .version_parser
            .as_ref()
            .unwrap()
            .normalize(&pretty_version, None)?;
        let mut composer_runtime_api = CompletePackage::new(
            "composer-runtime-api".to_string(),
            version.clone(),
            pretty_version.clone(),
        );
        composer_runtime_api.set_description("The Composer Runtime API".to_string());
        self.add_package(
            CompletePackageHandle::from_complete_package(composer_runtime_api).into(),
        )?;

        let php_version_const = self.runtime.get_constant("PHP_VERSION", None);
        let php_version_str = match &php_version_const {
            PhpMixed::String(s) => s.clone(),
            _ => "".to_string(),
        };
        match self
            .version_parser
            .as_ref()
            .unwrap()
            .normalize(&php_version_str, None)
        {
            Ok(v) => {
                pretty_version = php_version_str.clone();
                version = v;
            }
            Err(_) => {
                pretty_version =
                    Preg::replace(php_regex!("#^([^~+-]+).*$#"), "$1", &php_version_str);
                version = self
                    .version_parser
                    .as_ref()
                    .unwrap()
                    .normalize(&pretty_version, None)?;
            }
        }

        let mut php =
            CompletePackage::new("php".to_string(), version.clone(), pretty_version.clone());
        php.set_description("The PHP interpreter".to_string());
        self.add_package(CompletePackageHandle::from_complete_package(php).into())?;

        if self
            .runtime
            .get_constant("PHP_DEBUG", None)
            .as_bool()
            .unwrap_or(false)
        {
            let mut phpdebug = CompletePackage::new(
                "php-debug".to_string(),
                version.clone(),
                pretty_version.clone(),
            );
            phpdebug.set_description("The PHP interpreter, with debugging symbols".to_string());
            self.add_package(CompletePackageHandle::from_complete_package(phpdebug).into())?;
        }

        if self.runtime.has_constant("PHP_ZTS", None)
            && self
                .runtime
                .get_constant("PHP_ZTS", None)
                .as_bool()
                .unwrap_or(false)
        {
            let mut phpzts = CompletePackage::new(
                "php-zts".to_string(),
                version.clone(),
                pretty_version.clone(),
            );
            phpzts.set_description("The PHP interpreter, with Zend Thread Safety".to_string());
            self.add_package(CompletePackageHandle::from_complete_package(phpzts).into())?;
        }

        if self
            .runtime
            .get_constant("PHP_INT_SIZE", None)
            .as_int()
            .map(|v| v == 8)
            .unwrap_or(false)
        {
            let mut php64 = CompletePackage::new(
                "php-64bit".to_string(),
                version.clone(),
                pretty_version.clone(),
            );
            php64.set_description("The PHP interpreter, 64bit".to_string());
            self.add_package(CompletePackageHandle::from_complete_package(php64).into())?;
        }

        // The AF_INET6 constant is only defined if ext-sockets is available but
        // IPv6 support might still be available.
        let has_inet6 = self.runtime.has_constant("AF_INET6", None);
        // PHP: Silencer::call([$this->runtime, 'invoke'], 'inet_pton', ['::'])
        let inet_pton_check = Silencer::call(|| {
            Ok::<PhpMixed, anyhow::Error>(self.runtime.invoke(
                PhpMixed::String("inet_pton".to_string()),
                vec![PhpMixed::String("::".to_string())],
            ))
        })
        .unwrap_or(PhpMixed::Bool(false));
        if has_inet6 || !matches!(inet_pton_check, PhpMixed::Bool(false)) {
            let mut php_ipv6 = CompletePackage::new(
                "php-ipv6".to_string(),
                version.clone(),
                pretty_version.clone(),
            );
            php_ipv6.set_description("The PHP interpreter, with IPv6 support".to_string());
            self.add_package(CompletePackageHandle::from_complete_package(php_ipv6).into())?;
        }

        let loaded_extensions = self.runtime.get_extensions();

        // Extensions scanning
        for name in &loaded_extensions {
            if ["standard", "Core"].contains(&name.as_str()) {
                continue;
            }

            self.add_extension(name, &self.runtime.get_extension_version(name))?;
        }

        // Check for Xdebug in a restarted process
        if !in_array(
            PhpMixed::String("xdebug".to_string()),
            &PhpMixed::Array(
                loaded_extensions
                    .iter()
                    .enumerate()
                    .map(|(i, s)| (i.to_string(), PhpMixed::String(s.clone())))
                    .collect(),
            ),
            true,
        ) && let Some(xdebug_pretty_version) = XdebugHandler::get_skipped_version()
            && !xdebug_pretty_version.is_empty()
        {
            self.add_extension("xdebug", &xdebug_pretty_version)?;
        }

        // Another quick loop, just for possible libraries
        // Doing it this way to know that functions or constants exist before
        // relying on them.
        for name in &loaded_extensions {
            match name.as_str() {
                "amqp" => {
                    let info = self.runtime.get_extension_info(name)?;

                    // librabbitmq version => 0.9.0
                    let mut librabbitmq_matches: IndexMap<CaptureKey, String> = IndexMap::new();
                    if Preg::is_match3(
                        php_regex!("/^librabbitmq version => (?<version>.+)$/im"),
                        &info,
                        Some(&mut librabbitmq_matches),
                    ) {
                        self.add_library(
                            &mut libraries,
                            &format!("{}-librabbitmq", name),
                            librabbitmq_matches
                                .get(&CaptureKey::ByName("version".to_string()))
                                .map(|s| s.as_str()),
                            Some("AMQP librabbitmq version"),
                            &[],
                            &[],
                        )?;
                    }

                    // AMQP protocol version => 0-9-1
                    let mut protocol_matches: IndexMap<CaptureKey, String> = IndexMap::new();
                    if Preg::is_match3(
                        php_regex!("/^AMQP protocol version => (?<version>.+)$/im"),
                        &info,
                        Some(&mut protocol_matches),
                    ) {
                        let version_str = protocol_matches
                            .get(&CaptureKey::ByName("version".to_string()))
                            .cloned()
                            .unwrap_or_default();
                        self.add_library(
                            &mut libraries,
                            &format!("{}-protocol", name),
                            Some(&str_replace("-", ".", &version_str)),
                            Some("AMQP protocol version"),
                            &[],
                            &[],
                        )?;
                    }
                }

                "bz2" => {
                    let info = self.runtime.get_extension_info(name)?;

                    // BZip2 Version => 1.0.6, 6-Sept-2010
                    let mut matches: IndexMap<CaptureKey, String> = IndexMap::new();
                    if Preg::is_match3(
                        php_regex!("/^BZip2 Version => (?<version>.*),/im"),
                        &info,
                        Some(&mut matches),
                    ) {
                        self.add_library(
                            &mut libraries,
                            name,
                            matches
                                .get(&CaptureKey::ByName("version".to_string()))
                                .map(|s| s.as_str()),
                            None,
                            &[],
                            &[],
                        )?;
                    }
                }

                "curl" => {
                    let curl_version = self
                        .runtime
                        .invoke(PhpMixed::String("curl_version".to_string()), vec![]);
                    let curl_version_str = curl_version
                        .as_array()
                        .and_then(|m| m.get("version"))
                        .and_then(|v| v.as_string())
                        .map(|s| s.to_string());
                    if let Some(cv) = curl_version_str.as_deref() {
                        self.add_library(&mut libraries, name, Some(cv), None, &[], &[])?;
                    }

                    let info = self.runtime.get_extension_info(name)?;

                    // SSL Version => OpenSSL/1.0.1t
                    let mut ssl_matches: IndexMap<CaptureKey, String> = IndexMap::new();
                    if Preg::is_match3(
                        php_regex!("{^SSL Version => (?<library>[^/]+)/(?<version>.+)$}im"),
                        &info,
                        Some(&mut ssl_matches),
                    ) {
                        let ssl_library_raw = ssl_matches
                            .get(&CaptureKey::ByName("library".to_string()))
                            .cloned()
                            .unwrap_or_default();
                        let ssl_version = ssl_matches
                            .get(&CaptureKey::ByName("version".to_string()))
                            .cloned()
                            .unwrap_or_default();
                        let library = strtolower(&ssl_library_raw);
                        if library == "openssl" {
                            let mut is_fips = false;
                            let parsed_version = Version::parse_openssl(&ssl_version, &mut is_fips)
                                .unwrap_or_default();
                            let fips_provides: Vec<String> = if is_fips {
                                vec!["curl-openssl".to_string()]
                            } else {
                                Vec::new()
                            };
                            self.add_library(
                                &mut libraries,
                                &format!("{}-openssl{}", name, if is_fips { "-fips" } else { "" }),
                                Some(&parsed_version),
                                Some(&format!("curl OpenSSL version ({})", parsed_version)),
                                &[],
                                &fips_provides,
                            )?;
                        } else {
                            let (shortlib, ssl_lib);
                            if str_starts_with(&library, "(securetransport)") {
                                let mut securetransport_matches: IndexMap<CaptureKey, String> =
                                    IndexMap::new();
                                if Preg::is_match3(
                                    php_regex!("{^\\(securetransport\\) ([a-z0-9]+)}"),
                                    &library,
                                    Some(&mut securetransport_matches),
                                ) {
                                    shortlib = "securetransport".to_string();
                                    let m1 = securetransport_matches
                                        .get(&CaptureKey::ByIndex(1))
                                        .cloned()
                                        .unwrap_or_default();
                                    ssl_lib = format!("curl-{}", m1);
                                } else {
                                    shortlib = library.clone();
                                    ssl_lib = "curl-openssl".to_string();
                                }
                            } else {
                                shortlib = library.clone();
                                ssl_lib = "curl-openssl".to_string();
                            }
                            self.add_library(
                                &mut libraries,
                                &format!("{}-{}", name, shortlib),
                                Some(&ssl_version),
                                Some(&format!("curl {} version ({})", library, ssl_version)),
                                &[ssl_lib],
                                &[],
                            )?;
                        }
                    }

                    // libSSH Version => libssh2/1.4.3
                    let mut ssh_matches: IndexMap<CaptureKey, String> = IndexMap::new();
                    if Preg::is_match3(
                        php_regex!(
                            "{^libSSH Version => (?<library>[^/]+)/(?<version>.+?)(?:/.*)?$}im"
                        ),
                        &info,
                        Some(&mut ssh_matches),
                    ) {
                        let ssh_library = ssh_matches
                            .get(&CaptureKey::ByName("library".to_string()))
                            .cloned()
                            .unwrap_or_default();
                        let ssh_version = ssh_matches
                            .get(&CaptureKey::ByName("version".to_string()))
                            .cloned()
                            .unwrap_or_default();
                        self.add_library(
                            &mut libraries,
                            &format!("{}-{}", name, strtolower(&ssh_library)),
                            Some(&ssh_version),
                            Some(&format!("curl {} version", &ssh_library)),
                            &[],
                            &[],
                        )?;
                    }

                    // ZLib Version => 1.2.8
                    let mut zlib_matches: IndexMap<CaptureKey, String> = IndexMap::new();
                    if Preg::is_match3(
                        php_regex!("{^ZLib Version => (?<version>.+)$}im"),
                        &info,
                        Some(&mut zlib_matches),
                    ) {
                        self.add_library(
                            &mut libraries,
                            &format!("{}-zlib", name),
                            zlib_matches
                                .get(&CaptureKey::ByName("version".to_string()))
                                .map(|s| s.as_str()),
                            Some("curl zlib version"),
                            &[],
                            &[],
                        )?;
                    }
                }

                "date" => {
                    let info = self.runtime.get_extension_info(name)?;

                    // timelib version => 2018.03
                    let mut timelib_matches: IndexMap<CaptureKey, String> = IndexMap::new();
                    if Preg::is_match3(
                        php_regex!("/^timelib version => (?<version>.+)$/im"),
                        &info,
                        Some(&mut timelib_matches),
                    ) {
                        self.add_library(
                            &mut libraries,
                            &format!("{}-timelib", name),
                            timelib_matches
                                .get(&CaptureKey::ByName("version".to_string()))
                                .map(|s| s.as_str()),
                            Some("date timelib version"),
                            &[],
                            &[],
                        )?;
                    }

                    // Timezone Database => internal
                    let mut zoneinfo_source_matches: IndexMap<CaptureKey, String> = IndexMap::new();
                    if Preg::is_match3(
                        php_regex!("/^Timezone Database => (?<source>internal|external)$/im"),
                        &info,
                        Some(&mut zoneinfo_source_matches),
                    ) {
                        let external = zoneinfo_source_matches
                            .get(&CaptureKey::ByName("source".to_string()))
                            .map(|s| s == "external")
                            .unwrap_or(false);
                        let mut zoneinfo_matches: IndexMap<CaptureKey, String> = IndexMap::new();
                        if Preg::is_match3(
                            php_regex!(
                                "/^\"Olson\" Timezone Database Version => (?<version>.+?)(?:\\.system)?$/im"
                            ),
                            &info,
                            Some(&mut zoneinfo_matches),
                        ) {
                            let zoneinfo_version = zoneinfo_matches
                                .get(&CaptureKey::ByName("version".to_string()))
                                .cloned()
                                .unwrap_or_default();
                            // If the timezonedb is provided by ext/timezonedb, register that version as a replacement
                            if external && loaded_extensions.iter().any(|n| n == "timezonedb") {
                                self.add_library(
                                    &mut libraries,
                                    "timezonedb-zoneinfo",
                                    Some(&zoneinfo_version),
                                    Some(
                                        "zoneinfo (\"Olson\") database for date (replaced by timezonedb)",
                                    ),
                                    &[format!("{}-zoneinfo", name)],
                                    &[],
                                )?;
                            } else {
                                self.add_library(
                                    &mut libraries,
                                    &format!("{}-zoneinfo", name),
                                    Some(&zoneinfo_version),
                                    Some("zoneinfo (\"Olson\") database for date"),
                                    &[],
                                    &[],
                                )?;
                            }
                        }
                    }
                }

                "fileinfo" => {
                    let info = self.runtime.get_extension_info(name)?;

                    // libmagic => 537
                    let mut magic_matches: IndexMap<CaptureKey, String> = IndexMap::new();
                    if Preg::is_match3(
                        php_regex!("/^libmagic => (?<version>.+)$/im"),
                        &info,
                        Some(&mut magic_matches),
                    ) {
                        self.add_library(
                            &mut libraries,
                            &format!("{}-libmagic", name),
                            magic_matches
                                .get(&CaptureKey::ByName("version".to_string()))
                                .map(|s| s.as_str()),
                            Some("fileinfo libmagic version"),
                            &[],
                            &[],
                        )?;
                    }
                }

                "gd" => {
                    let gd_version = self.runtime.get_constant("GD_VERSION", None);
                    let gd_version_str = match &gd_version {
                        PhpMixed::String(s) => Some(s.clone()),
                        _ => None,
                    };
                    self.add_library(
                        &mut libraries,
                        name,
                        gd_version_str.as_deref(),
                        None,
                        &[],
                        &[],
                    )?;

                    let info = self.runtime.get_extension_info(name)?;

                    let mut libjpeg_matches: IndexMap<CaptureKey, String> = IndexMap::new();
                    if Preg::is_match3(
                        php_regex!("/^libJPEG Version => (?<version>.+?)(?: compatible)?$/im"),
                        &info,
                        Some(&mut libjpeg_matches),
                    ) {
                        let libjpeg_version = libjpeg_matches
                            .get(&CaptureKey::ByName("version".to_string()))
                            .cloned()
                            .unwrap_or_default();
                        let parsed = Version::parse_libjpeg(&libjpeg_version).unwrap_or_default();
                        self.add_library(
                            &mut libraries,
                            &format!("{}-libjpeg", name),
                            Some(&parsed),
                            Some("libjpeg version for gd"),
                            &[],
                            &[],
                        )?;
                    }

                    let mut libpng_matches: IndexMap<CaptureKey, String> = IndexMap::new();
                    if Preg::is_match3(
                        php_regex!("/^libPNG Version => (?<version>.+)$/im"),
                        &info,
                        Some(&mut libpng_matches),
                    ) {
                        self.add_library(
                            &mut libraries,
                            &format!("{}-libpng", name),
                            libpng_matches
                                .get(&CaptureKey::ByName("version".to_string()))
                                .map(|s| s.as_str()),
                            Some("libpng version for gd"),
                            &[],
                            &[],
                        )?;
                    }

                    let mut freetype_matches: IndexMap<CaptureKey, String> = IndexMap::new();
                    if Preg::is_match3(
                        php_regex!("/^FreeType Version => (?<version>.+)$/im"),
                        &info,
                        Some(&mut freetype_matches),
                    ) {
                        self.add_library(
                            &mut libraries,
                            &format!("{}-freetype", name),
                            freetype_matches
                                .get(&CaptureKey::ByName("version".to_string()))
                                .map(|s| s.as_str()),
                            Some("freetype version for gd"),
                            &[],
                            &[],
                        )?;
                    }

                    let mut libxpm_matches: IndexMap<CaptureKey, String> = IndexMap::new();
                    if Preg::is_match3(
                        php_regex!("/^libXpm Version => (?<versionId>\\d+)$/im"),
                        &info,
                        Some(&mut libxpm_matches),
                    ) {
                        let version_id: i64 = libxpm_matches
                            .get(&CaptureKey::ByName("versionId".to_string()))
                            .and_then(|s| s.parse().ok())
                            .unwrap_or(0);
                        let converted = Version::convert_libxpm_version_id(version_id);
                        self.add_library(
                            &mut libraries,
                            &format!("{}-libxpm", name),
                            Some(&converted),
                            Some("libxpm version for gd"),
                            &[],
                            &[],
                        )?;
                    }
                }

                "gmp" => {
                    let gmp_version = self.runtime.get_constant("GMP_VERSION", None);
                    let gmp_version_str = match &gmp_version {
                        PhpMixed::String(s) => Some(s.clone()),
                        _ => None,
                    };
                    self.add_library(
                        &mut libraries,
                        name,
                        gmp_version_str.as_deref(),
                        None,
                        &[],
                        &[],
                    )?;
                }

                "iconv" => {
                    let iconv_version = self.runtime.get_constant("ICONV_VERSION", None);
                    let iconv_version_str = match &iconv_version {
                        PhpMixed::String(s) => Some(s.clone()),
                        _ => None,
                    };
                    self.add_library(
                        &mut libraries,
                        name,
                        iconv_version_str.as_deref(),
                        None,
                        &[],
                        &[],
                    )?;
                }

                "intl" => {
                    let info = self.runtime.get_extension_info(name)?;

                    let description = "The ICU unicode and globalization support library";
                    // Truthy check is for testing only so we can make the condition fail
                    if self.runtime.has_constant("INTL_ICU_VERSION", None) {
                        let intl_icu_version = self.runtime.get_constant("INTL_ICU_VERSION", None);
                        let intl_icu_str = match &intl_icu_version {
                            PhpMixed::String(s) => Some(s.clone()),
                            _ => None,
                        };
                        self.add_library(
                            &mut libraries,
                            "icu",
                            intl_icu_str.as_deref(),
                            Some(description),
                            &[],
                            &[],
                        )?;
                    } else {
                        let mut matches: IndexMap<CaptureKey, String> = IndexMap::new();
                        if Preg::is_match3(
                            php_regex!("/^ICU version => (?<version>.+)$/im"),
                            &info,
                            Some(&mut matches),
                        ) {
                            self.add_library(
                                &mut libraries,
                                "icu",
                                matches
                                    .get(&CaptureKey::ByName("version".to_string()))
                                    .map(|s| s.as_str()),
                                Some(description),
                                &[],
                                &[],
                            )?;
                        }
                    }

                    // ICU TZData version => 2019c
                    let mut zoneinfo_matches: IndexMap<CaptureKey, String> = IndexMap::new();
                    if Preg::is_match3(
                        php_regex!("/^ICU TZData version => (?<version>.*)$/im"),
                        &info,
                        Some(&mut zoneinfo_matches),
                    ) {
                        let zi_version = zoneinfo_matches
                            .get(&CaptureKey::ByName("version".to_string()))
                            .cloned()
                            .unwrap_or_default();
                        if let Some(parsed) = Version::parse_zoneinfo_version(&zi_version) {
                            self.add_library(
                                &mut libraries,
                                "icu-zoneinfo",
                                Some(&parsed),
                                Some("zoneinfo (\"Olson\") database for icu"),
                                &[],
                                &[],
                            )?;
                        }
                    }

                    // Add a separate version for the CLDR library version
                    if self.runtime.has_class("ResourceBundle") {
                        let resource_bundle = self.runtime.invoke(
                            PhpMixed::List(vec![
                                PhpMixed::String("ResourceBundle".to_string()),
                                PhpMixed::String("create".to_string()),
                            ]),
                            vec![
                                PhpMixed::String("root".to_string()),
                                PhpMixed::String("ICUDATA".to_string()),
                                PhpMixed::Bool(false),
                            ],
                        );
                        if !matches!(resource_bundle, PhpMixed::Null) {
                            // TODO(plugin): `$resourceBundle->get('Version')` dynamic method call
                            let version_value =
                                Self::resource_bundle_get(&resource_bundle, "Version");
                            let version_str = match version_value {
                                PhpMixed::String(s) => Some(s),
                                _ => None,
                            };
                            self.add_library(
                                &mut libraries,
                                "icu-cldr",
                                version_str.as_deref(),
                                Some("ICU CLDR project version"),
                                &[],
                                &[],
                            )?;
                        }
                    }

                    if self.runtime.has_class("IntlChar") {
                        let intl_char_versions = self.runtime.invoke(
                            PhpMixed::List(vec![
                                PhpMixed::String("IntlChar".to_string()),
                                PhpMixed::String("getUnicodeVersion".to_string()),
                            ]),
                            vec![],
                        );
                        let sliced =
                            shirabe_php_shim::array_slice_mixed(&intl_char_versions, 0, Some(3));
                        let joined = implode(".", &Self::php_array_to_string_vec(&sliced));
                        self.add_library(
                            &mut libraries,
                            "icu-unicode",
                            Some(&joined),
                            Some("ICU unicode version"),
                            &[],
                            &[],
                        )?;
                    }
                }

                "imagick" => {
                    let image_magick_version = self.runtime.construct("Imagick", Vec::new())?;
                    // TODO(plugin): `->getVersion()` is a dynamic method call on Imagick
                    let image_magick_version_str =
                        Self::imagick_get_version_string(&image_magick_version);
                    // 6.x: ImageMagick 6.2.9 08/24/06 Q16 http://www.imagemagick.org
                    // 7.x: ImageMagick 7.0.8-34 Q16 x86_64 2019-03-23 https://imagemagick.org
                    let mut matches: IndexMap<CaptureKey, String> = IndexMap::new();
                    if Preg::is_match3(
                        php_regex!("/^ImageMagick (?<version>[\\d.]+)(?:-(?<patch>\\d+))?/"),
                        &image_magick_version_str,
                        Some(&mut matches),
                    ) {
                        let mut version_built = matches
                            .get(&CaptureKey::ByName("version".to_string()))
                            .cloned()
                            .unwrap_or_default();
                        if let Some(patch) = matches.get(&CaptureKey::ByName("patch".to_string())) {
                            version_built = format!("{}.{}", version_built, patch);
                        }

                        self.add_library(
                            &mut libraries,
                            &format!("{}-imagemagick", name),
                            Some(&version_built),
                            None,
                            &["imagick".to_string()],
                            &[],
                        )?;
                    }
                }

                "ldap" => {
                    let info = self.runtime.get_extension_info(name)?;

                    let mut matches: IndexMap<CaptureKey, String> = IndexMap::new();
                    let mut vendor_matches: IndexMap<CaptureKey, String> = IndexMap::new();
                    if Preg::is_match3(
                        php_regex!("/^Vendor Version => (?<versionId>\\d+)$/im"),
                        &info,
                        Some(&mut matches),
                    ) && Preg::is_match3(
                        php_regex!("/^Vendor Name => (?<vendor>.+)$/im"),
                        &info,
                        Some(&mut vendor_matches),
                    ) {
                        let version_id: i64 = matches
                            .get(&CaptureKey::ByName("versionId".to_string()))
                            .and_then(|s| s.parse().ok())
                            .unwrap_or(0);
                        let converted = Version::convert_openldap_version_id(version_id);
                        let vendor = vendor_matches
                            .get(&CaptureKey::ByName("vendor".to_string()))
                            .cloned()
                            .unwrap_or_default();
                        self.add_library(
                            &mut libraries,
                            &format!("{}-{}", name, strtolower(&vendor)),
                            Some(&converted),
                            Some(&format!("{} version of ldap", vendor)),
                            &[],
                            &[],
                        )?;
                    }
                }

                "libxml" => {
                    // ext/dom, ext/simplexml, ext/xmlreader and ext/xmlwriter use the same libxml as the ext/libxml
                    let target_exts = ["dom", "simplexml", "xml", "xmlreader", "xmlwriter"];
                    let intersected: Vec<String> = loaded_extensions
                        .iter()
                        .filter(|e| target_exts.contains(&e.as_str()))
                        .cloned()
                        .collect();
                    let libxml_provides: Vec<String> =
                        array_map_str_fn(|extension| format!("{}-libxml", extension), &intersected);
                    let libxml_dotted = self.runtime.get_constant("LIBXML_DOTTED_VERSION", None);
                    let libxml_dotted_str = match &libxml_dotted {
                        PhpMixed::String(s) => Some(s.clone()),
                        _ => None,
                    };
                    self.add_library(
                        &mut libraries,
                        name,
                        libxml_dotted_str.as_deref(),
                        Some("libxml library version"),
                        &[],
                        &libxml_provides,
                    )?;
                }

                "mbstring" => {
                    let info = self.runtime.get_extension_info(name)?;

                    // libmbfl version => 1.3.2
                    let mut libmbfl_matches: IndexMap<CaptureKey, String> = IndexMap::new();
                    if Preg::is_match3(
                        php_regex!("/^libmbfl version => (?<version>.+)$/im"),
                        &info,
                        Some(&mut libmbfl_matches),
                    ) {
                        self.add_library(
                            &mut libraries,
                            &format!("{}-libmbfl", name),
                            libmbfl_matches
                                .get(&CaptureKey::ByName("version".to_string()))
                                .map(|s| s.as_str()),
                            Some("mbstring libmbfl version"),
                            &[],
                            &[],
                        )?;
                    }

                    if self.runtime.has_constant("MB_ONIGURUMA_VERSION", None) {
                        let oniguruma = self.runtime.get_constant("MB_ONIGURUMA_VERSION", None);
                        let oniguruma_str = match &oniguruma {
                            PhpMixed::String(s) => Some(s.clone()),
                            _ => None,
                        };
                        self.add_library(
                            &mut libraries,
                            &format!("{}-oniguruma", name),
                            oniguruma_str.as_deref(),
                            Some("mbstring oniguruma version"),
                            &[],
                            &[],
                        )?;

                    // Multibyte regex (oniguruma) version => 5.9.5
                    // oniguruma version => 6.9.0
                    } else {
                        let mut oniguruma_matches: IndexMap<CaptureKey, String> = IndexMap::new();
                        if Preg::is_match3(
                            php_regex!(
                                "/^(?:oniguruma|Multibyte regex \\(oniguruma\\)) version => (?<version>.+)$/im"
                            ),
                            &info,
                            Some(&mut oniguruma_matches),
                        ) {
                            self.add_library(
                                &mut libraries,
                                &format!("{}-oniguruma", name),
                                oniguruma_matches
                                    .get(&CaptureKey::ByName("version".to_string()))
                                    .map(|s| s.as_str()),
                                Some("mbstring oniguruma version"),
                                &[],
                                &[],
                            )?;
                        }
                    }
                }

                "memcached" => {
                    let info = self.runtime.get_extension_info(name)?;

                    // libmemcached version => 1.0.18
                    let mut matches: IndexMap<CaptureKey, String> = IndexMap::new();
                    if Preg::is_match3(
                        php_regex!("/^libmemcached version => (?<version>.+)$/im"),
                        &info,
                        Some(&mut matches),
                    ) {
                        self.add_library(
                            &mut libraries,
                            &format!("{}-libmemcached", name),
                            matches
                                .get(&CaptureKey::ByName("version".to_string()))
                                .map(|s| s.as_str()),
                            Some("libmemcached version"),
                            &[],
                            &[],
                        )?;
                    }
                }

                "openssl" => {
                    let openssl_text = self.runtime.get_constant("OPENSSL_VERSION_TEXT", None);
                    let openssl_text_str = match &openssl_text {
                        PhpMixed::String(s) => s.clone(),
                        _ => "".to_string(),
                    };
                    // OpenSSL 1.1.1g  21 Apr 2020
                    let mut matches: IndexMap<CaptureKey, String> = IndexMap::new();
                    if Preg::is_match3(
                        php_regex!("{^(?:OpenSSL|LibreSSL)?\\s*(?<version>\\S+)}i"),
                        &openssl_text_str,
                        Some(&mut matches),
                    ) {
                        let version = matches
                            .get(&CaptureKey::ByName("version".to_string()))
                            .cloned()
                            .unwrap_or_default();
                        let mut is_fips = false;
                        let parsed_version =
                            Version::parse_openssl(&version, &mut is_fips).unwrap_or_default();
                        let mut provides_list: Vec<String> = Vec::new();
                        if is_fips {
                            provides_list.push(name.to_string());
                        }
                        self.add_library(
                            &mut libraries,
                            &format!("{}{}", name, if is_fips { "-fips" } else { "" }),
                            Some(&parsed_version),
                            Some(&openssl_text_str),
                            &[],
                            &provides_list,
                        )?;
                    }
                }

                "pcre" => {
                    let pcre_version = self.runtime.get_constant("PCRE_VERSION", None);
                    let pcre_version_str = match &pcre_version {
                        PhpMixed::String(s) => s.clone(),
                        _ => "".to_string(),
                    };
                    let stripped =
                        Preg::replace(php_regex!("{^(\\S+).*}"), "$1", &pcre_version_str);
                    self.add_library(&mut libraries, name, Some(&stripped), None, &[], &[])?;

                    let info = self.runtime.get_extension_info(name)?;

                    // PCRE Unicode Version => 12.1.0
                    let mut pcre_unicode_matches: IndexMap<CaptureKey, String> = IndexMap::new();
                    if Preg::is_match3(
                        php_regex!("/^PCRE Unicode Version => (?<version>.+)$/im"),
                        &info,
                        Some(&mut pcre_unicode_matches),
                    ) {
                        self.add_library(
                            &mut libraries,
                            &format!("{}-unicode", name),
                            pcre_unicode_matches
                                .get(&CaptureKey::ByName("version".to_string()))
                                .map(|s| s.as_str()),
                            Some("PCRE Unicode version support"),
                            &[],
                            &[],
                        )?;
                    }
                }

                "mysqlnd" | "pdo_mysql" => {
                    let info = self.runtime.get_extension_info(name)?;

                    let mut matches: IndexMap<CaptureKey, String> = IndexMap::new();
                    if Preg::is_match3(
                        php_regex!(
                            "/^(?:Client API version|Version) => mysqlnd (?<version>.+?) /mi"
                        ),
                        &info,
                        Some(&mut matches),
                    ) {
                        self.add_library(
                            &mut libraries,
                            &format!("{}-mysqlnd", name),
                            matches
                                .get(&CaptureKey::ByName("version".to_string()))
                                .map(|s| s.as_str()),
                            Some(&format!("mysqlnd library version for {}", name)),
                            &[],
                            &[],
                        )?;
                    }
                }

                "mongodb" => {
                    let info = self.runtime.get_extension_info(name)?;

                    let mut libmongoc_matches: IndexMap<CaptureKey, String> = IndexMap::new();
                    if Preg::is_match3(
                        php_regex!("/^libmongoc bundled version => (?<version>.+)$/im"),
                        &info,
                        Some(&mut libmongoc_matches),
                    ) {
                        self.add_library(
                            &mut libraries,
                            &format!("{}-libmongoc", name),
                            libmongoc_matches
                                .get(&CaptureKey::ByName("version".to_string()))
                                .map(|s| s.as_str()),
                            Some("libmongoc version of mongodb"),
                            &[],
                            &[],
                        )?;
                    }

                    let mut libbson_matches: IndexMap<CaptureKey, String> = IndexMap::new();
                    if Preg::is_match3(
                        php_regex!("/^libbson bundled version => (?<version>.+)$/im"),
                        &info,
                        Some(&mut libbson_matches),
                    ) {
                        self.add_library(
                            &mut libraries,
                            &format!("{}-libbson", name),
                            libbson_matches
                                .get(&CaptureKey::ByName("version".to_string()))
                                .map(|s| s.as_str()),
                            Some("libbson version of mongodb"),
                            &[],
                            &[],
                        )?;
                    }
                }

                "pgsql" => {
                    if self.runtime.has_constant("PGSQL_LIBPQ_VERSION", None) {
                        let pq_version = self.runtime.get_constant("PGSQL_LIBPQ_VERSION", None);
                        let pq_version_str = match &pq_version {
                            PhpMixed::String(s) => Some(s.clone()),
                            _ => None,
                        };
                        self.add_library(
                            &mut libraries,
                            "pgsql-libpq",
                            pq_version_str.as_deref(),
                            Some("libpq for pgsql"),
                            &[],
                            &[],
                        )?;
                    } else {
                        // intentional fall-through to next case...
                        let info = self.runtime.get_extension_info(name)?;

                        let mut matches: IndexMap<CaptureKey, String> = IndexMap::new();
                        if Preg::is_match3(
                            php_regex!("/^PostgreSQL\\(libpq\\) Version => (?<version>.*)$/im"),
                            &info,
                            Some(&mut matches),
                        ) {
                            self.add_library(
                                &mut libraries,
                                &format!("{}-libpq", name),
                                matches
                                    .get(&CaptureKey::ByName("version".to_string()))
                                    .map(|s| s.as_str()),
                                Some(&format!("libpq for {}", name)),
                                &[],
                                &[],
                            )?;
                        }
                    }
                }

                "pdo_pgsql" => {
                    let info = self.runtime.get_extension_info(name)?;

                    let mut matches: IndexMap<CaptureKey, String> = IndexMap::new();
                    if Preg::is_match3(
                        php_regex!("/^PostgreSQL\\(libpq\\) Version => (?<version>.*)$/im"),
                        &info,
                        Some(&mut matches),
                    ) {
                        self.add_library(
                            &mut libraries,
                            &format!("{}-libpq", name),
                            matches
                                .get(&CaptureKey::ByName("version".to_string()))
                                .map(|s| s.as_str()),
                            Some(&format!("libpq for {}", name)),
                            &[],
                            &[],
                        )?;
                    }
                }

                "pq" => {
                    let info = self.runtime.get_extension_info(name)?;

                    // Used Library => Compiled => Linked
                    // libpq => 14.3 (Ubuntu 14.3-1.pgdg22.04+1) => 15.0.2
                    let mut matches: IndexMap<CaptureKey, String> = IndexMap::new();
                    if Preg::is_match3(
                        php_regex!("/^libpq => (?<compiled>.+) => (?<linked>.+)$/im"),
                        &info,
                        Some(&mut matches),
                    ) {
                        self.add_library(
                            &mut libraries,
                            &format!("{}-libpq", name),
                            matches
                                .get(&CaptureKey::ByName("linked".to_string()))
                                .map(|s| s.as_str()),
                            Some(&format!("libpq for {}", name)),
                            &[],
                            &[],
                        )?;
                    }
                }

                "rdkafka" => {
                    if self.runtime.has_constant("RD_KAFKA_VERSION", None) {
                        // Interpreted as hex MM.mm.rr.xx:
                        //  - MM = Major
                        //  - mm = minor
                        //  - rr = revision
                        //  - xx = pre-release id (0xff is the final release)
                        //
                        // pre-release ID in practice is always 0xff even for RCs etc, so we ignore it
                        let lib_rd_kafka_version_int = self
                            .runtime
                            .get_constant("RD_KAFKA_VERSION", None)
                            .as_int()
                            .unwrap_or(0);
                        let version_built = format!(
                            "{}.{}.{}",
                            (lib_rd_kafka_version_int & 0x7F000000) >> 24,
                            (lib_rd_kafka_version_int & 0x00FF0000) >> 16,
                            (lib_rd_kafka_version_int & 0x0000FF00) >> 8,
                        );
                        self.add_library(
                            &mut libraries,
                            &format!("{}-librdkafka", name),
                            Some(&version_built),
                            Some(&format!("librdkafka for {}", name)),
                            &[],
                            &[],
                        )?;
                    }
                }

                "libsodium" | "sodium" => {
                    if self.runtime.has_constant("SODIUM_LIBRARY_VERSION", None) {
                        let sodium = self.runtime.get_constant("SODIUM_LIBRARY_VERSION", None);
                        let sodium_str = match &sodium {
                            PhpMixed::String(s) => Some(s.clone()),
                            _ => None,
                        };
                        self.add_library(
                            &mut libraries,
                            "libsodium",
                            sodium_str.as_deref(),
                            None,
                            &[],
                            &[],
                        )?;
                        self.add_library(
                            &mut libraries,
                            "libsodium",
                            sodium_str.as_deref(),
                            None,
                            &[],
                            &[],
                        )?;
                    }
                }

                "sqlite3" | "pdo_sqlite" => {
                    let info = self.runtime.get_extension_info(name)?;

                    let mut matches: IndexMap<CaptureKey, String> = IndexMap::new();
                    if Preg::is_match3(
                        php_regex!("/^SQLite Library => (?<version>.+)$/im"),
                        &info,
                        Some(&mut matches),
                    ) {
                        self.add_library(
                            &mut libraries,
                            &format!("{}-sqlite", name),
                            matches
                                .get(&CaptureKey::ByName("version".to_string()))
                                .map(|s| s.as_str()),
                            None,
                            &[],
                            &[],
                        )?;
                    }
                }

                "ssh2" => {
                    let info = self.runtime.get_extension_info(name)?;

                    let mut matches: IndexMap<CaptureKey, String> = IndexMap::new();
                    if Preg::is_match3(
                        php_regex!("/^libssh2 version => (?<version>.+)$/im"),
                        &info,
                        Some(&mut matches),
                    ) {
                        self.add_library(
                            &mut libraries,
                            &format!("{}-libssh2", name),
                            matches
                                .get(&CaptureKey::ByName("version".to_string()))
                                .map(|s| s.as_str()),
                            None,
                            &[],
                            &[],
                        )?;
                    }
                }

                "xsl" => {
                    let libxslt_version = self.runtime.get_constant("LIBXSLT_DOTTED_VERSION", None);
                    let libxslt_str = match &libxslt_version {
                        PhpMixed::String(s) => Some(s.clone()),
                        _ => None,
                    };
                    self.add_library(
                        &mut libraries,
                        "libxslt",
                        libxslt_str.as_deref(),
                        None,
                        &[],
                        &["xsl".to_string()],
                    )?;

                    let info = self.runtime.get_extension_info("xsl")?;
                    let mut matches: IndexMap<CaptureKey, String> = IndexMap::new();
                    if Preg::is_match3(
                        php_regex!(
                            "/^libxslt compiled against libxml Version => (?<version>.+)$/im"
                        ),
                        &info,
                        Some(&mut matches),
                    ) {
                        self.add_library(
                            &mut libraries,
                            "libxslt-libxml",
                            matches
                                .get(&CaptureKey::ByName("version".to_string()))
                                .map(|s| s.as_str()),
                            Some("libxml version libxslt is compiled against"),
                            &[],
                            &[],
                        )?;
                    }
                }

                "yaml" => {
                    let info = self.runtime.get_extension_info("yaml")?;

                    let mut matches: IndexMap<CaptureKey, String> = IndexMap::new();
                    if Preg::is_match3(
                        php_regex!("/^LibYAML Version => (?<version>.+)$/im"),
                        &info,
                        Some(&mut matches),
                    ) {
                        self.add_library(
                            &mut libraries,
                            &format!("{}-libyaml", name),
                            matches
                                .get(&CaptureKey::ByName("version".to_string()))
                                .map(|s| s.as_str()),
                            Some("libyaml version of yaml"),
                            &[],
                            &[],
                        )?;
                    }
                }

                "zip" => {
                    if self
                        .runtime
                        .has_constant("LIBZIP_VERSION", Some("ZipArchive".to_string()))
                    {
                        let libzip = self
                            .runtime
                            .get_constant("LIBZIP_VERSION", Some("ZipArchive".to_string()));
                        let libzip_str = match &libzip {
                            PhpMixed::String(s) => Some(s.clone()),
                            _ => None,
                        };
                        self.add_library(
                            &mut libraries,
                            &format!("{}-libzip", name),
                            libzip_str.as_deref(),
                            None,
                            &[],
                            &["zip".to_string()],
                        )?;
                    }
                }

                "zlib" => {
                    if self.runtime.has_constant("ZLIB_VERSION", None) {
                        let zlib = self.runtime.get_constant("ZLIB_VERSION", None);
                        let zlib_str = match &zlib {
                            PhpMixed::String(s) => Some(s.clone()),
                            _ => None,
                        };
                        self.add_library(
                            &mut libraries,
                            name,
                            zlib_str.as_deref(),
                            None,
                            &[],
                            &[],
                        )?;

                    // Linked Version => 1.2.8
                    } else {
                        let info = self.runtime.get_extension_info(name)?;
                        let mut matches: IndexMap<CaptureKey, String> = IndexMap::new();
                        if Preg::is_match3(
                            php_regex!("/^Linked Version => (?<version>.+)$/im"),
                            &info,
                            Some(&mut matches),
                        ) {
                            self.add_library(
                                &mut libraries,
                                name,
                                matches
                                    .get(&CaptureKey::ByName("version".to_string()))
                                    .map(|s| s.as_str()),
                                None,
                                &[],
                                &[],
                            )?;
                        }
                    }
                }

                _ => {}
            }
        }

        let hhvm_version = self.hhvm_detector.get_version();
        if let Some(hhvm_version) = hhvm_version {
            let (pretty_version, version);
            match self
                .version_parser
                .as_ref()
                .unwrap()
                .normalize(&hhvm_version, None)
            {
                Ok(v) => {
                    pretty_version = hhvm_version.clone();
                    version = v;
                }
                Err(_) => {
                    pretty_version =
                        Preg::replace(php_regex!("#^([^~+-]+).*$#"), "$1", &hhvm_version);
                    version = self
                        .version_parser
                        .as_ref()
                        .unwrap()
                        .normalize(&pretty_version, None)?;
                }
            }

            let mut hhvm = CompletePackage::new("hhvm".to_string(), version, pretty_version);
            hhvm.set_description("The HHVM Runtime (64bit)".to_string());
            self.add_package(CompletePackageHandle::from_complete_package(hhvm).into())?;
        }
        Ok(())
    }

    pub fn add_package(&mut self, package: PackageInterfaceHandle) -> anyhow::Result<()> {
        if package.as_complete().is_none() {
            return Err(anyhow::anyhow!(UnexpectedValueException {
                message: format!(
                    "Expected CompletePackage but got {}",
                    get_class(&PhpMixed::Null)
                ),
                code: 0,
            }));
        }

        let name = package.get_name();

        // Skip if overridden
        if self.overrides.contains_key(&name) {
            if matches!(self.overrides[&name].version, PhpMixed::Bool(false)) {
                self.add_disabled_package_from_pkg(package);
                return Ok(());
            }

            let overrider = self.inner.find_package(
                &name,
                crate::repository::FindPackageConstraint::String("*".to_string()),
            )?;
            let actual_text = if let Some(ref ov) = overrider {
                if package.get_version() == ov.get_version() {
                    "same as actual".to_string()
                } else {
                    format!("actual: {}", package.get_pretty_version())
                }
            } else {
                format!("actual: {}", package.get_pretty_version())
            };
            if let Some(overrider) = overrider
                && let Some(overrider) = overrider.as_complete()
            {
                let description = overrider.get_description().unwrap_or_default();
                overrider.set_description(format!("{}, {}", description, actual_text));
            }

            return Ok(());
        }

        // Skip if PHP is overridden and we are adding a php-* package
        if self.overrides.contains_key("php") && strpos(&name, "php-") == Some(0) {
            let php_override = PlatformOverride {
                name: self.overrides["php"].name.clone(),
                version: self.overrides["php"].version.clone(),
            };
            let overrider =
                self.add_overridden_package(&php_override, Some(package.get_pretty_name()))?;
            let actual_text = if package.get_version() == overrider.get_version() {
                "same as actual".to_string()
            } else {
                format!("actual: {}", package.get_pretty_version())
            };
            let current_description = overrider.get_description().unwrap_or_default();
            overrider.set_description(format!("{}, {}", current_description, actual_text));

            return Ok(());
        }

        self.inner.add_package(package);
        Ok(())
    }

    fn add_overridden_package(
        &mut self,
        r#override: &PlatformOverride,
        name: Option<String>,
    ) -> anyhow::Result<CompletePackageHandle> {
        let version_str = match &r#override.version {
            PhpMixed::String(s) => s.clone(),
            _ => "".to_string(),
        };
        let version = self
            .version_parser
            .as_ref()
            .unwrap()
            .normalize(&version_str, None)?;
        let mut package = CompletePackage::new(
            name.unwrap_or_else(|| r#override.name.clone()),
            version,
            version_str,
        );
        package.set_description("Package overridden via config.platform".to_string());
        let mut extra: IndexMap<String, PhpMixed> = IndexMap::new();
        extra.insert("config.platform".to_string(), PhpMixed::Bool(true));
        package.inner.set_extra(extra);
        let package = CompletePackageHandle::from_complete_package(package);
        self.inner.add_package(package.clone().into())?;

        if package.get_name() == "php" {
            let parts = explode(".", &package.get_version());
            let head = array_slice_strs(&parts, 0, Some(3));
            *LAST_SEEN_PLATFORM_PHP.lock().unwrap() = Some(implode(".", &head));
        }

        Ok(package)
    }

    fn add_disabled_package_from_pkg(&mut self, package: PackageInterfaceHandle) {
        // PHP type-hints CompletePackage here; the handle is guaranteed complete by add_package.
        let complete = package
            .as_complete()
            .expect("addDisabledPackage expects a CompletePackage");
        self.add_disabled_package(complete);
    }

    fn add_disabled_package(&mut self, package: CompletePackageInterfaceHandle) {
        let current_description = package.get_description().unwrap_or_default();
        package.set_description(format!(
            "{}. <warning>Package disabled via config.platform</warning>",
            current_description
        ));
        let mut extra: IndexMap<String, PhpMixed> = IndexMap::new();
        extra.insert("config.platform".to_string(), PhpMixed::Bool(true));
        // NOTE(phase-c): neither PackageInterface nor CompletePackageInterface exposes
        // setExtra (PHP defines it on BasePackage), and the handle API does not surface
        // it. Disabled packages are always plain CompletePackage objects, so reach the
        // concrete Package through the shared Rc.
        match &mut *package.as_rc().borrow_mut() {
            crate::package::AnyPackage::CompletePackage(p) => p.inner.set_extra(extra),
            _ => unreachable!("disabled platform package must be a concrete CompletePackage"),
        }

        self.disabled_packages.insert(package.get_name(), package);
    }

    /// Parses the version and adds a new package to the repository
    fn add_extension(&mut self, name: &str, pretty_version: &str) -> anyhow::Result<()> {
        let mut extra_description: Option<String> = None;
        let mut pretty_version = pretty_version.to_string();

        let version = match self
            .version_parser
            .as_ref()
            .unwrap()
            .normalize(&pretty_version, None)
        {
            Ok(v) => v,
            Err(_) => {
                extra_description = Some(format!(" (actual version: {})", pretty_version));
                let mut m: IndexMap<CaptureKey, String> = IndexMap::new();
                if Preg::is_match3(
                    php_regex!("{^(\\d+\\.\\d+\\.\\d+(?:\\.\\d+)?)}"),
                    &pretty_version,
                    Some(&mut m),
                ) {
                    pretty_version = m.get(&CaptureKey::ByIndex(1)).cloned().unwrap_or_default();
                } else {
                    pretty_version = "0".to_string();
                }
                self.version_parser
                    .as_ref()
                    .unwrap()
                    .normalize(&pretty_version, None)?
            }
        };

        let package_name = self.build_package_name(name);
        let mut ext = CompletePackage::new(package_name, version.clone(), pretty_version);
        ext.set_description(format!(
            "The {} PHP extension{}",
            name,
            extra_description.unwrap_or_default()
        ));
        ext.inner.set_type("php-ext".to_string());

        if name == "uuid" {
            let mut replaces: IndexMap<String, Link> = IndexMap::new();
            replaces.insert(
                "lib-uuid".to_string(),
                Link::new(
                    "ext-uuid".to_string(),
                    "lib-uuid".to_string(),
                    SimpleConstraint::new("=".to_string(), version.to_string(), None).into(),
                    Some(Link::TYPE_REPLACE.to_string()),
                    ext.get_pretty_version().to_string(),
                ),
            );
            ext.inner.set_replaces(replaces);
        }

        self.add_package(CompletePackageHandle::from_complete_package(ext).into())?;
        Ok(())
    }

    fn build_package_name(&self, name: &str) -> String {
        format!("ext-{}", str_replace(" ", "-", &strtolower(name)))
    }

    fn add_library(
        &mut self,
        libraries: &mut IndexMap<String, bool>,
        name: &str,
        pretty_version: Option<&str>,
        description: Option<&str>,
        replaces: &[String],
        provides: &[String],
    ) -> anyhow::Result<()> {
        let pretty_version = match pretty_version {
            Some(v) => v,
            None => return Ok(()),
        };
        let version = match self
            .version_parser
            .as_ref()
            .unwrap()
            .normalize(pretty_version, None)
        {
            Ok(v) => v,
            Err(_) => return Ok(()),
        };

        // avoid adding the same lib twice even if two conflicting extensions provide the same lib
        // see https://github.com/composer/composer/issues/12082
        if libraries.contains_key(&format!("lib-{}", name)) {
            return Ok(());
        }
        libraries.insert(format!("lib-{}", name), true);

        let description = description
            .map(|s| s.to_string())
            .unwrap_or_else(|| format!("The {} library", name));

        let mut lib = CompletePackage::new(
            format!("lib-{}", name),
            version.clone(),
            pretty_version.to_string(),
        );
        lib.set_description(description);

        let mut replace_links: IndexMap<String, Link> = IndexMap::new();
        for replace in replaces {
            let replace_lower = strtolower(replace);
            replace_links.insert(
                replace_lower.clone(),
                Link::new(
                    format!("lib-{}", name),
                    format!("lib-{}", replace_lower),
                    SimpleConstraint::new("=".to_string(), version.to_string(), None).into(),
                    Some(Link::TYPE_REPLACE.to_string()),
                    lib.get_pretty_version().to_string(),
                ),
            );
        }
        let mut provide_links: IndexMap<String, Link> = IndexMap::new();
        for provide in provides {
            let provide_lower = strtolower(provide);
            provide_links.insert(
                provide_lower.clone(),
                Link::new(
                    format!("lib-{}", name),
                    format!("lib-{}", provide_lower),
                    SimpleConstraint::new("=".to_string(), version.to_string(), None).into(),
                    Some(Link::TYPE_PROVIDE.to_string()),
                    lib.get_pretty_version().to_string(),
                ),
            );
        }
        lib.inner.set_replaces(replace_links);
        lib.inner.set_provides(provide_links);

        self.add_package(CompletePackageHandle::from_complete_package(lib).into())?;
        Ok(())
    }

    /// Check if a package name is a platform package.
    pub fn is_platform_package(name: &str) -> bool {
        let mut cache = IS_PLATFORM_PACKAGE_CACHE.lock().unwrap();

        if let Some(&cached) = cache.get(name) {
            return cached;
        }

        let result = Preg::is_match(Self::PLATFORM_PACKAGE_REGEX, name);
        cache.insert(name.to_string(), result);
        result
    }

    /// Returns the last seen config.platform.php version if defined
    ///
    /// This is a best effort attempt for internal purposes, retrieve the real
    /// packages from a PlatformRepository instance if you need a version guaranteed to
    /// be correct.
    pub fn get_platform_php_version() -> Option<String> {
        LAST_SEEN_PLATFORM_PHP.lock().unwrap().clone()
    }

    pub fn search(
        &mut self,
        query: String,
        mode: i64,
        r#type: Option<String>,
    ) -> anyhow::Result<Vec<crate::repository::SearchResult>> {
        // suppress vendor search as there are no vendors to match in platform packages
        if mode == crate::repository::SEARCH_VENDOR {
            return Ok(Vec::new());
        }

        self.ensure_initialized()?;
        self.inner.search(query, mode, r#type)
    }

    fn is_complete_package(package: PackageInterfaceHandle) -> bool {
        package.as_complete().is_some()
    }

    fn resource_bundle_get(_value: &PhpMixed, _key: &str) -> PhpMixed {
        // TODO(plugin): proper ResourceBundle::get($key) dispatch on a PHP object.
        PhpMixed::Null
    }

    fn imagick_get_version_string(_value: &PhpMixed) -> String {
        // TODO(plugin): proper Imagick->getVersion()['versionString'] dispatch.
        "".to_string()
    }

    fn php_array_to_string_vec(value: &PhpMixed) -> Vec<String> {
        match value {
            PhpMixed::List(list) => list
                .iter()
                .map(|v| match v {
                    PhpMixed::String(s) => s.clone(),
                    PhpMixed::Int(i) => i.to_string(),
                    PhpMixed::Float(f) => f.to_string(),
                    _ => "".to_string(),
                })
                .collect(),
            PhpMixed::Array(m) => m
                .values()
                .map(|v| match v {
                    PhpMixed::String(s) => s.clone(),
                    PhpMixed::Int(i) => i.to_string(),
                    PhpMixed::Float(f) => f.to_string(),
                    _ => "".to_string(),
                })
                .collect(),
            _ => Vec::new(),
        }
    }
}

impl crate::repository::RepositoryInterface for PlatformRepository {
    fn count(&self) -> anyhow::Result<usize> {
        self.inner.count()
    }

    fn has_package(&self, package: PackageInterfaceHandle) -> bool {
        self.inner.has_package(package)
    }

    fn find_package(
        &mut self,
        name: &str,
        constraint: crate::repository::FindPackageConstraint,
    ) -> anyhow::Result<Option<crate::package::BasePackageHandle>> {
        self.ensure_initialized()?;
        self.inner.find_package(name, constraint)
    }

    fn find_packages(
        &mut self,
        name: &str,
        constraint: Option<crate::repository::FindPackageConstraint>,
    ) -> anyhow::Result<Vec<crate::package::BasePackageHandle>> {
        self.ensure_initialized()?;
        self.inner.find_packages(name, constraint)
    }

    fn get_packages(&mut self) -> anyhow::Result<Vec<crate::package::BasePackageHandle>> {
        self.ensure_initialized()?;
        self.inner.get_packages()
    }

    fn load_packages(
        &mut self,
        package_name_map: IndexMap<String, Option<shirabe_semver::constraint::AnyConstraint>>,
        acceptable_stabilities: IndexMap<String, i64>,
        stability_flags: IndexMap<String, i64>,
        already_loaded: IndexMap<String, IndexMap<String, crate::package::PackageInterfaceHandle>>,
    ) -> anyhow::Result<crate::repository::LoadPackagesResult> {
        self.ensure_initialized()?;
        self.inner.load_packages(
            package_name_map,
            acceptable_stabilities,
            stability_flags,
            already_loaded,
        )
    }

    fn search(
        &mut self,
        query: String,
        mode: i64,
        r#type: Option<String>,
    ) -> anyhow::Result<Vec<crate::repository::SearchResult>> {
        PlatformRepository::search(self, query, mode, r#type)
    }

    fn get_providers(
        &mut self,
        package_name: String,
    ) -> anyhow::Result<IndexMap<String, crate::repository::ProviderInfo>> {
        self.ensure_initialized()?;
        self.inner.get_providers(package_name)
    }

    fn get_repo_name(&self) -> String {
        PlatformRepository::get_repo_name(self)
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }

    fn set_self_handle(&self, weak: crate::repository::RepositoryInterfaceWeakHandle) {
        self.inner.set_self_handle(weak);
    }
}
