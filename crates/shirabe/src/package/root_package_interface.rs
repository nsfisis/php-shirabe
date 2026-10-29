//! ref: composer/src/Composer/Package/RootPackageInterface.php

use crate::package::{CompletePackageInterface, Link, PackageInterface};
use bstr::BString;
use indexmap::IndexMap;
use shirabe_php_shim::PhpMixed;

pub trait RootPackageInterface: CompletePackageInterface {
    fn get_aliases(&self) -> Vec<IndexMap<BString, BString>>;

    fn get_minimum_stability(&self) -> BString;

    fn get_stability_flags(&self) -> IndexMap<BString, i64>;

    fn get_references(&self) -> IndexMap<BString, BString>;

    fn get_prefer_stable(&self) -> bool;

    fn get_config(&self) -> IndexMap<BString, PhpMixed>;

    fn set_requires(&mut self, requires: IndexMap<BString, Link>);

    fn set_dev_requires(&mut self, dev_requires: IndexMap<BString, Link>);

    fn set_conflicts(&mut self, conflicts: IndexMap<BString, Link>);

    fn set_provides(&mut self, provides: IndexMap<BString, Link>);

    fn set_replaces(&mut self, replaces: IndexMap<BString, Link>);

    fn set_autoload(&mut self, autoload: IndexMap<BString, PhpMixed>);

    fn set_dev_autoload(&mut self, dev_autoload: IndexMap<BString, PhpMixed>);

    fn set_stability_flags(&mut self, stability_flags: IndexMap<BString, i64>);

    fn set_minimum_stability(&mut self, minimum_stability: BString);

    fn set_prefer_stable(&mut self, prefer_stable: bool);

    fn set_config(&mut self, config: IndexMap<BString, PhpMixed>);

    fn set_references(&mut self, references: IndexMap<BString, BString>);

    fn set_aliases(&mut self, aliases: Vec<IndexMap<BString, BString>>);

    fn set_suggests(&mut self, suggests: IndexMap<BString, BString>);

    fn set_extra(&mut self, extra: IndexMap<BString, PhpMixed>);

    fn as_package_interface(&self) -> &dyn PackageInterface;
}
