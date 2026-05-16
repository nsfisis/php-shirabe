//! ref: composer/src/Composer/Package/PackageInterface.php

use chrono::{DateTime, Utc};
use indexmap::IndexMap;
use shirabe_php_shim::PhpMixed;

use crate::package::link::Link;
use crate::repository::repository_interface::RepositoryInterface;

/// Defines the essential information a package has that is used during solving/installation
///
/// PackageInterface & derivatives are considered internal, you may use them in type hints but extending/implementing them is not recommended and not supported. Things may change without notice.
///
/// @phpstan-type AutoloadRules    array{psr-0?: array<string, string|string[]>, psr-4?: array<string, string|string[]>, classmap?: list<string>, files?: list<string>, exclude-from-classmap?: list<string>}
/// @phpstan-type DevAutoloadRules array{psr-0?: array<string, string|string[]>, psr-4?: array<string, string|string[]>, classmap?: list<string>, files?: list<string>}
/// @phpstan-type PhpExtConfig     array{extension-name?: string, priority?: int, support-zts?: bool, support-nts?: bool, build-path?: string|null, download-url-method?: string|list<string>, os-families?: non-empty-list<non-empty-string>, os-families-exclude?: non-empty-list<non-empty-string>, configure-options?: list<array{name: string, description?: string}>}
pub trait PackageInterface: std::fmt::Display {
    /// Returns the package's name without version info, thus not a unique identifier
    ///
    /// @return string package name
    fn get_name(&self) -> &str;

    /// Returns the package's pretty (i.e. with proper case) name
    ///
    /// @return string package name
    fn get_pretty_name(&self) -> &str;

    /// Returns a set of names that could refer to this package
    ///
    /// No version or release type information should be included in any of the
    /// names. Provided or replaced package names need to be returned as well.
    ///
    /// @param bool $provides Whether provided names should be included
    ///
    /// @return string[] An array of strings referring to this package
    fn get_names(&self, provides: bool) -> Vec<String>;

    /// Allows the solver to set an id for this package to refer to it.
    fn set_id(&mut self, id: i64);

    /// Retrieves the package's id set through setId
    ///
    /// @return int The previously set package id
    fn get_id(&self) -> i64;

    /// Returns whether the package is a development virtual package or a concrete one
    fn is_dev(&self) -> bool;

    /// Returns the package type, e.g. library
    ///
    /// @return string The package type
    fn get_type(&self) -> &str;

    /// Returns the package targetDir property
    ///
    /// @return ?string The package targetDir
    fn get_target_dir(&self) -> Option<&str>;

    /// Returns the package extra data
    ///
    /// @return mixed[] The package extra data
    fn get_extra(&self) -> IndexMap<String, PhpMixed>;

    /// Sets source from which this package was installed (source/dist).
    ///
    /// @param ?string $type source/dist
    /// @phpstan-param 'source'|'dist'|null $type
    fn set_installation_source(&mut self, r#type: Option<String>);

    /// Returns source from which this package was installed (source/dist).
    ///
    /// @return ?string source/dist
    /// @phpstan-return 'source'|'dist'|null
    fn get_installation_source(&self) -> Option<&str>;

    /// Returns the repository type of this package, e.g. git, svn
    ///
    /// @return ?string The repository type
    fn get_source_type(&self) -> Option<&str>;

    /// Returns the repository url of this package, e.g. git://github.com/naderman/composer.git
    ///
    /// @return ?string The repository url
    fn get_source_url(&self) -> Option<&str>;

    /// Returns the repository urls of this package including mirrors, e.g. git://github.com/naderman/composer.git
    ///
    /// @return list<string>
    fn get_source_urls(&self) -> Vec<String>;

    /// Returns the repository reference of this package, e.g. master, 1.0.0 or a commit hash for git
    ///
    /// @return ?string The repository reference
    fn get_source_reference(&self) -> Option<&str>;

    /// Returns the source mirrors of this package
    ///
    /// @return ?list<array{url: non-empty-string, preferred: bool}>
    fn get_source_mirrors(&self) -> Option<Vec<IndexMap<String, PhpMixed>>>;

    /// @param  null|list<array{url: non-empty-string, preferred: bool}> $mirrors
    fn set_source_mirrors(&mut self, mirrors: Option<Vec<IndexMap<String, PhpMixed>>>);

    /// Returns the type of the distribution archive of this version, e.g. zip, tarball
    ///
    /// @return ?string The repository type
    fn get_dist_type(&self) -> Option<&str>;

    /// Returns the url of the distribution archive of this version
    ///
    /// @return ?non-empty-string
    fn get_dist_url(&self) -> Option<&str>;

    /// Returns the urls of the distribution archive of this version, including mirrors
    ///
    /// @return non-empty-string[]
    fn get_dist_urls(&self) -> Vec<String>;

    /// Returns the reference of the distribution archive of this version, e.g. master, 1.0.0 or a commit hash for git
    fn get_dist_reference(&self) -> Option<&str>;

    /// Returns the sha1 checksum for the distribution archive of this version
    ///
    /// Can be an empty string which should be treated as null
    fn get_dist_sha1_checksum(&self) -> Option<&str>;

    /// Returns the dist mirrors of this package
    ///
    /// @return ?list<array{url: non-empty-string, preferred: bool}>
    fn get_dist_mirrors(&self) -> Option<Vec<IndexMap<String, PhpMixed>>>;

    /// @param  null|list<array{url: non-empty-string, preferred: bool}> $mirrors
    fn set_dist_mirrors(&mut self, mirrors: Option<Vec<IndexMap<String, PhpMixed>>>);

    /// Returns the version of this package
    ///
    /// @return string version
    fn get_version(&self) -> &str;

    /// Returns the pretty (i.e. non-normalized) version string of this package
    ///
    /// @return string version
    fn get_pretty_version(&self) -> &str;

    /// Returns the pretty version string plus a git or hg commit hash of this package
    ///
    /// @see getPrettyVersion
    ///
    /// @param  bool   $truncate    If the source reference is a sha1 hash, truncate it
    /// @param  int    $displayMode One of the DISPLAY_ constants on this interface determining display of references
    /// @return string version
    ///
    /// @phpstan-param self::DISPLAY_SOURCE_REF_IF_DEV|self::DISPLAY_SOURCE_REF|self::DISPLAY_DIST_REF $displayMode
    fn get_full_pretty_version(&self, truncate: bool, display_mode: i64) -> String;

    /// Returns the release date of the package
    fn get_release_date(&self) -> Option<DateTime<Utc>>;

    /// Returns the stability of this package: one of (dev, alpha, beta, RC, stable)
    ///
    /// @phpstan-return 'stable'|'RC'|'beta'|'alpha'|'dev'
    fn get_stability(&self) -> &str;

    /// Returns a set of links to packages which need to be installed before
    /// this package can be installed
    ///
    /// @return array<string, Link> A map of package links defining required packages, indexed by the require package's name
    fn get_requires(&self) -> IndexMap<String, Link>;

    /// Returns a set of links to packages which must not be installed at the
    /// same time as this package
    ///
    /// @return Link[] An array of package links defining conflicting packages
    fn get_conflicts(&self) -> Vec<Link>;

    /// Returns a set of links to virtual packages that are provided through
    /// this package
    ///
    /// @return Link[] An array of package links defining provided packages
    fn get_provides(&self) -> Vec<Link>;

    /// Returns a set of links to packages which can alternatively be
    /// satisfied by installing this package
    ///
    /// @return Link[] An array of package links defining replaced packages
    fn get_replaces(&self) -> Vec<Link>;

    /// Returns a set of links to packages which are required to develop
    /// this package. These are installed if in dev mode.
    ///
    /// @return array<string, Link> A map of package links defining packages required for development, indexed by the require package's name
    fn get_dev_requires(&self) -> IndexMap<String, Link>;

    /// Returns a set of package names and reasons why they are useful in
    /// combination with this package.
    ///
    /// @return array An array of package suggestions with descriptions
    /// @phpstan-return array<string, string>
    fn get_suggests(&self) -> IndexMap<String, String>;

    /// Returns an associative array of autoloading rules
    ///
    /// {"<type>": {"<namespace": "<directory>"}}
    ///
    /// Type is either "psr-4", "psr-0", "classmap" or "files". Namespaces are mapped to
    /// directories for autoloading using the type specified.
    ///
    /// @return array Mapping of autoloading rules
    /// @phpstan-return AutoloadRules
    fn get_autoload(&self) -> IndexMap<String, PhpMixed>;

    /// Returns an associative array of dev autoloading rules
    ///
    /// {"<type>": {"<namespace": "<directory>"}}
    ///
    /// Type is either "psr-4", "psr-0", "classmap" or "files". Namespaces are mapped to
    /// directories for autoloading using the type specified.
    ///
    /// @return array Mapping of dev autoloading rules
    /// @phpstan-return DevAutoloadRules
    fn get_dev_autoload(&self) -> IndexMap<String, PhpMixed>;

    /// Returns a list of directories which should get added to PHP's
    /// include path.
    ///
    /// @return string[]
    fn get_include_paths(&self) -> Vec<String>;

    /// Returns the settings for php extension packages
    ///
    /// @phpstan-return PhpExtConfig|null
    fn get_php_ext(&self) -> Option<IndexMap<String, PhpMixed>>;

    /// Stores a reference to the repository that owns the package
    fn set_repository(&mut self, repository: Box<dyn RepositoryInterface>) -> anyhow::Result<()>;

    /// Returns a reference to the repository that owns the package
    fn get_repository(&self) -> Option<&dyn RepositoryInterface>;

    /// Returns the package binaries
    ///
    /// @return string[]
    fn get_binaries(&self) -> Vec<String>;

    /// Returns package unique name, constructed from name and version.
    fn get_unique_name(&self) -> String;

    /// Returns the package notification url
    fn get_notification_url(&self) -> Option<&str>;

    // PHP: __toString — implemented via std::fmt::Display supertrait

    /// Converts the package into a pretty readable string
    fn get_pretty_string(&self) -> String;

    fn is_default_branch(&self) -> bool;

    /// Returns a list of options to download package dist files
    ///
    /// @return mixed[]
    fn get_transport_options(&self) -> IndexMap<String, PhpMixed>;

    /// Configures the list of options to download package dist files
    ///
    /// @param mixed[] $options
    fn set_transport_options(&mut self, options: IndexMap<String, PhpMixed>);

    fn set_source_reference(&mut self, reference: Option<String>);

    fn set_dist_url(&mut self, url: Option<String>);

    fn set_dist_type(&mut self, r#type: Option<String>);

    fn set_dist_reference(&mut self, reference: Option<String>);

    /// Set dist and source references and update dist URL for ones that contain a reference
    fn set_source_dist_references(&mut self, reference: &str);
}

impl dyn PackageInterface {
    pub const DISPLAY_SOURCE_REF_IF_DEV: i64 = 0;
    pub const DISPLAY_SOURCE_REF: i64 = 1;
    pub const DISPLAY_DIST_REF: i64 = 2;
}
