//! ref: composer/src/Composer/Package/RootPackageInterface.php

use indexmap::IndexMap;
use shirabe_php_shim::PhpMixed;

use crate::package::complete_package_interface::CompletePackageInterface;
use crate::package::link::Link;

pub trait RootPackageInterface: CompletePackageInterface {
    fn get_aliases(&self) -> &[IndexMap<String, String>];

    fn get_minimum_stability(&self) -> &str;

    fn get_stability_flags(&self) -> &IndexMap<String, i64>;

    fn get_references(&self) -> &IndexMap<String, String>;

    fn get_prefer_stable(&self) -> bool;

    fn get_config(&self) -> &IndexMap<String, PhpMixed>;

    fn set_requires(&mut self, requires: Vec<Link>);

    fn set_dev_requires(&mut self, dev_requires: Vec<Link>);

    fn set_conflicts(&mut self, conflicts: Vec<Link>);

    fn set_provides(&mut self, provides: Vec<Link>);

    fn set_replaces(&mut self, replaces: Vec<Link>);

    fn set_autoload(&mut self, autoload: IndexMap<String, PhpMixed>);

    fn set_dev_autoload(&mut self, dev_autoload: IndexMap<String, PhpMixed>);

    fn set_stability_flags(&mut self, stability_flags: IndexMap<String, i64>);

    fn set_minimum_stability(&mut self, minimum_stability: String);

    fn set_prefer_stable(&mut self, prefer_stable: bool);

    fn set_config(&mut self, config: IndexMap<String, PhpMixed>);

    fn set_references(&mut self, references: IndexMap<String, String>);

    fn set_aliases(&mut self, aliases: Vec<IndexMap<String, String>>);

    fn set_suggests(&mut self, suggests: IndexMap<String, String>);

    fn set_extra(&mut self, extra: IndexMap<String, PhpMixed>);
}
