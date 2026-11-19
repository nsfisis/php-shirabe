//! The methods a `Composer\Package\PackageInterface` entity answers over RPC, one forwarding impl
//! per package class.

use crate::package::{
    __php_binding_alias_package, __php_binding_complete_package,
    __php_binding_complete_package_interface, __php_binding_package,
    __php_binding_package_interface, __php_binding_root_package_interface, AliasPackage,
    AnyPackage, CompleteAliasPackage, CompletePackageInterface, Package, PackageInterface,
    PackageInterfaceHandle, RootAliasPackage, RootPackageInterface,
};
use crate::php_worker::rpc::entity::{Call, PropertyAccess, entity_value};
use shirabe_php_binding_macros::php_entity;
use shirabe_php_rpc::{FromRpcValue, PhpThrow, RpcValue, ToRpcValue};
use shirabe_php_shim::bformat;

#[expect(clippy::disallowed_types, reason = "the message `expect` takes")]
const REAL: &str = "only a real package crosses as Composer\\Package\\Package or a subclass";
#[expect(clippy::disallowed_types, reason = "the message `expect` takes")]
const COMPLETE: &str = "only a complete package crosses as a class declaring its methods";
#[expect(clippy::disallowed_types, reason = "the message `expect` takes")]
const ROOT: &str = "only a root package crosses as a class declaring its methods";
#[expect(clippy::disallowed_types, reason = "the message `expect` takes")]
const ALIAS: &str =
    "only an alias package crosses as Composer\\Package\\AliasPackage or a subclass";

php_entity! {
    handle: PackageInterfaceHandle,
    type_error: "a package",
    identity: |package| std::rc::Rc::as_ptr(package.as_rc()) as *const (),
    class_of: |package| Ok(match &*package.as_rc().borrow() {
        AnyPackage::Package(_) => br"Composer\Package\Package",
        AnyPackage::CompletePackage(_) => br"Composer\Package\CompletePackage",
        AnyPackage::RootPackage(_) => br"Composer\Package\RootPackage",
        AnyPackage::AliasPackage(_) => br"Composer\Package\AliasPackage",
        AnyPackage::CompleteAliasPackage(_) => br"Composer\Package\CompleteAliasPackage",
        AnyPackage::RootAliasPackage(_) => br"Composer\Package\RootAliasPackage",
    }),
    classes: {
        r"Composer\Package\BasePackage": { ancestor: true },
        r"Composer\Package\Package": {
            constructor: {
                adapters: __php_binding_package,
                into: |package| constructed(AnyPackage::Package(package)),
            },
        },
        r"Composer\Package\CompletePackage": {
            constructor: {
                adapters: __php_binding_complete_package,
                into: |package| constructed(AnyPackage::CompletePackage(package)),
            },
        },
        r"Composer\Package\RootPackage": { constructor: unimplemented },
        r"Composer\Package\AliasPackage": {
            constructor: { handwritten: construct_alias_package },
        },
        r"Composer\Package\CompleteAliasPackage": {
            constructor: { handwritten: construct_complete_alias_package },
        },
        r"Composer\Package\RootAliasPackage": {
            constructor: { handwritten: construct_root_alias_package },
        },
    },
    methods: {
        r"Composer\Package\PackageInterface": {
            handwritten: { module: base_package, members: [getNames, __toString] },
            instance: {
                adapters: __php_binding_package_interface,
                target: dyn PackageInterface,
                borrow: |package| std::cell::Ref::map(
                    package.as_rc().borrow(),
                    AnyPackage::as_package_interface,
                ),
                borrow_mut: |package| std::cell::RefMut::map(
                    package.as_rc().borrow_mut(),
                    AnyPackage::as_package_interface_mut,
                ),
            },
        },
        r"Composer\Package\CompletePackageInterface": {
            handwritten: {
                module: complete_package,
                members: [setDescription, setHomepage, setArchiveName],
            },
            instance: {
                adapters: __php_binding_complete_package_interface,
                target: dyn CompletePackageInterface,
                borrow: |package| std::cell::Ref::map(package.as_rc().borrow(), |package| {
                    package.as_complete_package_interface().expect(COMPLETE)
                }),
                borrow_mut: |package| std::cell::RefMut::map(package.as_rc().borrow_mut(), |package| {
                    package.as_complete_package_interface_mut().expect(COMPLETE)
                }),
            },
        },
        r"Composer\Package\RootPackageInterface": {
            instance: {
                adapters: __php_binding_root_package_interface,
                target: dyn RootPackageInterface,
                borrow: |package| std::cell::Ref::map(package.as_rc().borrow(), |package| {
                    package.as_root_package_interface().expect(ROOT)
                }),
                borrow_mut: |package| std::cell::RefMut::map(package.as_rc().borrow_mut(), |package| {
                    package.as_root_package_interface_mut().expect(ROOT)
                }),
            },
        },
        r"Composer\Package\BasePackage": {
            handwritten: { module: base_package },
        },
        r"Composer\Package\Package": {
            instance: {
                adapters: __php_binding_package,
                target: Package,
                borrow_mut: |package| std::cell::RefMut::map(package.as_rc().borrow_mut(), |package| {
                    package.as_package_mut().expect(REAL)
                }),
            },
        },
        r"Composer\Package\AliasPackage": {
            instance: {
                adapters: __php_binding_alias_package,
                target: AliasPackage,
                borrow: |package| std::cell::Ref::map(package.as_rc().borrow(), |package| {
                    package.as_alias_package().expect(ALIAS)
                }),
                borrow_mut: |package| std::cell::RefMut::map(package.as_rc().borrow_mut(), |package| {
                    package.as_alias_package_mut().expect(ALIAS)
                }),
            },
        },
    },
    handlers: {
        clone: clone_package,
        properties: { id: package_id },
    },
}

// `AnyPackage::dup` carries `BasePackage::__clone` (repository reset, id = -1) and the
// `RootAliasPackage::__clone` override.
fn clone_package(package: &PackageInterfaceHandle) -> Result<PackageInterfaceHandle, PhpThrow> {
    let cloned = package.as_rc().borrow().dup();
    Ok(constructed(cloned))
}

// `BasePackage::$id` is the one instance property the packages expose so far.
fn package_id(
    package: &PackageInterfaceHandle,
    access: PropertyAccess<'_>,
) -> Result<RpcValue, PhpThrow> {
    match access {
        PropertyAccess::Get { .. } => Ok(package
            .as_rc()
            .borrow()
            .as_package_interface()
            .get_id()
            .to_rpc_value()),
        PropertyAccess::Isset { .. } => Ok(RpcValue::Bool(true)),
        PropertyAccess::Set { value, .. } => {
            let id = i64::from_rpc_value(value)?;
            package
                .as_rc()
                .borrow_mut()
                .as_package_interface_mut()
                .set_id(id);
            Ok(RpcValue::Null)
        }
        PropertyAccess::Unset { .. } => Err(PhpThrow::runtime(
            "the package property `id` cannot be unset over RPC",
        )),
    }
}

/// Hands a package to the child.
pub(crate) fn package_value(
    package: &std::rc::Rc<std::cell::RefCell<AnyPackage>>,
) -> Result<RpcValue, PhpThrow> {
    entity_value(&PackageInterfaceHandle::from_rc_unchecked(package.clone()))
}

impl ToRpcValue for PackageInterfaceHandle {
    fn to_rpc_value(self) -> RpcValue {
        entity_value(&self).expect("every package variant crosses as its own class")
    }
}

fn constructed(package: AnyPackage) -> PackageInterfaceHandle {
    PackageInterfaceHandle::from_rc_unchecked(std::rc::Rc::new(std::cell::RefCell::new(package)))
}

/// The members of `BasePackage` the Rust side has no counterpart of, computed from the package,
/// and the ones whose stub signature is wider than `PackageInterface`'s.
mod base_package {
    use crate::package::{PackageInterfaceHandle, STABILITY_STABLE, stability_priority};
    use crate::php_worker::rpc::entity::Call;
    use crate::repository::PlatformRepository;
    use shirabe_php_rpc::{PhpThrow, RpcValue, ToRpcValue};
    use shirabe_php_shim::{PhpMixed, PhpTruthy as _};

    // `BasePackage::getNames()` takes `$provides` untyped, so any value arrives and is used for
    // its truthiness.
    pub(super) fn get_names(
        package: &PackageInterfaceHandle,
        call: &mut Call<'_>,
    ) -> Result<RpcValue, PhpThrow> {
        let provides = call.arg::<PhpMixed>(0)?.is_truthy();
        Ok(package
            .as_rc()
            .borrow()
            .as_package_interface()
            .get_names(provides)
            .to_rpc_value())
    }

    pub(super) fn __to_string(
        package: &PackageInterfaceHandle,
        _: &mut Call<'_>,
    ) -> Result<RpcValue, PhpThrow> {
        Ok(package
            .as_rc()
            .borrow()
            .as_package_interface()
            .get_unique_name()
            .to_rpc_value())
    }

    // `BasePackage`'s concrete methods are not forwarded by `PackageInterface`, so both are
    // computed from the interface here, as `VersionSelector` already does for the second.
    pub(super) fn is_platform(
        package: &PackageInterfaceHandle,
        _: &mut Call<'_>,
    ) -> Result<RpcValue, PhpThrow> {
        Ok(package
            .as_rc()
            .borrow()
            .as_package_interface()
            .get_repository()
            .is_some_and(|repository| repository.is::<PlatformRepository>())
            .to_rpc_value())
    }

    pub(super) fn equals(
        package: &PackageInterfaceHandle,
        call: &mut Call<'_>,
    ) -> Result<RpcValue, PhpThrow> {
        let other = call.arg::<PackageInterfaceHandle>(0)?;
        Ok(package.equals(&other).to_rpc_value())
    }

    pub(super) fn get_stability_priority(
        package: &PackageInterfaceHandle,
        _: &mut Call<'_>,
    ) -> Result<RpcValue, PhpThrow> {
        Ok(stability_priority(
            package
                .as_rc()
                .borrow()
                .as_package_interface()
                .get_stability(),
        )
        .unwrap_or(STABILITY_STABLE)
        .to_rpc_value())
    }
}

/// The setters of `CompletePackageInterface` whose implementations take a wider type than the
/// interface declares.
mod complete_package {
    use crate::package::PackageInterfaceHandle;
    use crate::php_worker::rpc::entity::Call;
    use bstr::BString;
    use shirabe_php_rpc::{PhpThrow, RpcValue};

    #[expect(clippy::disallowed_types, reason = "the message `expect` takes")]
    const COMPLETE: &str = "only a complete package crosses as a class declaring its methods";

    // TODO(type-model): `CompletePackage` and `CompleteAliasPackage` take `?string` here, but the
    // Rust setters take the `string` `CompletePackageInterface` declares, so null is rejected.
    pub(super) fn set_description(
        package: &PackageInterfaceHandle,
        call: &mut Call<'_>,
    ) -> Result<RpcValue, PhpThrow> {
        let value = call.arg::<BString>(0)?;
        package
            .as_rc()
            .borrow_mut()
            .as_complete_package_interface_mut()
            .expect(COMPLETE)
            .set_description(value);
        Ok(RpcValue::Null)
    }

    // TODO(type-model): `CompletePackage` and `CompleteAliasPackage` take `?string` here, but the
    // Rust setters take the `string` `CompletePackageInterface` declares, so null is rejected.
    pub(super) fn set_homepage(
        package: &PackageInterfaceHandle,
        call: &mut Call<'_>,
    ) -> Result<RpcValue, PhpThrow> {
        let value = call.arg::<BString>(0)?;
        package
            .as_rc()
            .borrow_mut()
            .as_complete_package_interface_mut()
            .expect(COMPLETE)
            .set_homepage(value);
        Ok(RpcValue::Null)
    }

    // TODO(type-model): `CompletePackage` and `CompleteAliasPackage` take `?string` here, but the
    // Rust setters take the `string` `CompletePackageInterface` declares, so null is rejected.
    pub(super) fn set_archive_name(
        package: &PackageInterfaceHandle,
        call: &mut Call<'_>,
    ) -> Result<RpcValue, PhpThrow> {
        let value = call.arg::<BString>(0)?;
        package
            .as_rc()
            .borrow_mut()
            .as_complete_package_interface_mut()
            .expect(COMPLETE)
            .set_archive_name(value);
        Ok(RpcValue::Null)
    }
}

// The alias target has to be a package that already lives on the Rust side; an alias of an
// alias has no Rust representation, so its narrowing is an explicit error too.
fn construct_alias_package(call: &mut Call<'_>) -> Result<PackageInterfaceHandle, PhpThrow> {
    let alias_of = call
        .arg::<PackageInterfaceHandle>(0)?
        .as_package()
        .ok_or_else(|| {
            PhpThrow::runtime(bformat!("{} expects a real Package to alias", call.class()))
        })?;
    Ok(constructed(AnyPackage::AliasPackage(
        AliasPackage::new(alias_of, call.arg(1)?, call.arg(2)?).map_err(call.failed())?,
    )))
}

fn construct_complete_alias_package(
    call: &mut Call<'_>,
) -> Result<PackageInterfaceHandle, PhpThrow> {
    let alias_of = call
        .arg::<PackageInterfaceHandle>(0)?
        .as_complete_package()
        .ok_or_else(|| {
            PhpThrow::runtime(bformat!(
                "{} expects a real CompletePackage to alias",
                call.class()
            ))
        })?;
    Ok(constructed(AnyPackage::CompleteAliasPackage(
        CompleteAliasPackage::new(alias_of, call.arg(1)?, call.arg(2)?).map_err(call.failed())?,
    )))
}

fn construct_root_alias_package(call: &mut Call<'_>) -> Result<PackageInterfaceHandle, PhpThrow> {
    let alias_of = call
        .arg::<PackageInterfaceHandle>(0)?
        .as_root_package()
        .ok_or_else(|| {
            PhpThrow::runtime(bformat!(
                "{} expects a real RootPackage to alias",
                call.class()
            ))
        })?;
    Ok(constructed(AnyPackage::RootAliasPackage(
        RootAliasPackage::new(alias_of, call.arg(1)?, call.arg(2)?).map_err(call.failed())?,
    )))
}
