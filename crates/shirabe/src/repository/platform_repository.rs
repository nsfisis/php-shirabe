//! ref: composer/src/Composer/Repository/PlatformRepository.php

use crate::composer;
use crate::package::version::VersionParser;
use crate::package::{
    CompletePackage, CompletePackageHandle, CompletePackageInterface,
    CompletePackageInterfaceHandle, Link, PackageInterface, PackageInterfaceHandle,
};
use crate::platform::{HhvmDetector, HhvmDetectorInterface, Version};
use crate::plugin::plugin_interface::{self};
use crate::repository::{ArrayRepository, RepositoryInterface};
use bstr::{B, BStr, BString, ByteSlice};
use indexmap::IndexMap;
use shirabe_php_rpc::PlatformInfo;
use shirabe_php_shim::{
    InvalidArgumentException, PhpMixed, UnexpectedValueException, array_slice_mixed,
    array_slice_strs, bconcat, explode, get_class, get_debug_type, implode, is_string,
    legacy_lossy, php_regex, preg_is_match, preg_match, preg_replace, str_replace, strpos,
    strtolower, var_export,
};
use shirabe_semver::constraint::SimpleConstraint;
use std::sync::{LazyLock, Mutex};

static LAST_SEEN_PLATFORM_PHP: LazyLock<Mutex<Option<BString>>> =
    LazyLock::new(|| Mutex::new(None));

static IS_PLATFORM_PACKAGE_CACHE: LazyLock<Mutex<IndexMap<BString, bool>>> =
    LazyLock::new(|| Mutex::new(IndexMap::new()));

#[derive(Debug)]
pub struct PlatformOverride {
    pub name: BString,
    /// `PhpMixed::String(_)` or `PhpMixed::Bool(false)`.
    pub version: PhpMixed,
}

#[derive(Debug)]
pub struct PlatformRepository {
    inner: ArrayRepository,
    version_parser: Option<VersionParser>,
    overrides: IndexMap<BString, PlatformOverride>,
    disabled_packages: IndexMap<BString, CompletePackageInterfaceHandle>,
    platform_info: Option<PlatformInfo>,
    hhvm_detector: Box<dyn HhvmDetectorInterface>,
}

impl PlatformRepository {
    const PLATFORM_PACKAGE_REGEX: &'static str = "{^(?:php(?:-64bit|-ipv6|-zts|-debug)?|hhvm|(?:ext|lib)-[a-z0-9](?:[_.-]?[a-z0-9]+)*|composer(?:-(?:plugin|runtime)-api)?)$}iD";

    /// A `None` `platform_info` is resolved from the PHP worker on the first `initialize()`, so
    /// constructing the repository on its own never starts the worker.
    pub fn new(
        packages: Vec<PackageInterfaceHandle>,
        overrides: IndexMap<BString, PhpMixed>,
        platform_info: Option<PlatformInfo>,
        hhvm_detector: Option<Box<dyn HhvmDetectorInterface>>,
    ) -> anyhow::Result<Self> {
        let hhvm_detector: Box<dyn HhvmDetectorInterface> =
            hhvm_detector.unwrap_or_else(|| Box::new(HhvmDetector::new(None, None)));
        let mut overrides_map: IndexMap<BString, PlatformOverride> = IndexMap::new();
        for (name, version) in overrides {
            // TODO(bytes): platform package names are Strings, so non-UTF-8 bytes are replaced.
            let name = BString::from(&name.clone()[..]);
            if !is_string(&version) && !matches!(version, PhpMixed::Bool(false)) {
                return Err(UnexpectedValueException::new(format!(
                    "config.platform.{} should be a string or false, but got {} {}",
                    name.display_lossy(),
                    get_debug_type(&version).display_lossy(),
                    var_export(&version, true).display_lossy()
                ))
                .into());
            }
            if name == "php" && matches!(version, PhpMixed::Bool(false)) {
                return Err(UnexpectedValueException::new(format!(
                    "config.platform.{} cannot be set to false as you cannot disable php entirely.",
                    name.display_lossy()
                ))
                .into());
            }
            overrides_map.insert(
                BString::from(&strtolower(&name)[..]),
                PlatformOverride {
                    name: name.clone(),
                    version,
                },
            );
        }
        // PHP: parent::__construct($packages) runs after the overrides are set, and the parent
        // ctor's $this->addPackage() calls late-bind to PlatformRepository::addPackage.
        let mut this = Self {
            inner: ArrayRepository::new(vec![])?,
            version_parser: None,
            overrides: overrides_map,
            disabled_packages: IndexMap::new(),
            platform_info,
            hhvm_detector,
        };
        for package in packages {
            this.add_package(package)?;
        }
        Ok(this)
    }

    pub fn get_repo_name(&self) -> BString {
        bstr::BString::from("platform repo".to_string())
    }

    pub fn is_platform_package_disabled(&self, name: &BStr) -> bool {
        self.disabled_packages.contains_key(name)
    }

    pub fn get_disabled_packages(&self) -> &IndexMap<BString, CompletePackageInterfaceHandle> {
        &self.disabled_packages
    }

    fn ensure_initialized(&mut self) -> anyhow::Result<()> {
        if !self.inner.is_initialized() {
            self.initialize()?;
        }
        Ok(())
    }

    #[tracing::instrument(skip_all)]
    fn initialize(&mut self) -> anyhow::Result<()> {
        self.inner.initialize();

        let platform_info = self
            .platform_info
            .get_or_insert_with(|| shirabe_php_rpc::get_platform_info().clone())
            .clone();

        let mut libraries: IndexMap<BString, bool> = IndexMap::new();

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
                return Err(InvalidArgumentException::new(format!(
                    "Invalid platform package name in config.platform: {}",
                    r#override.name.display_lossy()
                ))
                .into());
            }

            if !matches!(r#override.version, PhpMixed::Bool(false)) {
                self.add_overridden_package(r#override, None)?;
            }
        }

        let mut pretty_version = composer::get_version();
        let mut version = BString::from(
            &self
                .version_parser
                .as_ref()
                .unwrap()
                .normalize(&pretty_version, None)?[..],
        );
        let mut composer = CompletePackage::new(
            bstr::BString::from("composer".to_string()),
            version.clone(),
            bstr::BString::from(pretty_version.clone()),
        );
        composer.set_description(bstr::BString::from("Composer package".to_string()));
        self.add_package(CompletePackageHandle::from_complete_package(composer).into())?;

        pretty_version = plugin_interface::PLUGIN_API_VERSION.to_string();
        version = BString::from(
            &self
                .version_parser
                .as_ref()
                .unwrap()
                .normalize(&pretty_version, None)?[..],
        );
        let mut composer_plugin_api = CompletePackage::new(
            bstr::BString::from("composer-plugin-api".to_string()),
            version.clone(),
            bstr::BString::from(pretty_version.clone()),
        );
        composer_plugin_api
            .set_description(bstr::BString::from("The Composer Plugin API".to_string()));
        self.add_package(CompletePackageHandle::from_complete_package(composer_plugin_api).into())?;

        pretty_version = composer::RUNTIME_API_VERSION.to_string();
        version = BString::from(
            &self
                .version_parser
                .as_ref()
                .unwrap()
                .normalize(&pretty_version, None)?[..],
        );
        let mut composer_runtime_api = CompletePackage::new(
            bstr::BString::from("composer-runtime-api".to_string()),
            version.clone(),
            bstr::BString::from(pretty_version.clone()),
        );
        composer_runtime_api
            .set_description(bstr::BString::from("The Composer Runtime API".to_string()));
        self.add_package(
            CompletePackageHandle::from_complete_package(composer_runtime_api).into(),
        )?;

        let php_version_const = platform_info.get_constant("PHP_VERSION", None);
        let php_version_str = legacy_lossy(&match &php_version_const {
            PhpMixed::String(s) => s.clone(),
            _ => bstr::BString::from("".to_string()),
        })
        .into_owned();
        match self
            .version_parser
            .as_ref()
            .unwrap()
            .normalize(&php_version_str, None)
        {
            Ok(v) => {
                pretty_version = php_version_str.clone();
                version = BString::from(&v[..]);
            }
            Err(_) => {
                pretty_version = legacy_lossy(&BString::from(
                    &preg_replace(php_regex!("#^([^~+-]+).*$#"), "$1", &php_version_str)[..],
                ))
                .into_owned();
                version = BString::from(
                    &self
                        .version_parser
                        .as_ref()
                        .unwrap()
                        .normalize(&pretty_version, None)?[..],
                );
            }
        }

        let mut php = CompletePackage::new(
            bstr::BString::from("php".to_string()),
            version.clone(),
            bstr::BString::from(pretty_version.clone()),
        );
        php.set_description(bstr::BString::from("The PHP interpreter".to_string()));
        self.add_package(CompletePackageHandle::from_complete_package(php).into())?;

        if platform_info
            .get_constant("PHP_DEBUG", None)
            .as_bool()
            .unwrap_or(false)
        {
            let mut phpdebug = CompletePackage::new(
                bstr::BString::from("php-debug".to_string()),
                version.clone(),
                bstr::BString::from(pretty_version.clone()),
            );
            phpdebug.set_description(bstr::BString::from(
                "The PHP interpreter, with debugging symbols".to_string(),
            ));
            self.add_package(CompletePackageHandle::from_complete_package(phpdebug).into())?;
        }

        if platform_info.has_constant("PHP_ZTS", None)
            && platform_info
                .get_constant("PHP_ZTS", None)
                .as_bool()
                .unwrap_or(false)
        {
            let mut phpzts = CompletePackage::new(
                bstr::BString::from("php-zts".to_string()),
                version.clone(),
                bstr::BString::from(pretty_version.clone()),
            );
            phpzts.set_description(bstr::BString::from(
                "The PHP interpreter, with Zend Thread Safety".to_string(),
            ));
            self.add_package(CompletePackageHandle::from_complete_package(phpzts).into())?;
        }

        if platform_info
            .get_constant("PHP_INT_SIZE", None)
            .as_int()
            .map(|v| v == 8)
            .unwrap_or(false)
        {
            let mut php64 = CompletePackage::new(
                bstr::BString::from("php-64bit".to_string()),
                version.clone(),
                bstr::BString::from(pretty_version.clone()),
            );
            php64.set_description(bstr::BString::from(
                "The PHP interpreter, 64bit".to_string(),
            ));
            self.add_package(CompletePackageHandle::from_complete_package(php64).into())?;
        }

        // The AF_INET6 constant is only defined if ext-sockets is available but
        // IPv6 support might still be available.
        let has_inet6 = platform_info.has_constant("AF_INET6", None);
        let inet_pton_check = &platform_info.inet_pton_ipv6;
        if has_inet6 || !matches!(inet_pton_check, PhpMixed::Bool(false)) {
            let mut php_ipv6 = CompletePackage::new(
                bstr::BString::from("php-ipv6".to_string()),
                version,
                bstr::BString::from(pretty_version),
            );
            php_ipv6.set_description(bstr::BString::from(
                "The PHP interpreter, with IPv6 support".to_string(),
            ));
            self.add_package(CompletePackageHandle::from_complete_package(php_ipv6).into())?;
        }

        let loaded_extensions = platform_info.get_extensions().to_vec();

        // Extensions scanning
        for name in &loaded_extensions {
            if [b"standard".as_slice(), b"Core"].contains(&name.as_slice()) {
                continue;
            }

            self.add_extension(name.as_bstr(), platform_info.get_extension_version(name))?;
        }

        // Check for Xdebug in a restarted process
        // PHP re-adds the extension `XdebugHandler::getSkippedVersion()` names, since its restart
        // runs on an ini file with Xdebug commented out. The worker is instead started with
        // `xdebug.mode=off` (see docs/dev/xdebug.md), which leaves the extension loaded, so it is
        // already among $loadedExtensions.

        // Another quick loop, just for possible libraries
        // Doing it this way to know that functions or constants exist before
        // relying on them.
        for name in &loaded_extensions {
            match name.as_slice() {
                b"amqp" => {
                    let info = platform_info.get_extension_info(name);

                    // librabbitmq version => 0.9.0
                    if let Some(librabbitmq_matches) = preg_match(
                        php_regex!("/^librabbitmq version => (?<version>.+)$/im"),
                        info,
                    ) {
                        self.add_library(
                            &mut libraries,
                            bstr::BStr::new(&bconcat!(name, "-librabbitmq")),
                            librabbitmq_matches.name("version"),
                            Some("AMQP librabbitmq version"),
                            &[],
                            &[],
                        )?;
                    }

                    // AMQP protocol version => 0-9-1
                    if let Some(protocol_matches) = preg_match(
                        php_regex!("/^AMQP protocol version => (?<version>.+)$/im"),
                        info,
                    ) {
                        let version_str =
                            BString::from(protocol_matches.name("version").unwrap_or_default());
                        self.add_library(
                            &mut libraries,
                            bstr::BStr::new(&bconcat!(name, "-protocol")),
                            Some(&*legacy_lossy(&str_replace("-", ".", &version_str))),
                            Some("AMQP protocol version"),
                            &[],
                            &[],
                        )?;
                    }
                }

                b"bz2" => {
                    let info = platform_info.get_extension_info(name);

                    // BZip2 Version => 1.0.6, 6-Sept-2010
                    if let Some(matches) =
                        preg_match(php_regex!("/^BZip2 Version => (?<version>.*),/im"), info)
                    {
                        self.add_library(
                            &mut libraries,
                            name.as_bstr(),
                            matches.name("version"),
                            None::<&str>,
                            &[],
                            &[],
                        )?;
                    }
                }

                b"curl" => {
                    let curl_version = &platform_info.curl_version;
                    let curl_version_str = curl_version
                        .as_array()
                        .and_then(|m| m.get(B("version")))
                        .and_then(|v| v.as_string())
                        .map(BString::from);
                    if let Some(cv) = curl_version_str.as_deref() {
                        self.add_library(
                            &mut libraries,
                            name.as_bstr(),
                            Some(cv),
                            None::<&str>,
                            &[],
                            &[],
                        )?;
                    }

                    let info = platform_info.get_extension_info(name);

                    // SSL Version => OpenSSL/1.0.1t
                    if let Some(ssl_matches) = preg_match(
                        php_regex!("{^SSL Version => (?<library>[^/]+)/(?<version>.+)$}im"),
                        info,
                    ) {
                        let ssl_library_raw =
                            BString::from(ssl_matches.name("library").unwrap_or_default());
                        let ssl_version =
                            BString::from(ssl_matches.name("version").unwrap_or_default());
                        let library =
                            BString::from(&BString::from(&strtolower(&ssl_library_raw)[..])[..]);
                        if library == "openssl" {
                            let mut is_fips = false;
                            let parsed_version =
                                Version::parse_openssl(&legacy_lossy(&ssl_version), &mut is_fips)
                                    .unwrap_or_default();
                            let fips_provides: Vec<BString> = if is_fips {
                                vec![bstr::BString::from("curl-openssl".to_string())]
                            } else {
                                Vec::new()
                            };
                            self.add_library(
                                &mut libraries,
                                bstr::BStr::new(&format!(
                                    "{}-openssl{}",
                                    legacy_lossy(&name),
                                    if is_fips { "-fips" } else { "" }
                                )),
                                Some(&parsed_version),
                                Some(&format!("curl OpenSSL version ({})", parsed_version)),
                                &[],
                                &fips_provides,
                            )?;
                        } else {
                            let (shortlib, ssl_lib);
                            if library.starts_with(b"(securetransport)") {
                                if let Some(securetransport_matches) = preg_match(
                                    php_regex!("{^\\(securetransport\\) ([a-z0-9]+)}"),
                                    &library,
                                ) {
                                    shortlib = "securetransport".to_string();
                                    let m1 = BString::from(
                                        securetransport_matches.get(1).unwrap_or_default(),
                                    );
                                    ssl_lib = bconcat!("curl-", m1);
                                } else {
                                    shortlib = legacy_lossy(&library.clone()).into_owned();
                                    ssl_lib = "curl-openssl".into();
                                }
                            } else {
                                shortlib = legacy_lossy(&library.clone()).into_owned();
                                ssl_lib = "curl-openssl".into();
                            }
                            self.add_library(
                                &mut libraries,
                                bstr::BStr::new(&format!("{}-{}", legacy_lossy(&name), shortlib)),
                                Some(&ssl_version),
                                Some(&bconcat!("curl ", library, " version (", ssl_version, ")")),
                                &[ssl_lib],
                                &[],
                            )?;
                        }
                    }

                    // libSSH Version => libssh2/1.4.3
                    if let Some(ssh_matches) = preg_match(
                        php_regex!(
                            "{^libSSH Version => (?<library>[^/]+)/(?<version>.+?)(?:/.*)?$}im"
                        ),
                        info,
                    ) {
                        let ssh_library =
                            BString::from(ssh_matches.name("library").unwrap_or_default());
                        let ssh_version =
                            BString::from(ssh_matches.name("version").unwrap_or_default());
                        self.add_library(
                            &mut libraries,
                            bstr::BStr::new(&bconcat!(name, "-", strtolower(&ssh_library))),
                            Some(&ssh_version),
                            Some(&*bconcat!("curl ", &ssh_library, " version")),
                            &[],
                            &[],
                        )?;
                    }

                    // ZLib Version => 1.2.8
                    if let Some(zlib_matches) =
                        preg_match(php_regex!("{^ZLib Version => (?<version>.+)$}im"), info)
                    {
                        self.add_library(
                            &mut libraries,
                            bstr::BStr::new(&bconcat!(name, "-zlib")),
                            zlib_matches.name("version"),
                            Some("curl zlib version"),
                            &[],
                            &[],
                        )?;
                    }
                }

                b"date" => {
                    let info = platform_info.get_extension_info(name);

                    // timelib version => 2018.03
                    if let Some(timelib_matches) =
                        preg_match(php_regex!("/^timelib version => (?<version>.+)$/im"), info)
                    {
                        self.add_library(
                            &mut libraries,
                            bstr::BStr::new(&bconcat!(name, "-timelib")),
                            timelib_matches.name("version"),
                            Some("date timelib version"),
                            &[],
                            &[],
                        )?;
                    }

                    // Timezone Database => internal
                    if let Some(zoneinfo_source_matches) = preg_match(
                        php_regex!("/^Timezone Database => (?<source>internal|external)$/im"),
                        info,
                    ) {
                        let external = zoneinfo_source_matches
                            .name("source")
                            .map(|s| s == "external")
                            .unwrap_or(false);
                        if let Some(zoneinfo_matches) = preg_match(
                            php_regex!(
                                "/^\"Olson\" Timezone Database Version => (?<version>.+?)(?:\\.system)?$/im"
                            ),
                            info,
                        ) {
                            let zoneinfo_version =
                                BString::from(zoneinfo_matches.name("version").unwrap_or_default());
                            // If the timezonedb is provided by ext/timezonedb, register that version as a replacement
                            if external && loaded_extensions.iter().any(|n| n == "timezonedb") {
                                self.add_library(
                                &mut libraries,
                                bstr::BStr::new("timezonedb-zoneinfo"),
                                Some(&zoneinfo_version),
                                Some(
                                    "zoneinfo (\"Olson\") database for date (replaced by timezonedb)",
                                ),
                                &[bconcat!(name, "-zoneinfo")],
                                &[],
                            )?;
                            } else {
                                self.add_library(
                                    &mut libraries,
                                    bstr::BStr::new(&bconcat!(name, "-zoneinfo")),
                                    Some(&zoneinfo_version),
                                    Some("zoneinfo (\"Olson\") database for date"),
                                    &[],
                                    &[],
                                )?;
                            }
                        }
                    }
                }

                b"fileinfo" => {
                    let info = platform_info.get_extension_info(name);

                    // libmagic => 537
                    if let Some(magic_matches) =
                        preg_match(php_regex!("/^libmagic => (?<version>.+)$/im"), info)
                    {
                        self.add_library(
                            &mut libraries,
                            bstr::BStr::new(&bconcat!(name, "-libmagic")),
                            magic_matches.name("version"),
                            Some("fileinfo libmagic version"),
                            &[],
                            &[],
                        )?;
                    }
                }

                b"gd" => {
                    let gd_version = platform_info.get_constant("GD_VERSION", None);
                    let gd_version_str = match &gd_version {
                        PhpMixed::String(s) => Some(s.clone()),
                        _ => None,
                    };
                    self.add_library(
                        &mut libraries,
                        name.as_bstr(),
                        gd_version_str.as_deref(),
                        None::<&str>,
                        &[],
                        &[],
                    )?;

                    let info = platform_info.get_extension_info(name);

                    if let Some(libjpeg_matches) = preg_match(
                        php_regex!("/^libJPEG Version => (?<version>.+?)(?: compatible)?$/im"),
                        info,
                    ) {
                        let libjpeg_version =
                            BString::from(libjpeg_matches.name("version").unwrap_or_default());
                        let parsed = Version::parse_libjpeg(&legacy_lossy(&libjpeg_version))
                            .unwrap_or_default();
                        self.add_library(
                            &mut libraries,
                            bstr::BStr::new(&bconcat!(name, "-libjpeg")),
                            Some(&parsed),
                            Some("libjpeg version for gd"),
                            &[],
                            &[],
                        )?;
                    }

                    if let Some(libpng_matches) =
                        preg_match(php_regex!("/^libPNG Version => (?<version>.+)$/im"), info)
                    {
                        self.add_library(
                            &mut libraries,
                            bstr::BStr::new(&bconcat!(name, "-libpng")),
                            libpng_matches.name("version"),
                            Some("libpng version for gd"),
                            &[],
                            &[],
                        )?;
                    }

                    if let Some(freetype_matches) =
                        preg_match(php_regex!("/^FreeType Version => (?<version>.+)$/im"), info)
                    {
                        self.add_library(
                            &mut libraries,
                            bstr::BStr::new(&bconcat!(name, "-freetype")),
                            freetype_matches.name("version"),
                            Some("freetype version for gd"),
                            &[],
                            &[],
                        )?;
                    }

                    if let Some(libxpm_matches) = preg_match(
                        php_regex!("/^libXpm Version => (?<versionId>\\d+)$/im"),
                        info,
                    ) {
                        let version_id: i64 = libxpm_matches
                            .name("versionId")
                            .and_then(|s| legacy_lossy(s).parse().ok())
                            .unwrap_or(0);
                        let converted = Version::convert_libxpm_version_id(version_id);
                        self.add_library(
                            &mut libraries,
                            bstr::BStr::new(&bconcat!(name, "-libxpm")),
                            Some(&converted),
                            Some("libxpm version for gd"),
                            &[],
                            &[],
                        )?;
                    }
                }

                b"gmp" => {
                    let gmp_version = platform_info.get_constant("GMP_VERSION", None);
                    let gmp_version_str = match &gmp_version {
                        PhpMixed::String(s) => Some(s.clone()),
                        _ => None,
                    };
                    self.add_library(
                        &mut libraries,
                        name.as_bstr(),
                        gmp_version_str.as_deref(),
                        None::<&str>,
                        &[],
                        &[],
                    )?;
                }

                b"iconv" => {
                    let iconv_version = platform_info.get_constant("ICONV_VERSION", None);
                    let iconv_version_str = match &iconv_version {
                        PhpMixed::String(s) => Some(s.clone()),
                        _ => None,
                    };
                    self.add_library(
                        &mut libraries,
                        name.as_bstr(),
                        iconv_version_str.as_deref(),
                        None::<&str>,
                        &[],
                        &[],
                    )?;
                }

                b"intl" => {
                    let info = platform_info.get_extension_info(name);

                    let description = "The ICU unicode and globalization support library";
                    // Truthy check is for testing only so we can make the condition fail
                    if platform_info.has_constant("INTL_ICU_VERSION", None) {
                        let intl_icu_version = platform_info.get_constant("INTL_ICU_VERSION", None);
                        let intl_icu_str = match &intl_icu_version {
                            PhpMixed::String(s) => Some(s.clone()),
                            _ => None,
                        };
                        self.add_library(
                            &mut libraries,
                            bstr::BStr::new("icu"),
                            intl_icu_str.as_deref(),
                            Some(description),
                            &[],
                            &[],
                        )?;
                    } else {
                        if let Some(matches) =
                            preg_match(php_regex!("/^ICU version => (?<version>.+)$/im"), info)
                        {
                            self.add_library(
                                &mut libraries,
                                bstr::BStr::new("icu"),
                                matches.name("version"),
                                Some(description),
                                &[],
                                &[],
                            )?;
                        }
                    }

                    // ICU TZData version => 2019c
                    if let Some(zoneinfo_matches) = preg_match(
                        php_regex!("/^ICU TZData version => (?<version>.*)$/im"),
                        info,
                    ) {
                        let zi_version =
                            BString::from(zoneinfo_matches.name("version").unwrap_or_default());
                        if let Some(parsed) =
                            Version::parse_zoneinfo_version(&legacy_lossy(&zi_version))
                        {
                            self.add_library(
                                &mut libraries,
                                bstr::BStr::new("icu-zoneinfo"),
                                Some(&parsed),
                                Some("zoneinfo (\"Olson\") database for icu"),
                                &[],
                                &[],
                            )?;
                        }
                    }

                    // Add a separate version for the CLDR library version
                    if platform_info.has_class("ResourceBundle") {
                        let resource_bundle = &platform_info.resource_bundle;
                        if !matches!(resource_bundle, PhpMixed::Null) {
                            let version_value =
                                Self::resource_bundle_get(resource_bundle, "Version");
                            let version_str = match version_value {
                                PhpMixed::String(s) => Some(s.clone()),
                                _ => None,
                            };
                            self.add_library(
                                &mut libraries,
                                bstr::BStr::new("icu-cldr"),
                                version_str.as_deref(),
                                Some("ICU CLDR project version"),
                                &[],
                                &[],
                            )?;
                        }
                    }

                    if platform_info.has_class("IntlChar") {
                        let intl_char_versions = &platform_info.intl_char_unicode_version;
                        let sliced = array_slice_mixed(intl_char_versions, 0, Some(3));
                        let joined = BString::from(
                            &implode(".", &Self::php_array_to_string_vec(&sliced))[..],
                        );
                        self.add_library(
                            &mut libraries,
                            bstr::BStr::new("icu-unicode"),
                            Some(&joined),
                            Some("ICU unicode version"),
                            &[],
                            &[],
                        )?;
                    }
                }

                b"imagick" => {
                    let image_magick_version = &platform_info.imagick;
                    let image_magick_version_str =
                        Self::imagick_get_version_string(image_magick_version);
                    // 6.x: ImageMagick 6.2.9 08/24/06 Q16 http://www.imagemagick.org
                    // 7.x: ImageMagick 7.0.8-34 Q16 x86_64 2019-03-23 https://imagemagick.org
                    if let Some(matches) = preg_match(
                        php_regex!("/^ImageMagick (?<version>[\\d.]+)(?:-(?<patch>\\d+))?/"),
                        &image_magick_version_str,
                    ) {
                        let mut version_built =
                            BString::from(matches.name("version").unwrap_or_default());
                        if let Some(patch) = matches.name("patch") {
                            version_built = bconcat!(version_built, ".", patch);
                        }

                        self.add_library(
                            &mut libraries,
                            bstr::BStr::new(&bconcat!(name, "-imagemagick")),
                            Some(&version_built),
                            None::<&str>,
                            &[bstr::BString::from("imagick".to_string())],
                            &[],
                        )?;
                    }
                }

                b"ldap" => {
                    let info = platform_info.get_extension_info(name);

                    if let Some(matches) = preg_match(
                        php_regex!("/^Vendor Version => (?<versionId>\\d+)$/im"),
                        info,
                    ) && let Some(vendor_matches) =
                        preg_match(php_regex!("/^Vendor Name => (?<vendor>.+)$/im"), info)
                    {
                        let version_id: i64 = matches
                            .name("versionId")
                            .and_then(|s| legacy_lossy(s).parse().ok())
                            .unwrap_or(0);
                        let converted = Version::convert_openldap_version_id(version_id);
                        let vendor =
                            BString::from(vendor_matches.name("vendor").unwrap_or_default());
                        self.add_library(
                            &mut libraries,
                            bstr::BStr::new(&bconcat!(name, "-", strtolower(&vendor))),
                            Some(&converted),
                            Some(&*bconcat!(vendor, " version of ldap")),
                            &[],
                            &[],
                        )?;
                    }
                }

                b"libxml" => {
                    // ext/dom, ext/simplexml, ext/xmlreader and ext/xmlwriter use the same libxml as the ext/libxml
                    let target_exts = ["dom", "simplexml", "xml", "xmlreader", "xmlwriter"];
                    let intersected: Vec<BString> = loaded_extensions
                        .iter()
                        .filter(|e| target_exts.iter().any(|t| e == t))
                        .cloned()
                        .collect();
                    let libxml_provides: Vec<BString> = intersected
                        .iter()
                        .map(|extension| bconcat!(extension, "-libxml"))
                        .collect();
                    let libxml_dotted = platform_info.get_constant("LIBXML_DOTTED_VERSION", None);
                    let libxml_dotted_str = match &libxml_dotted {
                        PhpMixed::String(s) => Some(s.clone()),
                        _ => None,
                    };
                    self.add_library(
                        &mut libraries,
                        name.as_bstr(),
                        libxml_dotted_str.as_deref(),
                        Some("libxml library version"),
                        &[],
                        &libxml_provides,
                    )?;
                }

                b"mbstring" => {
                    let info = platform_info.get_extension_info(name);

                    // libmbfl version => 1.3.2
                    if let Some(libmbfl_matches) =
                        preg_match(php_regex!("/^libmbfl version => (?<version>.+)$/im"), info)
                    {
                        self.add_library(
                            &mut libraries,
                            bstr::BStr::new(&bconcat!(name, "-libmbfl")),
                            libmbfl_matches.name("version"),
                            Some("mbstring libmbfl version"),
                            &[],
                            &[],
                        )?;
                    }

                    if platform_info.has_constant("MB_ONIGURUMA_VERSION", None) {
                        let oniguruma = platform_info.get_constant("MB_ONIGURUMA_VERSION", None);
                        let oniguruma_str = match &oniguruma {
                            PhpMixed::String(s) => Some(s.clone()),
                            _ => None,
                        };
                        self.add_library(
                            &mut libraries,
                            bstr::BStr::new(&bconcat!(name, "-oniguruma")),
                            oniguruma_str.as_deref(),
                            Some("mbstring oniguruma version"),
                            &[],
                            &[],
                        )?;

                    // Multibyte regex (oniguruma) version => 5.9.5
                    // oniguruma version => 6.9.0
                    } else {
                        if let Some(oniguruma_matches) = preg_match(
                            php_regex!(
                                "/^(?:oniguruma|Multibyte regex \\(oniguruma\\)) version => (?<version>.+)$/im"
                            ),
                            info,
                        ) {
                            self.add_library(
                                &mut libraries,
                                bstr::BStr::new(&bconcat!(name, "-oniguruma")),
                                oniguruma_matches.name("version"),
                                Some("mbstring oniguruma version"),
                                &[],
                                &[],
                            )?;
                        }
                    }
                }

                b"memcached" => {
                    let info = platform_info.get_extension_info(name);

                    // libmemcached version => 1.0.18
                    if let Some(matches) = preg_match(
                        php_regex!("/^libmemcached version => (?<version>.+)$/im"),
                        info,
                    ) {
                        self.add_library(
                            &mut libraries,
                            bstr::BStr::new(&bconcat!(name, "-libmemcached")),
                            matches.name("version"),
                            Some("libmemcached version"),
                            &[],
                            &[],
                        )?;
                    }
                }

                b"openssl" => {
                    let openssl_text = platform_info.get_constant("OPENSSL_VERSION_TEXT", None);
                    let openssl_text_str = legacy_lossy(&match &openssl_text {
                        PhpMixed::String(s) => s.clone(),
                        _ => bstr::BString::from("".to_string()),
                    })
                    .into_owned();
                    // OpenSSL 1.1.1g  21 Apr 2020
                    if let Some(matches) = preg_match(
                        php_regex!("{^(?:OpenSSL|LibreSSL)?\\s*(?<version>\\S+)}i"),
                        &openssl_text_str,
                    ) {
                        let version = BString::from(matches.name("version").unwrap_or_default());
                        let mut is_fips = false;
                        let parsed_version =
                            Version::parse_openssl(&legacy_lossy(&version), &mut is_fips)
                                .unwrap_or_default();
                        let mut provides_list: Vec<BString> = Vec::new();
                        if is_fips {
                            provides_list.push(name.clone());
                        }
                        self.add_library(
                            &mut libraries,
                            bstr::BStr::new(&format!(
                                "{}{}",
                                legacy_lossy(&name),
                                if is_fips { "-fips" } else { "" }
                            )),
                            Some(&parsed_version),
                            Some(&openssl_text_str),
                            &[],
                            &provides_list,
                        )?;
                    }
                }

                b"pcre" => {
                    let pcre_version = platform_info.get_constant("PCRE_VERSION", None);
                    let pcre_version_str = match &pcre_version {
                        PhpMixed::String(s) => s.clone(),
                        _ => bstr::BString::from("".to_string()),
                    };
                    let stripped = BString::from(
                        &preg_replace(php_regex!("{^(\\S+).*}"), "$1", &pcre_version_str)[..],
                    );
                    self.add_library(
                        &mut libraries,
                        name.as_bstr(),
                        Some(&stripped),
                        None::<&str>,
                        &[],
                        &[],
                    )?;

                    let info = platform_info.get_extension_info(name);

                    // PCRE Unicode Version => 12.1.0
                    if let Some(pcre_unicode_matches) = preg_match(
                        php_regex!("/^PCRE Unicode Version => (?<version>.+)$/im"),
                        info,
                    ) {
                        self.add_library(
                            &mut libraries,
                            bstr::BStr::new(&bconcat!(name, "-unicode")),
                            pcre_unicode_matches.name("version"),
                            Some("PCRE Unicode version support"),
                            &[],
                            &[],
                        )?;
                    }
                }

                b"mysqlnd" | b"pdo_mysql" => {
                    let info = platform_info.get_extension_info(name);

                    if let Some(matches) = preg_match(
                        php_regex!(
                            "/^(?:Client API version|Version) => mysqlnd (?<version>.+?) /mi"
                        ),
                        info,
                    ) {
                        self.add_library(
                            &mut libraries,
                            bstr::BStr::new(&bconcat!(name, "-mysqlnd")),
                            matches.name("version"),
                            Some(&*bconcat!("mysqlnd library version for ", name)),
                            &[],
                            &[],
                        )?;
                    }
                }

                b"mongodb" => {
                    let info = platform_info.get_extension_info(name);

                    if let Some(libmongoc_matches) = preg_match(
                        php_regex!("/^libmongoc bundled version => (?<version>.+)$/im"),
                        info,
                    ) {
                        self.add_library(
                            &mut libraries,
                            bstr::BStr::new(&bconcat!(name, "-libmongoc")),
                            libmongoc_matches.name("version"),
                            Some("libmongoc version of mongodb"),
                            &[],
                            &[],
                        )?;
                    }

                    if let Some(libbson_matches) = preg_match(
                        php_regex!("/^libbson bundled version => (?<version>.+)$/im"),
                        info,
                    ) {
                        self.add_library(
                            &mut libraries,
                            bstr::BStr::new(&bconcat!(name, "-libbson")),
                            libbson_matches.name("version"),
                            Some("libbson version of mongodb"),
                            &[],
                            &[],
                        )?;
                    }
                }

                b"pgsql" => {
                    if platform_info.has_constant("PGSQL_LIBPQ_VERSION", None) {
                        let pq_version = platform_info.get_constant("PGSQL_LIBPQ_VERSION", None);
                        let pq_version_str = match &pq_version {
                            PhpMixed::String(s) => Some(s.clone()),
                            _ => None,
                        };
                        self.add_library(
                            &mut libraries,
                            bstr::BStr::new("pgsql-libpq"),
                            pq_version_str.as_deref(),
                            Some("libpq for pgsql"),
                            &[],
                            &[],
                        )?;
                    } else {
                        // intentional fall-through to next case...
                        let info = platform_info.get_extension_info(name);

                        if let Some(matches) = preg_match(
                            php_regex!("/^PostgreSQL\\(libpq\\) Version => (?<version>.*)$/im"),
                            info,
                        ) {
                            self.add_library(
                                &mut libraries,
                                bstr::BStr::new(&bconcat!(name, "-libpq")),
                                matches.name("version"),
                                Some(&*bconcat!("libpq for ", name)),
                                &[],
                                &[],
                            )?;
                        }
                    }
                }

                b"pdo_pgsql" => {
                    let info = platform_info.get_extension_info(name);

                    if let Some(matches) = preg_match(
                        php_regex!("/^PostgreSQL\\(libpq\\) Version => (?<version>.*)$/im"),
                        info,
                    ) {
                        self.add_library(
                            &mut libraries,
                            bstr::BStr::new(&bconcat!(name, "-libpq")),
                            matches.name("version"),
                            Some(&*bconcat!("libpq for ", name)),
                            &[],
                            &[],
                        )?;
                    }
                }

                b"pq" => {
                    let info = platform_info.get_extension_info(name);

                    // Used Library => Compiled => Linked
                    // libpq => 14.3 (Ubuntu 14.3-1.pgdg22.04+1) => 15.0.2
                    if let Some(matches) = preg_match(
                        php_regex!("/^libpq => (?<compiled>.+) => (?<linked>.+)$/im"),
                        info,
                    ) {
                        self.add_library(
                            &mut libraries,
                            bstr::BStr::new(&bconcat!(name, "-libpq")),
                            matches.name("linked"),
                            Some(&*bconcat!("libpq for ", name)),
                            &[],
                            &[],
                        )?;
                    }
                }

                b"rdkafka" => {
                    if platform_info.has_constant("RD_KAFKA_VERSION", None) {
                        // Interpreted as hex MM.mm.rr.xx:
                        //  - MM = Major
                        //  - mm = minor
                        //  - rr = revision
                        //  - xx = pre-release id (0xff is the final release)
                        //
                        // pre-release ID in practice is always 0xff even for RCs etc, so we ignore it
                        let lib_rd_kafka_version_int = platform_info
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
                            bstr::BStr::new(&bconcat!(name, "-librdkafka")),
                            Some(&version_built),
                            Some(&*bconcat!("librdkafka for ", name)),
                            &[],
                            &[],
                        )?;
                    }
                }

                b"libsodium" | b"sodium" => {
                    if platform_info.has_constant("SODIUM_LIBRARY_VERSION", None) {
                        let sodium = platform_info.get_constant("SODIUM_LIBRARY_VERSION", None);
                        let sodium_str = match &sodium {
                            PhpMixed::String(s) => Some(s.clone()),
                            _ => None,
                        };
                        self.add_library(
                            &mut libraries,
                            bstr::BStr::new("libsodium"),
                            sodium_str.as_deref(),
                            None::<&str>,
                            &[],
                            &[],
                        )?;
                        self.add_library(
                            &mut libraries,
                            bstr::BStr::new("libsodium"),
                            sodium_str.as_deref(),
                            None::<&str>,
                            &[],
                            &[],
                        )?;
                    }
                }

                b"sqlite3" | b"pdo_sqlite" => {
                    let info = platform_info.get_extension_info(name);

                    if let Some(matches) =
                        preg_match(php_regex!("/^SQLite Library => (?<version>.+)$/im"), info)
                    {
                        self.add_library(
                            &mut libraries,
                            bstr::BStr::new(&bconcat!(name, "-sqlite")),
                            matches.name("version"),
                            None::<&str>,
                            &[],
                            &[],
                        )?;
                    }
                }

                b"ssh2" => {
                    let info = platform_info.get_extension_info(name);

                    if let Some(matches) =
                        preg_match(php_regex!("/^libssh2 version => (?<version>.+)$/im"), info)
                    {
                        self.add_library(
                            &mut libraries,
                            bstr::BStr::new(&bconcat!(name, "-libssh2")),
                            matches.name("version"),
                            None::<&str>,
                            &[],
                            &[],
                        )?;
                    }
                }

                b"xsl" => {
                    let libxslt_version =
                        platform_info.get_constant("LIBXSLT_DOTTED_VERSION", None);
                    let libxslt_str = match &libxslt_version {
                        PhpMixed::String(s) => Some(s.clone()),
                        _ => None,
                    };
                    self.add_library(
                        &mut libraries,
                        bstr::BStr::new("libxslt"),
                        libxslt_str.as_deref(),
                        None::<&str>,
                        &[bstr::BString::from("xsl".to_string())],
                        &[],
                    )?;

                    let info = platform_info.get_extension_info("xsl");
                    if let Some(matches) = preg_match(
                        php_regex!(
                            "/^libxslt compiled against libxml Version => (?<version>.+)$/im"
                        ),
                        info,
                    ) {
                        self.add_library(
                            &mut libraries,
                            bstr::BStr::new("libxslt-libxml"),
                            matches.name("version"),
                            Some("libxml version libxslt is compiled against"),
                            &[],
                            &[],
                        )?;
                    }
                }

                b"yaml" => {
                    let info = platform_info.get_extension_info("yaml");

                    if let Some(matches) =
                        preg_match(php_regex!("/^LibYAML Version => (?<version>.+)$/im"), info)
                    {
                        self.add_library(
                            &mut libraries,
                            bstr::BStr::new(&bconcat!(name, "-libyaml")),
                            matches.name("version"),
                            Some("libyaml version of yaml"),
                            &[],
                            &[],
                        )?;
                    }
                }

                b"zip" => {
                    if platform_info.has_constant("LIBZIP_VERSION", Some("ZipArchive")) {
                        let libzip =
                            platform_info.get_constant("LIBZIP_VERSION", Some("ZipArchive"));
                        let libzip_str = match &libzip {
                            PhpMixed::String(s) => Some(s.clone()),
                            _ => None,
                        };
                        self.add_library(
                            &mut libraries,
                            bstr::BStr::new(&bconcat!(name, "-libzip")),
                            libzip_str.as_deref(),
                            None::<&str>,
                            &[bstr::BString::from("zip".to_string())],
                            &[],
                        )?;
                    }
                }

                b"zlib" => {
                    if platform_info.has_constant("ZLIB_VERSION", None) {
                        let zlib = platform_info.get_constant("ZLIB_VERSION", None);
                        let zlib_str = match &zlib {
                            PhpMixed::String(s) => Some(s.clone()),
                            _ => None,
                        };
                        self.add_library(
                            &mut libraries,
                            name.as_bstr(),
                            zlib_str.as_deref(),
                            None::<&str>,
                            &[],
                            &[],
                        )?;

                    // Linked Version => 1.2.8
                    } else {
                        let info = platform_info.get_extension_info(name);
                        if let Some(matches) =
                            preg_match(php_regex!("/^Linked Version => (?<version>.+)$/im"), info)
                        {
                            self.add_library(
                                &mut libraries,
                                name.as_bstr(),
                                matches.name("version"),
                                None::<&str>,
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
                    pretty_version = legacy_lossy(&BString::from(
                        &preg_replace(php_regex!("#^([^~+-]+).*$#"), "$1", &hhvm_version)[..],
                    ))
                    .into_owned();
                    version = self
                        .version_parser
                        .as_ref()
                        .unwrap()
                        .normalize(&pretty_version, None)?;
                }
            }

            let mut hhvm = CompletePackage::new(
                bstr::BString::from("hhvm".to_string()),
                version,
                bstr::BString::from(pretty_version),
            );
            hhvm.set_description("The HHVM Runtime (64bit)".into());
            self.add_package(CompletePackageHandle::from_complete_package(hhvm).into())?;
        }
        Ok(())
    }

    pub fn add_package(&mut self, package: PackageInterfaceHandle) -> anyhow::Result<()> {
        if package.as_complete().is_none() {
            return Err(UnexpectedValueException::new(format!(
                "Expected CompletePackage but got {}",
                get_class(&PhpMixed::Null).display_lossy()
            ))
            .into());
        }

        let name = package.get_name();

        // Skip if overridden
        if self.overrides.contains_key(&name) {
            if matches!(self.overrides[&name].version, PhpMixed::Bool(false)) {
                self.add_disabled_package_from_pkg(package);
                return Ok(());
            }

            // PHP: $this->findPackage() reaches ArrayRepository::findPackage, whose
            // getPackages() call late-binds to PlatformRepository::initialize.
            self.ensure_initialized()?;
            let overrider = self.inner.find_package(
                name.as_bstr(),
                crate::repository::FindPackageConstraint::String("*".into()),
            )?;
            let actual_text = if let Some(ref ov) = overrider {
                if package.get_version() == ov.get_version() {
                    BString::from("same as actual")
                } else {
                    bconcat!("actual: ", package.get_pretty_version())
                }
            } else {
                bconcat!("actual: ", package.get_pretty_version())
            };
            if let Some(overrider) = overrider
                && let Some(overrider) = overrider.as_complete()
            {
                let description = overrider.get_description().unwrap_or_default();
                overrider.set_description(bconcat!(description, ", ", actual_text));
            }

            return Ok(());
        }

        // Skip if PHP is overridden and we are adding a php-* package
        if self.overrides.contains_key(b"php".as_slice()) && strpos(&name, "php-") == Some(0) {
            let php_override = PlatformOverride {
                name: self.overrides[b"php".as_slice()].name.clone(),
                version: self.overrides[b"php".as_slice()].version.clone(),
            };
            let overrider =
                self.add_overridden_package(&php_override, Some(package.get_pretty_name()))?;
            let actual_text = if package.get_version() == overrider.get_version() {
                BString::from("same as actual")
            } else {
                bconcat!("actual: ", package.get_pretty_version())
            };
            let current_description = overrider.get_description().unwrap_or_default();
            overrider.set_description(bconcat!(current_description, ", ", actual_text));

            return Ok(());
        }

        // PHP: parent::addPackage() calls the late-bound $this->initialize() before pushing.
        self.ensure_initialized()?;
        self.inner.add_package(package)?;
        Ok(())
    }

    fn add_overridden_package(
        &mut self,
        r#override: &PlatformOverride,
        name: Option<BString>,
    ) -> anyhow::Result<CompletePackageHandle> {
        let version_str = match &r#override.version {
            PhpMixed::String(s) => s.clone(),
            _ => bstr::BString::from("".to_string()),
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
        package.set_description(bstr::BString::from(
            "Package overridden via config.platform".to_string(),
        ));
        let mut extra: IndexMap<BString, PhpMixed> = IndexMap::new();
        extra.insert("config.platform".into(), PhpMixed::Bool(true));
        package.inner.set_extra(extra);
        let package = CompletePackageHandle::from_complete_package(package);
        // PHP: parent::addPackage() calls the late-bound $this->initialize() before pushing.
        self.ensure_initialized()?;
        self.inner.add_package(package.clone().into())?;

        if package.get_name() == "php" {
            let parts = explode(".", package.get_version());
            let head = array_slice_strs(&parts.to_vec(), 0, Some(3));
            *LAST_SEEN_PLATFORM_PHP.lock().unwrap() = Some(BString::from(&implode(".", &head)[..]));
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
        package.set_description(bconcat!(
            current_description,
            ". <warning>Package disabled via config.platform</warning>"
        ));
        let mut extra: IndexMap<BString, PhpMixed> = IndexMap::new();
        extra.insert("config.platform".into(), PhpMixed::Bool(true));
        // TODO(type-model): neither PackageInterface nor CompletePackageInterface exposes
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
    fn add_extension(&mut self, name: &BStr, pretty_version: &BStr) -> anyhow::Result<()> {
        let mut extra_description: Option<BString> = None;
        let mut pretty_version = BString::from(pretty_version);

        let version = match self
            .version_parser
            .as_ref()
            .unwrap()
            .normalize(&pretty_version, None)
        {
            Ok(v) => v,
            Err(_) => {
                extra_description = Some(bconcat!(" (actual version: ", pretty_version, ")"));
                if let Some(m) = preg_match(
                    php_regex!("{^(\\d+\\.\\d+\\.\\d+(?:\\.\\d+)?)}"),
                    &pretty_version,
                ) {
                    pretty_version = BString::from(m.get(1).unwrap_or_default());
                } else {
                    pretty_version = "0".into();
                }
                self.version_parser
                    .as_ref()
                    .unwrap()
                    .normalize(&pretty_version, None)?
            }
        };

        let package_name = self.build_package_name(name);
        let mut ext = CompletePackage::new(package_name, version.clone(), pretty_version);
        ext.set_description(bconcat!(
            "The ",
            name,
            " PHP extension",
            extra_description.unwrap_or_default()
        ));
        ext.inner.set_type("php-ext".into());

        if name == "uuid" {
            let mut replaces: IndexMap<BString, Link> = IndexMap::new();
            replaces.insert(
                "lib-uuid".into(),
                Link::new(
                    "ext-uuid".into(),
                    "lib-uuid".into(),
                    SimpleConstraint::new("=", version, None).into(),
                    Some(Link::TYPE_REPLACE.into()),
                    BString::from(ext.get_pretty_version()),
                ),
            );
            ext.inner.set_replaces(replaces);
        }

        self.add_package(CompletePackageHandle::from_complete_package(ext).into())?;
        Ok(())
    }

    fn build_package_name(&self, name: &BStr) -> BString {
        bconcat!("ext-", str_replace(" ", "-", strtolower(name)))
    }

    fn add_library(
        &mut self,
        libraries: &mut IndexMap<BString, bool>,
        name: &BStr,
        pretty_version: Option<impl AsRef<[u8]>>,
        description: Option<&(impl AsRef<[u8]> + ?Sized)>,
        replaces: &[BString],
        provides: &[BString],
    ) -> anyhow::Result<()> {
        let pretty_version = match &pretty_version {
            Some(v) => v.as_ref().as_bstr(),
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
        if libraries.contains_key(&bconcat!("lib-", name)) {
            return Ok(());
        }
        libraries.insert(bconcat!("lib-", name), true);

        let description = description
            .map(|d| BString::from(d.as_ref()))
            .unwrap_or_else(|| bconcat!("The ", name, " library"));

        let mut lib = CompletePackage::new(
            bconcat!("lib-", name),
            version.clone(),
            bstr::BString::from(pretty_version),
        );
        lib.set_description(description);

        let mut replace_links: IndexMap<BString, Link> = IndexMap::new();
        for replace in replaces {
            let replace_lower = BString::from(&strtolower(replace)[..]);
            replace_links.insert(
                replace_lower.clone(),
                Link::new(
                    bconcat!("lib-", name),
                    bconcat!("lib-", replace_lower),
                    SimpleConstraint::new("=", BString::from(&version[..]), None).into(),
                    Some(bstr::BString::from(Link::TYPE_REPLACE.to_string())),
                    bstr::BString::from(lib.get_pretty_version()),
                ),
            );
        }
        let mut provide_links: IndexMap<BString, Link> = IndexMap::new();
        for provide in provides {
            let provide_lower = BString::from(&strtolower(provide)[..]);
            provide_links.insert(
                provide_lower.clone(),
                Link::new(
                    bconcat!("lib-", name),
                    bconcat!("lib-", provide_lower),
                    SimpleConstraint::new("=", BString::from(&version[..]), None).into(),
                    Some(bstr::BString::from(Link::TYPE_PROVIDE.to_string())),
                    bstr::BString::from(lib.get_pretty_version()),
                ),
            );
        }
        lib.inner.set_replaces(replace_links);
        lib.inner.set_provides(provide_links);

        self.add_package(CompletePackageHandle::from_complete_package(lib).into())?;
        Ok(())
    }

    /// Check if a package name is a platform package.
    pub fn is_platform_package(name: impl AsRef<[u8]>) -> bool {
        let name = name.as_ref();
        let mut cache = IS_PLATFORM_PACKAGE_CACHE.lock().unwrap();

        if let Some(&cached) = cache.get(name.as_bstr()) {
            return cached;
        }

        let result = preg_is_match(Self::PLATFORM_PACKAGE_REGEX, name);
        cache.insert(BString::from(name), result);
        result
    }

    /// Returns the last seen config.platform.php version if defined
    ///
    /// This is a best effort attempt for internal purposes, retrieve the real
    /// packages from a PlatformRepository instance if you need a version guaranteed to
    /// be correct.
    pub fn get_platform_php_version() -> Option<BString> {
        LAST_SEEN_PLATFORM_PHP.lock().unwrap().clone()
    }

    pub fn search(
        &mut self,
        query: BString,
        mode: i64,
        r#type: Option<BString>,
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

    /// PHP `$resourceBundle->get($key)`. A live PHP object has no `PhpMixed` counterpart, so
    /// [`PlatformInfo`] carries the entries the caller reads instead of the object.
    fn resource_bundle_get(value: &PhpMixed, key: &str) -> PhpMixed {
        Self::php_object_field(value, key).unwrap_or(PhpMixed::Null)
    }

    /// PHP `$imagick->getVersion()['versionString']`, read the same way.
    fn imagick_get_version_string(value: &PhpMixed) -> BString {
        match Self::php_object_field(value, "versionString") {
            Some(PhpMixed::String(version)) => version.clone(),
            _ => BString::default(),
        }
    }

    fn php_object_field(value: &PhpMixed, key: &str) -> Option<PhpMixed> {
        match value {
            PhpMixed::Object(fields) | PhpMixed::Array(fields) => fields.get(B(key)).cloned(),
            _ => None,
        }
    }

    fn php_array_to_string_vec(value: &PhpMixed) -> Vec<BString> {
        match value {
            PhpMixed::List(list) => list
                .iter()
                .map(|v| match v {
                    PhpMixed::String(s) => s.clone(),
                    PhpMixed::Int(i) => bstr::BString::from(i.to_string()),
                    PhpMixed::Float(f) => bstr::BString::from(f.to_string()),
                    _ => bstr::BString::from("".to_string()),
                })
                .collect(),
            PhpMixed::Array(m) => m
                .values()
                .map(|v| match v {
                    PhpMixed::String(s) => s.clone(),
                    PhpMixed::Int(i) => bstr::BString::from(i.to_string()),
                    PhpMixed::Float(f) => bstr::BString::from(f.to_string()),
                    _ => bstr::BString::from("".to_string()),
                })
                .collect(),
            _ => Vec::new(),
        }
    }
}

impl crate::repository::RepositoryInterface for PlatformRepository {
    // PHP's ArrayRepository::count()/hasPackage() call $this->initialize(), which
    // virtual-dispatches to PlatformRepository::initialize(); the guard restores that.
    fn count(&mut self) -> anyhow::Result<usize> {
        self.ensure_initialized()?;
        self.inner.count()
    }

    fn has_package(&mut self, package: PackageInterfaceHandle) -> anyhow::Result<bool> {
        self.ensure_initialized()?;
        self.inner.has_package(package)
    }

    fn find_package(
        &mut self,
        name: &BStr,
        constraint: crate::repository::FindPackageConstraint,
    ) -> anyhow::Result<Option<crate::package::BasePackageHandle>> {
        self.ensure_initialized()?;
        self.inner.find_package(name, constraint)
    }

    fn find_packages(
        &mut self,
        name: &BStr,
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
        package_name_map: IndexMap<BString, Option<shirabe_semver::constraint::AnyConstraint>>,
        acceptable_stabilities: IndexMap<BString, i64>,
        stability_flags: IndexMap<BString, i64>,
        already_loaded: IndexMap<
            BString,
            IndexMap<BString, crate::package::PackageInterfaceHandle>,
        >,
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
        query: BString,
        mode: i64,
        r#type: Option<BString>,
    ) -> anyhow::Result<Vec<crate::repository::SearchResult>> {
        PlatformRepository::search(self, query, mode, r#type)
    }

    fn get_providers(
        &mut self,
        package_name: BString,
    ) -> anyhow::Result<IndexMap<BString, crate::repository::ProviderInfo>> {
        self.ensure_initialized()?;
        self.inner.get_providers(package_name)
    }

    fn get_repo_name(&self) -> anyhow::Result<BString> {
        Ok(PlatformRepository::get_repo_name(self))
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }

    fn set_self_handle(&self, weak: crate::repository::RepositoryInterfaceWeakHandle) {
        self.inner.set_self_handle(weak);
    }
}
