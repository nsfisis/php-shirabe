//! ref: composer/tests/Composer/Test/Package/Dumper/ArrayDumperTest.php

use bstr::{B, BString};
use indexmap::IndexMap;
use shirabe::package::ArrayDumper;
use shirabe::package::handle::{CompletePackageHandle, RootPackageHandle};
use shirabe_php_shim::{PhpMixed, legacy_lossy};
use shirabe_semver::VersionParser;

fn set_up() -> ArrayDumper {
    ArrayDumper::new()
}

fn complete_package() -> CompletePackageHandle {
    let norm = legacy_lossy(&VersionParser.normalize("1.0.0", None).unwrap()).into_owned();
    CompletePackageHandle::new("dummy/pkg".to_string(), norm, "1.0.0".to_string())
}

fn root_package() -> RootPackageHandle {
    let norm = legacy_lossy(&VersionParser.normalize("1.0.0", None).unwrap()).into_owned();
    RootPackageHandle::new("dummy/pkg".to_string(), norm, "1.0.0".to_string())
}

#[test]
fn test_required_information() {
    let dumper = set_up();
    let config = dumper.dump(complete_package().into());

    let mut expected: IndexMap<BString, PhpMixed> = IndexMap::new();
    expected.insert("name".into(), PhpMixed::string("dummy/pkg".to_string()));
    expected.insert("version".into(), PhpMixed::string("1.0.0".to_string()));
    expected.insert(
        "version_normalized".into(),
        PhpMixed::string("1.0.0.0".to_string()),
    );
    expected.insert("type".into(), PhpMixed::string("library".to_string()));

    assert_eq!(expected, config);
}

#[test]
fn test_root_package() {
    let dumper = set_up();
    let package = root_package();
    package.set_minimum_stability("dev".to_string());

    let config = dumper.dump(package.into());

    assert_eq!(
        Some(&PhpMixed::string("dev".to_string())),
        config.get(B("minimum-stability"))
    );
}

#[test]
fn test_dump_abandoned() {
    let dumper = set_up();
    let package = complete_package();
    package.set_abandoned(PhpMixed::Bool(true));

    let config = dumper.dump(package.into());

    assert_eq!(Some(&PhpMixed::Bool(true)), config.get(B("abandoned")));
}

#[test]
fn test_dump_abandoned_replacement() {
    let dumper = set_up();
    let package = complete_package();
    package.set_abandoned(PhpMixed::string("foo/bar".to_string()));

    let config = dumper.dump(package.into());

    assert_eq!(
        Some(&PhpMixed::string("foo/bar".to_string())),
        config.get(B("abandoned"))
    );
}

// PHP drives 26 heterogeneous setters dynamically (set{ucfirst(method)}) across strings,
// arrays, a DateTime and Link maps, then checks the corresponding dumped key. Three of the
// data sets cannot be expressed faithfully because the ported production types were narrowed
// from PHP's loose `array`: the `authors` set passes plain strings (Rust set_authors wants
// Vec<IndexMap<String,String>>), `scripts` passes a bare string value (set_scripts wants
// IndexMap<String,Vec<String>>), and `funding` passes a single map (set_funding wants
// Vec<IndexMap<String,PhpMixed>>); the dumper additionally re-wraps each of these. Porting the
// remaining 23 would drop those three cases, so testKeys stays unported (all-or-nothing).
#[test]
#[ignore = "authors/scripts/funding data sets pass loosely-typed PHP arrays the narrowed Rust set_authors/set_scripts/set_funding types cannot represent, and the dumper re-wraps them; faithful all-or-nothing port blocked without loosening those production types"]
fn test_keys() {
    // TODO(type-model): authors/scripts/funding data sets pass loosely-typed PHP arrays the
    // narrowed Rust set_authors/set_scripts/set_funding types cannot represent, and the
    // dumper re-wraps them; faithful all-or-nothing port blocked without loosening those
    // production types.
    todo!()
}
