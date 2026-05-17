//! ref: composer/src/Composer/Repository/PlatformRepository.php

use std::sync::{LazyLock, Mutex};

use indexmap::IndexMap;

use shirabe_external_packages::composer::pcre::preg::Preg;
use shirabe_external_packages::composer::xdebug_handler::xdebug_handler::XdebugHandler;
use shirabe_php_shim::{
    InvalidArgumentException, PhpMixed, UnexpectedValueException, array_map_str_fn, array_slice,
    array_slice_strs, explode, get_class, implode, in_array, is_string, sprintf, str_replace,
    str_starts_with, strpos, strtolower, var_export,
};
use shirabe_semver::constraint::constraint::Constraint;

use crate::composer::Composer;
use crate::package::complete_package::CompletePackage;
use crate::package::complete_package_interface::CompletePackageInterface;
use crate::package::link::Link;
use crate::package::package_interface::PackageInterface;
use crate::package::version::version_parser::VersionParser;
use crate::platform::hhvm_detector::HhvmDetector;
use crate::platform::runtime::Runtime;
use crate::platform::version::Version;
use crate::plugin::plugin_interface::{self, PluginInterface};
use crate::repository::array_repository::ArrayRepository;
use crate::repository::repository_interface::RepositoryInterface;
use crate::util::silencer::Silencer;

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
    pub(crate) disabled_packages: IndexMap<String, Box<dyn CompletePackageInterface>>,
    pub(crate) runtime: Runtime,
    pub(crate) hhvm_detector: HhvmDetector,
}

impl PlatformRepository {
    pub const PLATFORM_PACKAGE_REGEX: &'static str = "{^(?:php(?:-64bit|-ipv6|-zts|-debug)?|hhvm|(?:ext|lib)-[a-z0-9](?:[_.-]?[a-z0-9]+)*|composer(?:-(?:plugin|runtime)-api)?)$}iD";

    pub fn new(
        packages: Vec<Box<dyn PackageInterface>>,
        overrides: IndexMap<String, PhpMixed>,
        runtime: Option<Runtime>,
        hhvm_detector: Option<HhvmDetector>,
    ) -> anyhow::Result<Self> {
        let runtime = runtime.unwrap_or_else(|| Runtime);
        let hhvm_detector = hhvm_detector.unwrap_or_else(|| HhvmDetector::new(None, None));
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
            inner: ArrayRepository::new(packages),
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

    pub fn get_disabled_packages(&self) -> &IndexMap<String, Box<dyn CompletePackageInterface>> {
        &self.disabled_packages
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

        let mut pretty_version = Composer::get_version();
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
        self.add_package(Box::new(composer))?;

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
        self.add_package(Box::new(composer_plugin_api))?;

        pretty_version = Composer::RUNTIME_API_VERSION.to_string();
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
        self.add_package(Box::new(composer_runtime_api))?;

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
                pretty_version = Preg::replace("#^([^~+-]+).*$#", "$1", &php_version_str)
                    .unwrap_or(php_version_str);
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
        self.add_package(Box::new(php))?;

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
            self.add_package(Box::new(phpdebug))?;
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
            self.add_package(Box::new(phpzts))?;
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
            self.add_package(Box::new(php64))?;
        }

        // The AF_INET6 constant is only defined if ext-sockets is available but
        // IPv6 support might still be available.
        let has_inet6 = self.runtime.has_constant("AF_INET6", None);
        let inet_pton_check = Silencer::call(|| {
            // TODO(phase-b): Runtime::invoke takes a Box<dyn Fn(Vec<PhpMixed>) -> PhpMixed>;
            // mirror PHP's `Silencer::call([$this->runtime, 'invoke'], 'inet_pton', ['::'])`.
            Ok::<PhpMixed, anyhow::Error>(self.runtime.invoke(
                Box::new(|_args| PhpMixed::Bool(false)),
                vec![
                    PhpMixed::String("inet_pton".to_string()),
                    PhpMixed::String("::".to_string()),
                ],
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
            self.add_package(Box::new(php_ipv6))?;
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
                    .map(|(i, s)| (i.to_string(), Box::new(PhpMixed::String(s.clone()))))
                    .collect(),
            ),
            true,
        ) {
            if let Some(xdebug_pretty_version) = XdebugHandler::get_skipped_version() {
                if !xdebug_pretty_version.is_empty() {
                    self.add_extension("xdebug", &xdebug_pretty_version)?;
                }
            }
        }

        // Another quick loop, just for possible libraries
        // Doing it this way to know that functions or constants exist before
        // relying on them.
        for name in &loaded_extensions {
            match name.as_str() {
                "amqp" => {
                    let info = self.runtime.get_extension_info(name)?;

                    // librabbitmq version => 0.9.0
                    if let Ok(Some(librabbitmq_matches)) = Preg::is_match_strict_groups(
                        "/^librabbitmq version => (?<version>.+)$/im",
                        &info,
                    ) {
                        self.add_library(
                            &mut libraries,
                            &format!("{}-librabbitmq", name),
                            Some(&librabbitmq_matches["version"]),
                            Some("AMQP librabbitmq version"),
                            &[],
                            &[],
                        )?;
                    }

                    // AMQP protocol version => 0-9-1
                    if let Ok(Some(protocol_matches)) = Preg::is_match_strict_groups(
                        "/^AMQP protocol version => (?<version>.+)$/im",
                        &info,
                    ) {
                        self.add_library(
                            &mut libraries,
                            &format!("{}-protocol", name),
                            Some(&str_replace("-", ".", &protocol_matches["version"])),
                            Some("AMQP protocol version"),
                            &[],
                            &[],
                        )?;
                    }
                }

                "bz2" => {
                    let info = self.runtime.get_extension_info(name)?;

                    // BZip2 Version => 1.0.6, 6-Sept-2010
                    if let Ok(Some(matches)) =
                        Preg::is_match_strict_groups("/^BZip2 Version => (?<version>.*),/im", &info)
                    {
                        self.add_library(
                            &mut libraries,
                            name,
                            Some(&matches["version"]),
                            None,
                            &[],
                            &[],
                        )?;
                    }
                }

                "curl" => {
                    let curl_version = self.runtime.invoke(
                        Box::new(|_args| PhpMixed::Null),
                        vec![PhpMixed::String("curl_version".to_string())],
                    );
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
                    if let Ok(Some(ssl_matches)) = Preg::is_match_strict_groups(
                        "{^SSL Version => (?<library>[^/]+)/(?<version>.+)$}im",
                        &info,
                    ) {
                        let library = strtolower(&ssl_matches["library"]);
                        if library == "openssl" {
                            let mut is_fips = false;
                            let parsed_version =
                                Version::parse_openssl(&ssl_matches["version"], &mut is_fips)
                                    .unwrap_or_default();
                            self.add_library(
                                &mut libraries,
                                &format!("{}-openssl{}", name, if is_fips { "-fips" } else { "" }),
                                Some(&parsed_version),
                                Some(&format!("curl OpenSSL version ({})", parsed_version)),
                                &[],
                                if is_fips {
                                    &["curl-openssl".to_string()]
                                } else {
                                    &[]
                                },
                            )?;
                        } else {
                            let (shortlib, ssl_lib);
                            if str_starts_with(&library, "(securetransport)") {
                                if let Ok(Some(securetransport_matches)) =
                                    Preg::is_match_strict_groups(
                                        "{^\\(securetransport\\) ([a-z0-9]+)}",
                                        &library,
                                    )
                                {
                                    shortlib = "securetransport".to_string();
                                    ssl_lib = format!("curl-{}", securetransport_matches["1"]);
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
                                Some(&ssl_matches["version"]),
                                Some(&format!(
                                    "curl {} version ({})",
                                    library, &ssl_matches["version"]
                                )),
                                &[ssl_lib],
                                &[],
                            )?;
                        }
                    }

                    // libSSH Version => libssh2/1.4.3
                    if let Ok(Some(ssh_matches)) = Preg::is_match_strict_groups(
                        "{^libSSH Version => (?<library>[^/]+)/(?<version>.+?)(?:/.*)?$}im",
                        &info,
                    ) {
                        self.add_library(
                            &mut libraries,
                            &format!("{}-{}", name, strtolower(&ssh_matches["library"])),
                            Some(&ssh_matches["version"]),
                            Some(&format!("curl {} version", &ssh_matches["library"])),
                            &[],
                            &[],
                        )?;
                    }

                    // ZLib Version => 1.2.8
                    if let Ok(Some(zlib_matches)) =
                        Preg::is_match_strict_groups("{^ZLib Version => (?<version>.+)$}im", &info)
                    {
                        self.add_library(
                            &mut libraries,
                            &format!("{}-zlib", name),
                            Some(&zlib_matches["version"]),
                            Some("curl zlib version"),
                            &[],
                            &[],
                        )?;
                    }
                }

                "date" => {
                    let info = self.runtime.get_extension_info(name)?;

                    // timelib version => 2018.03
                    if let Ok(Some(timelib_matches)) = Preg::is_match_strict_groups(
                        "/^timelib version => (?<version>.+)$/im",
                        &info,
                    ) {
                        self.add_library(
                            &mut libraries,
                            &format!("{}-timelib", name),
                            Some(&timelib_matches["version"]),
                            Some("date timelib version"),
                            &[],
                            &[],
                        )?;
                    }

                    // Timezone Database => internal
                    if let Ok(Some(zoneinfo_source_matches)) = Preg::is_match_strict_groups(
                        "/^Timezone Database => (?<source>internal|external)$/im",
                        &info,
                    ) {
                        let external = zoneinfo_source_matches["source"] == "external";
                        if let Ok(Some(zoneinfo_matches)) = Preg::is_match_strict_groups(
                            "/^\"Olson\" Timezone Database Version => (?<version>.+?)(?:\\.system)?$/im",
                            &info,
                        ) {
                            // If the timezonedb is provided by ext/timezonedb, register that version as a replacement
                            if external && loaded_extensions.iter().any(|n| n == "timezonedb") {
                                self.add_library(
                                    &mut libraries,
                                    "timezonedb-zoneinfo",
                                    Some(&zoneinfo_matches["version"]),
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
                                    Some(&zoneinfo_matches["version"]),
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
                    if let Ok(Some(magic_matches)) =
                        Preg::is_match_strict_groups("/^libmagic => (?<version>.+)$/im", &info)
                    {
                        self.add_library(
                            &mut libraries,
                            &format!("{}-libmagic", name),
                            Some(&magic_matches["version"]),
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

                    if let Ok(Some(libjpeg_matches)) = Preg::is_match_strict_groups(
                        "/^libJPEG Version => (?<version>.+?)(?: compatible)?$/im",
                        &info,
                    ) {
                        let parsed =
                            Version::parse_libjpeg(&libjpeg_matches["version"]).unwrap_or_default();
                        self.add_library(
                            &mut libraries,
                            &format!("{}-libjpeg", name),
                            Some(&parsed),
                            Some("libjpeg version for gd"),
                            &[],
                            &[],
                        )?;
                    }

                    if let Ok(Some(libpng_matches)) = Preg::is_match_strict_groups(
                        "/^libPNG Version => (?<version>.+)$/im",
                        &info,
                    ) {
                        self.add_library(
                            &mut libraries,
                            &format!("{}-libpng", name),
                            Some(&libpng_matches["version"]),
                            Some("libpng version for gd"),
                            &[],
                            &[],
                        )?;
                    }

                    if let Ok(Some(freetype_matches)) = Preg::is_match_strict_groups(
                        "/^FreeType Version => (?<version>.+)$/im",
                        &info,
                    ) {
                        self.add_library(
                            &mut libraries,
                            &format!("{}-freetype", name),
                            Some(&freetype_matches["version"]),
                            Some("freetype version for gd"),
                            &[],
                            &[],
                        )?;
                    }

                    if let Ok(Some(libxpm_matches)) = Preg::is_match_strict_groups(
                        "/^libXpm Version => (?<versionId>\\d+)$/im",
                        &info,
                    ) {
                        let version_id: i64 = libxpm_matches["versionId"].parse().unwrap_or(0);
                        let converted =
                            Version::convert_libxpm_version_id(version_id).unwrap_or_default();
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
                    } else if let Ok(Some(matches)) =
                        Preg::is_match_strict_groups("/^ICU version => (?<version>.+)$/im", &info)
                    {
                        self.add_library(
                            &mut libraries,
                            "icu",
                            Some(&matches["version"]),
                            Some(description),
                            &[],
                            &[],
                        )?;
                    }

                    // ICU TZData version => 2019c
                    if let Ok(Some(zoneinfo_matches)) = Preg::is_match_strict_groups(
                        "/^ICU TZData version => (?<version>.*)$/im",
                        &info,
                    ) {
                        if let Some(parsed) =
                            Version::parse_zoneinfo_version(&zoneinfo_matches["version"])
                        {
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
                            Box::new(|_args| PhpMixed::Null),
                            vec![
                                PhpMixed::List(vec![
                                    Box::new(PhpMixed::String("ResourceBundle".to_string())),
                                    Box::new(PhpMixed::String("create".to_string())),
                                ]),
                                PhpMixed::List(vec![
                                    Box::new(PhpMixed::String("root".to_string())),
                                    Box::new(PhpMixed::String("ICUDATA".to_string())),
                                    Box::new(PhpMixed::Bool(false)),
                                ]),
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
                            Box::new(|_args| PhpMixed::Null),
                            vec![PhpMixed::List(vec![
                                Box::new(PhpMixed::String("IntlChar".to_string())),
                                Box::new(PhpMixed::String("getUnicodeVersion".to_string())),
                            ])],
                        );
                        let sliced = array_slice(&intl_char_versions, 0, Some(3));
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
                    if let Ok(Some(matches)) = Preg::is_match_strict_groups(
                        "/^ImageMagick (?<version>[\\d.]+)(?:-(?<patch>\\d+))?/",
                        &image_magick_version_str,
                    ) {
                        let mut version_built = matches["version"].clone();
                        if let Some(patch) = matches.get("patch") {
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

                    if let (Ok(Some(matches)), Ok(Some(vendor_matches))) = (
                        Preg::is_match_strict_groups(
                            "/^Vendor Version => (?<versionId>\\d+)$/im",
                            &info,
                        ),
                        Preg::is_match_strict_groups("/^Vendor Name => (?<vendor>.+)$/im", &info),
                    ) {
                        let version_id: i64 = matches["versionId"].parse().unwrap_or(0);
                        let converted =
                            Version::convert_openldap_version_id(version_id).unwrap_or_default();
                        self.add_library(
                            &mut libraries,
                            &format!("{}-{}", name, strtolower(&vendor_matches["vendor"])),
                            Some(&converted),
                            Some(&format!("{} version of ldap", &vendor_matches["vendor"])),
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
                    if let Ok(Some(libmbfl_matches)) = Preg::is_match_strict_groups(
                        "/^libmbfl version => (?<version>.+)$/im",
                        &info,
                    ) {
                        self.add_library(
                            &mut libraries,
                            &format!("{}-libmbfl", name),
                            Some(&libmbfl_matches["version"]),
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
                    } else if let Ok(Some(oniguruma_matches)) = Preg::is_match_strict_groups(
                        "/^(?:oniguruma|Multibyte regex \\(oniguruma\\)) version => (?<version>.+)$/im",
                        &info,
                    ) {
                        self.add_library(
                            &mut libraries,
                            &format!("{}-oniguruma", name),
                            Some(&oniguruma_matches["version"]),
                            Some("mbstring oniguruma version"),
                            &[],
                            &[],
                        )?;
                    }
                }

                "memcached" => {
                    let info = self.runtime.get_extension_info(name)?;

                    // libmemcached version => 1.0.18
                    if let Ok(Some(matches)) = Preg::is_match_strict_groups(
                        "/^libmemcached version => (?<version>.+)$/im",
                        &info,
                    ) {
                        self.add_library(
                            &mut libraries,
                            &format!("{}-libmemcached", name),
                            Some(&matches["version"]),
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
                    if let Ok(Some(matches)) = Preg::is_match_strict_groups(
                        "{^(?:OpenSSL|LibreSSL)?\\s*(?<version>\\S+)}i",
                        &openssl_text_str,
                    ) {
                        let mut is_fips = false;
                        let parsed_version =
                            Version::parse_openssl(&matches["version"], &mut is_fips)
                                .unwrap_or_default();
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
                    let stripped = Preg::replace("{^(\\S+).*}", "$1", &pcre_version_str)
                        .unwrap_or(pcre_version_str);
                    self.add_library(&mut libraries, name, Some(&stripped), None, &[], &[])?;

                    let info = self.runtime.get_extension_info(name)?;

                    // PCRE Unicode Version => 12.1.0
                    if let Ok(Some(pcre_unicode_matches)) = Preg::is_match_strict_groups(
                        "/^PCRE Unicode Version => (?<version>.+)$/im",
                        &info,
                    ) {
                        self.add_library(
                            &mut libraries,
                            &format!("{}-unicode", name),
                            Some(&pcre_unicode_matches["version"]),
                            Some("PCRE Unicode version support"),
                            &[],
                            &[],
                        )?;
                    }
                }

                "mysqlnd" | "pdo_mysql" => {
                    let info = self.runtime.get_extension_info(name)?;

                    if let Ok(Some(matches)) = Preg::is_match_strict_groups(
                        "/^(?:Client API version|Version) => mysqlnd (?<version>.+?) /mi",
                        &info,
                    ) {
                        self.add_library(
                            &mut libraries,
                            &format!("{}-mysqlnd", name),
                            Some(&matches["version"]),
                            Some(&format!("mysqlnd library version for {}", name)),
                            &[],
                            &[],
                        )?;
                    }
                }

                "mongodb" => {
                    let info = self.runtime.get_extension_info(name)?;

                    if let Ok(Some(libmongoc_matches)) = Preg::is_match_strict_groups(
                        "/^libmongoc bundled version => (?<version>.+)$/im",
                        &info,
                    ) {
                        self.add_library(
                            &mut libraries,
                            &format!("{}-libmongoc", name),
                            Some(&libmongoc_matches["version"]),
                            Some("libmongoc version of mongodb"),
                            &[],
                            &[],
                        )?;
                    }

                    if let Ok(Some(libbson_matches)) = Preg::is_match_strict_groups(
                        "/^libbson bundled version => (?<version>.+)$/im",
                        &info,
                    ) {
                        self.add_library(
                            &mut libraries,
                            &format!("{}-libbson", name),
                            Some(&libbson_matches["version"]),
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

                        if let Ok(Some(matches)) = Preg::is_match_strict_groups(
                            "/^PostgreSQL\\(libpq\\) Version => (?<version>.*)$/im",
                            &info,
                        ) {
                            self.add_library(
                                &mut libraries,
                                &format!("{}-libpq", name),
                                Some(&matches["version"]),
                                Some(&format!("libpq for {}", name)),
                                &[],
                                &[],
                            )?;
                        }
                    }
                }

                "pdo_pgsql" => {
                    let info = self.runtime.get_extension_info(name)?;

                    if let Ok(Some(matches)) = Preg::is_match_strict_groups(
                        "/^PostgreSQL\\(libpq\\) Version => (?<version>.*)$/im",
                        &info,
                    ) {
                        self.add_library(
                            &mut libraries,
                            &format!("{}-libpq", name),
                            Some(&matches["version"]),
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
                    if let Ok(Some(matches)) = Preg::is_match_strict_groups(
                        "/^libpq => (?<compiled>.+) => (?<linked>.+)$/im",
                        &info,
                    ) {
                        self.add_library(
                            &mut libraries,
                            &format!("{}-libpq", name),
                            Some(&matches["linked"]),
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
                        let version_built = sprintf(
                            "%d.%d.%d",
                            &[
                                PhpMixed::Int((lib_rd_kafka_version_int & 0x7F000000) >> 24),
                                PhpMixed::Int((lib_rd_kafka_version_int & 0x00FF0000) >> 16),
                                PhpMixed::Int((lib_rd_kafka_version_int & 0x0000FF00) >> 8),
                            ],
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

                    if let Ok(Some(matches)) = Preg::is_match_strict_groups(
                        "/^SQLite Library => (?<version>.+)$/im",
                        &info,
                    ) {
                        self.add_library(
                            &mut libraries,
                            &format!("{}-sqlite", name),
                            Some(&matches["version"]),
                            None,
                            &[],
                            &[],
                        )?;
                    }
                }

                "ssh2" => {
                    let info = self.runtime.get_extension_info(name)?;

                    if let Ok(Some(matches)) = Preg::is_match_strict_groups(
                        "/^libssh2 version => (?<version>.+)$/im",
                        &info,
                    ) {
                        self.add_library(
                            &mut libraries,
                            &format!("{}-libssh2", name),
                            Some(&matches["version"]),
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
                    if let Ok(Some(matches)) = Preg::is_match_strict_groups(
                        "/^libxslt compiled against libxml Version => (?<version>.+)$/im",
                        &info,
                    ) {
                        self.add_library(
                            &mut libraries,
                            "libxslt-libxml",
                            Some(&matches["version"]),
                            Some("libxml version libxslt is compiled against"),
                            &[],
                            &[],
                        )?;
                    }
                }

                "yaml" => {
                    let info = self.runtime.get_extension_info("yaml")?;

                    if let Ok(Some(matches)) = Preg::is_match_strict_groups(
                        "/^LibYAML Version => (?<version>.+)$/im",
                        &info,
                    ) {
                        self.add_library(
                            &mut libraries,
                            &format!("{}-libyaml", name),
                            Some(&matches["version"]),
                            Some("libyaml version of yaml"),
                            &[],
                            &[],
                        )?;
                    }
                }

                "zip" => {
                    if self
                        .runtime
                        .has_constant("LIBZIP_VERSION", Some("ZipArchive"))
                    {
                        let libzip = self
                            .runtime
                            .get_constant("LIBZIP_VERSION", Some("ZipArchive"));
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
                        if let Ok(Some(matches)) = Preg::is_match_strict_groups(
                            "/^Linked Version => (?<version>.+)$/im",
                            &info,
                        ) {
                            self.add_library(
                                &mut libraries,
                                name,
                                Some(&matches["version"]),
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
                    pretty_version = Preg::replace("#^([^~+-]+).*$#", "$1", &hhvm_version)
                        .unwrap_or(hhvm_version);
                    version = self
                        .version_parser
                        .as_ref()
                        .unwrap()
                        .normalize(&pretty_version, None)?;
                }
            }

            let mut hhvm = CompletePackage::new("hhvm".to_string(), version, pretty_version);
            hhvm.set_description("The HHVM Runtime (64bit)".to_string());
            self.add_package(Box::new(hhvm))?;
        }
        Ok(())
    }

    pub fn add_package(&mut self, package: Box<dyn PackageInterface>) -> anyhow::Result<()> {
        // TODO(phase-b): downcast `package` to CompletePackage; this stub keeps the structure.
        if !Self::is_complete_package(package.as_ref()) {
            return Err(anyhow::anyhow!(UnexpectedValueException {
                message: format!(
                    "Expected CompletePackage but got {}",
                    get_class(&PhpMixed::Null)
                ),
                code: 0,
            }));
        }

        // Skip if overridden
        if self.overrides.contains_key(package.get_name()) {
            if matches!(
                self.overrides[package.get_name()].version,
                PhpMixed::Bool(false)
            ) {
                self.add_disabled_package_from_pkg(package);
                return Ok(());
            }

            let overrider = self
                .inner
                .find_package(package.get_name().to_string(), "*".to_string());
            let actual_text = if let Some(ref ov) = overrider {
                if package.get_version() == ov.get_version() {
                    "same as actual".to_string()
                } else {
                    format!("actual: {}", package.get_pretty_version())
                }
            } else {
                format!("actual: {}", package.get_pretty_version())
            };
            if let Some(_overrider_pkg) = overrider {
                // TODO(phase-b): downcast `overrider` to CompletePackageInterface for setDescription
                let _ = actual_text;
            }

            return Ok(());
        }

        // Skip if PHP is overridden and we are adding a php-* package
        if self.overrides.contains_key("php") && strpos(package.get_name(), "php-") == Some(0) {
            let php_override = PlatformOverride {
                name: self.overrides["php"].name.clone(),
                version: self.overrides["php"].version.clone(),
            };
            let mut overrider = self.add_overridden_package(
                &php_override,
                Some(package.get_pretty_name().to_string()),
            )?;
            let actual_text = if package.get_version() == overrider.get_version() {
                "same as actual".to_string()
            } else {
                format!("actual: {}", package.get_pretty_version())
            };
            let current_description = overrider.get_description().unwrap_or("").to_string();
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
    ) -> anyhow::Result<CompletePackage> {
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
        package.set_extra(extra);
        // TODO(phase-b): CompletePackage is `Box<dyn PackageInterface>`-cloneable in PHP;
        // here we add a clone for ArrayRepository but also return the original.
        self.inner.add_package(Box::new(package.clone()));

        if package.get_name() == "php" {
            let parts = explode(".", package.get_version());
            let head = array_slice_strs(&parts, 0, Some(3));
            *LAST_SEEN_PLATFORM_PHP.lock().unwrap() = Some(implode(".", &head));
        }

        Ok(package)
    }

    fn add_disabled_package_from_pkg(&mut self, _package: Box<dyn PackageInterface>) {
        // TODO(phase-b): downcast to CompletePackage and call `addDisabledPackage`.
    }

    fn add_disabled_package(&mut self, mut package: CompletePackage) {
        let current_description = package.get_description().unwrap_or("").to_string();
        package.set_description(format!(
            "{}. <warning>Package disabled via config.platform</warning>",
            current_description
        ));
        let mut extra: IndexMap<String, PhpMixed> = IndexMap::new();
        extra.insert("config.platform".to_string(), PhpMixed::Bool(true));
        package.set_extra(extra);

        self.disabled_packages
            .insert(package.get_name().to_string(), Box::new(package));
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
                if let Ok(Some(m)) = Preg::is_match_strict_groups(
                    "{^(\\d+\\.\\d+\\.\\d+(?:\\.\\d+)?)}",
                    &pretty_version,
                ) {
                    pretty_version = m["1"].clone();
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
        ext.set_type("php-ext".to_string());

        if name == "uuid" {
            let mut replaces: IndexMap<String, Link> = IndexMap::new();
            replaces.insert(
                "lib-uuid".to_string(),
                Link::new(
                    "ext-uuid".to_string(),
                    "lib-uuid".to_string(),
                    Box::new(Constraint::new("=", &version)),
                    Link::TYPE_REPLACE.to_string(),
                    Some(ext.get_pretty_version().to_string()),
                ),
            );
            ext.set_replaces(replaces);
        }

        self.add_package(Box::new(ext))?;
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
                    Box::new(Constraint::new("=", &version)),
                    Link::TYPE_REPLACE.to_string(),
                    Some(lib.get_pretty_version().to_string()),
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
                    Box::new(Constraint::new("=", &version)),
                    Link::TYPE_PROVIDE.to_string(),
                    Some(lib.get_pretty_version().to_string()),
                ),
            );
        }
        lib.set_replaces(replace_links);
        lib.set_provides(provide_links);

        self.add_package(Box::new(lib))?;
        Ok(())
    }

    /// Check if a package name is a platform package.
    pub fn is_platform_package(name: &str) -> bool {
        let mut cache = IS_PLATFORM_PACKAGE_CACHE.lock().unwrap();

        if let Some(&cached) = cache.get(name) {
            return cached;
        }

        let result = Preg::is_match(Self::PLATFORM_PACKAGE_REGEX, name).unwrap_or(false);
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
        &self,
        query: String,
        mode: i64,
        r#type: Option<String>,
    ) -> Vec<crate::repository::repository_interface::SearchResult> {
        // suppress vendor search as there are no vendors to match in platform packages
        if mode == <dyn RepositoryInterface>::SEARCH_VENDOR {
            return Vec::new();
        }

        self.inner.search(query, mode, r#type)
    }

    // ---- helpers ----

    fn is_complete_package(_package: &dyn PackageInterface) -> bool {
        // TODO(phase-b): use Any-style downcasting once the trait carries it.
        true
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
                .map(|v| match v.as_ref() {
                    PhpMixed::String(s) => s.clone(),
                    PhpMixed::Int(i) => i.to_string(),
                    PhpMixed::Float(f) => f.to_string(),
                    _ => "".to_string(),
                })
                .collect(),
            PhpMixed::Array(m) => m
                .values()
                .map(|v| match v.as_ref() {
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
