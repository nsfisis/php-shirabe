//! Shirabe-specific integration tests for the alias package proxy stubs. Upstream Composer has
//! no test for this (its plugins run in-process), so the fixture `fixtures/alias-v1` is
//! Shirabe-owned.

use crate::async_runtime::run;
use crate::php_worker::lock_php_worker;
use crate::plugin_installer_test::{new_installer, set_up};
use bstr::{B, BString};
use shirabe::installer::InstallerInterface;
use shirabe::package::loader::{ArrayLoader, JsonLoader, JsonLoaderInput};
use shirabe::package::{
    CompleteAliasPackageHandle, CompletePackageHandle, PackageInterfaceHandle, RootPackageHandle,
};
use shirabe_php_shim::{PhpMixed, path_to_bytes};

fn alias_fixture_package() -> PackageInterfaceHandle {
    let loader = JsonLoader::new(Box::new(ArrayLoader::new(None, false)));
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/plugin/fixtures/alias-v1/composer.json");
    loader
        .load(JsonLoaderInput::String(
            path_to_bytes(&path.canonicalize().unwrap()).into(),
        ))
        .unwrap()
}

#[test]
fn test_alias_packages_cross_the_plugin_boundary() {
    let _worker = lock_php_worker();
    let set_up = set_up();

    let aliased = CompletePackageHandle::new(
        BString::from("vendor/aliased"),
        BString::from("1.0.0.0"),
        BString::from("1.0.0"),
    );
    aliased.set_description(BString::from("an aliased package"));
    let alias = CompleteAliasPackageHandle::new(
        aliased.clone(),
        BString::from("2.0.0.0"),
        BString::from("2.0.0"),
    )
    .unwrap();
    set_up
        .repository
        .borrow_mut()
        .add_package(alias.clone().into())
        .unwrap();

    let real_root = RootPackageHandle::new(
        BString::from("dummy/root"),
        BString::from("1.0.0.0"),
        BString::from("1.0.0"),
    );
    let root_alias = shirabe::package::RootAliasPackageHandle::new(
        real_root.clone(),
        BString::from("1.1.0.0"),
        BString::from("1.1.0"),
    )
    .unwrap();
    set_up.composer.borrow_mut().set_package(root_alias.into());

    let installer = new_installer(&set_up);
    set_up.pm.borrow_mut().load_installed_plugins().unwrap();
    run(installer.install(&set_up.repository, alias_fixture_package())).unwrap();

    assert_eq!(
        "alias: Composer\\Package\\CompleteAliasPackage vendor/aliased 2.0.0 of Composer\\Package\\CompletePackage 1.0.0\n\
         alias description: an aliased package\n\
         alias self-version: no\n\
         alias root-flag: no\n\
         alias root-flag: yes\n\
         root: Composer\\Package\\RootAliasPackage dummy/root 1.1.0 of Composer\\Package\\RootPackage 1.0.0\n\
         root aliasOf identity: same\n\
         root minimum stability: stable\n\
         built: Composer\\Package\\CompleteAliasPackage vendor/aliased 9.9.9 of Composer\\Package\\CompletePackage 1.0.0\n",
        set_up.io.borrow().get_output()
    );

    // The flag the plugin set landed on the Rust-side entity, not on a child-side copy.
    let alias: shirabe::package::AliasPackageHandle = alias.into();
    assert!(alias.is_root_package_alias());

    // `RootAliasPackage`'s setters write through to the package it aliases.
    assert_eq!("dev", real_root.get_minimum_stability());
    assert_eq!(
        Some(&PhpMixed::string(BString::from("alias-v1"))),
        real_root.get_extra().get(B("seen-by"))
    );
}
