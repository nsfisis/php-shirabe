//! ref: composer/src/Composer/Package/PackageInterface.php

use crate::package::{Link, Mirror};
use crate::repository::RepositoryInterfaceHandle;
use bstr::{BStr, BString};
use chrono::{DateTime, Utc};
use indexmap::IndexMap;
use shirabe_php_shim::PhpMixed;

/// Selects how `get_full_pretty_version` renders the reference.
///
/// ref: PackageInterface::DISPLAY_SOURCE_REF_IF_DEV
/// ref: PackageInterface::DISPLAY_SOURCE_REF
/// ref: PackageInterface::DISPLAY_DIST_REF
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DisplayMode {
    SourceRefIfDev,
    SourceRef,
    DistRef,
}

/// Defines the essential information a package has that is used during solving/installation
///
/// PackageInterface & derivatives are considered internal, you may use them in type hints but extending/implementing them is not recommended and not supported. Things may change without notice.
pub trait PackageInterface: std::fmt::Debug {
    /// Returns the package's name without version info, thus not a unique identifier
    fn get_name(&self) -> &BStr;

    /// Returns the package's pretty (i.e. with proper case) name
    fn get_pretty_name(&self) -> &BStr;

    /// Returns a set of names that could refer to this package
    ///
    /// No version or release type information should be included in any of the
    /// names. Provided or replaced package names need to be returned as well.
    fn get_names(&self, provides: bool) -> Vec<BString>;

    /// Allows the solver to set an id for this package to refer to it.
    fn set_id(&mut self, id: i64);

    /// Retrieves the package's id set through setId
    fn get_id(&self) -> i64;

    /// Returns whether the package is a development virtual package or a concrete one
    fn is_dev(&self) -> bool;

    /// Returns the package type, e.g. library
    fn get_type(&self) -> BString;

    /// Returns the package targetDir property
    fn get_target_dir(&self) -> Option<BString>;

    /// Returns the package extra data
    fn get_extra(&self) -> IndexMap<BString, PhpMixed>;

    /// Sets source from which this package was installed (source/dist).
    fn set_installation_source(&mut self, r#type: Option<BString>);

    /// Returns source from which this package was installed (source/dist).
    fn get_installation_source(&self) -> Option<BString>;

    /// Returns the repository type of this package, e.g. git, svn
    fn get_source_type(&self) -> Option<BString>;

    /// Returns the repository url of this package, e.g. git://github.com/naderman/composer.git
    fn get_source_url(&self) -> Option<BString>;

    /// Returns the repository urls of this package including mirrors, e.g. git://github.com/naderman/composer.git
    fn get_source_urls(&self) -> Vec<BString>;

    /// Returns the repository reference of this package, e.g. master, 1.0.0 or a commit hash for git
    fn get_source_reference(&self) -> Option<BString>;

    /// Returns the source mirrors of this package
    fn get_source_mirrors(&self) -> Option<Vec<Mirror>>;

    fn set_source_mirrors(&mut self, mirrors: Option<Vec<Mirror>>);

    /// Returns the type of the distribution archive of this version, e.g. zip, tarball
    fn get_dist_type(&self) -> Option<BString>;

    /// Returns the url of the distribution archive of this version
    fn get_dist_url(&self) -> Option<BString>;

    /// Returns the urls of the distribution archive of this version, including mirrors
    fn get_dist_urls(&self) -> Vec<BString>;

    /// Returns the reference of the distribution archive of this version, e.g. master, 1.0.0 or a commit hash for git
    fn get_dist_reference(&self) -> Option<BString>;

    /// Returns the sha1 checksum for the distribution archive of this version
    ///
    /// Can be an empty string which should be treated as null
    fn get_dist_sha1_checksum(&self) -> Option<BString>;

    /// Returns the dist mirrors of this package
    fn get_dist_mirrors(&self) -> Option<Vec<Mirror>>;

    fn set_dist_mirrors(&mut self, mirrors: Option<Vec<Mirror>>);

    /// Returns the version of this package
    fn get_version(&self) -> &BStr;

    /// Returns the pretty (i.e. non-normalized) version string of this package
    fn get_pretty_version(&self) -> &BStr;

    /// Returns the pretty version string plus a git or hg commit hash of this package
    fn get_full_pretty_version(&self, truncate: bool, display_mode: DisplayMode) -> BString;

    /// Returns the release date of the package
    fn get_release_date(&self) -> Option<DateTime<Utc>>;

    /// Returns the stability of this package: one of (dev, alpha, beta, RC, stable)
    fn get_stability(&self) -> &BStr;

    /// Returns a set of links to packages which need to be installed before
    /// this package can be installed
    fn get_requires(&self) -> std::rc::Rc<IndexMap<BString, Link>>;

    /// Returns a set of links to packages which must not be installed at the
    /// same time as this package
    fn get_conflicts(&self) -> std::rc::Rc<IndexMap<BString, Link>>;

    /// Returns a set of links to virtual packages that are provided through
    /// this package
    fn get_provides(&self) -> std::rc::Rc<IndexMap<BString, Link>>;

    /// Returns a set of links to packages which can alternatively be
    /// satisfied by installing this package
    fn get_replaces(&self) -> std::rc::Rc<IndexMap<BString, Link>>;

    /// Returns a set of links to packages which are required to develop
    /// this package. These are installed if in dev mode.
    fn get_dev_requires(&self) -> std::rc::Rc<IndexMap<BString, Link>>;

    /// Returns a set of package names and reasons why they are useful in
    /// combination with this package.
    fn get_suggests(&self) -> IndexMap<BString, BString>;

    /// Stands in for PHP's `$package->{'get'.ucfirst($linkType)}()`, so `link_type` is one of the
    /// `Link::TYPE_*` values.
    fn get_links_for_type(
        &self,
        link_type: &str,
    ) -> std::rc::Rc<IndexMap<BString, crate::package::Link>> {
        match link_type {
            crate::package::Link::TYPE_REQUIRE => self.get_requires(),
            crate::package::Link::TYPE_DEV_REQUIRE => self.get_dev_requires(),
            crate::package::Link::TYPE_CONFLICT => self.get_conflicts(),
            crate::package::Link::TYPE_PROVIDE => self.get_provides(),
            crate::package::Link::TYPE_REPLACE => self.get_replaces(),
            _ => std::rc::Rc::new(IndexMap::new()),
        }
    }

    /// Returns an associative array of autoloading rules
    ///
    /// {"<type>": {"<namespace": "<directory>"}}
    ///
    /// Type is either "psr-4", "psr-0", "classmap" or "files". Namespaces are mapped to
    /// directories for autoloading using the type specified.
    fn get_autoload(&self) -> IndexMap<BString, PhpMixed>;

    /// Returns an associative array of dev autoloading rules
    ///
    /// {"<type>": {"<namespace": "<directory>"}}
    ///
    /// Type is either "psr-4", "psr-0", "classmap" or "files". Namespaces are mapped to
    /// directories for autoloading using the type specified.
    fn get_dev_autoload(&self) -> IndexMap<BString, PhpMixed>;

    /// Returns a list of directories which should get added to PHP's
    /// include path.
    fn get_include_paths(&self) -> Vec<BString>;

    /// Returns the settings for php extension packages
    fn get_php_ext(&self) -> Option<IndexMap<BString, PhpMixed>>;

    /// Stores a reference to the repository that owns the package
    fn set_repository(&mut self, repository: RepositoryInterfaceHandle) -> anyhow::Result<()>;

    /// Returns a reference to the repository that owns the package
    fn get_repository(&self) -> Option<RepositoryInterfaceHandle>;

    /// Returns the package binaries
    fn get_binaries(&self) -> Vec<BString>;

    /// Returns package unique name, constructed from name and version.
    fn get_unique_name(&self) -> BString;

    /// Returns the package notification url
    fn get_notification_url(&self) -> Option<BString>;

    /// PHP's `__toString()`.
    fn to_php_string(&self) -> BString {
        self.get_unique_name()
    }

    /// Converts the package into a pretty readable string
    fn get_pretty_string(&self) -> BString;

    fn is_default_branch(&self) -> bool;

    /// Returns a list of options to download package dist files
    fn get_transport_options(&self) -> IndexMap<BString, PhpMixed>;

    /// Configures the list of options to download package dist files
    fn set_transport_options(&mut self, options: IndexMap<BString, PhpMixed>);

    fn set_source_reference(&mut self, reference: Option<BString>);

    fn set_source_url(&mut self, url: Option<BString>);

    fn set_dist_url(&mut self, url: Option<BString>);

    fn set_dist_type(&mut self, r#type: Option<BString>);

    fn set_dist_reference(&mut self, reference: Option<BString>);

    /// Set dist and source references and update dist URL for ones that contain a reference
    fn set_source_dist_references(&mut self, reference: BString);

    fn as_alias_package(&self) -> Option<&crate::package::AliasPackage> {
        None
    }

    fn as_complete_package_interface(
        &self,
    ) -> Option<&dyn crate::package::CompletePackageInterface> {
        None
    }

    fn as_complete_package(&self) -> Option<&dyn crate::package::CompletePackageInterface> {
        None
    }

    fn as_root_package_interface(&self) -> Option<&dyn crate::package::RootPackageInterface> {
        None
    }
}
