//! The methods a `Composer\Package\PackageInterface` entity answers over RPC, one forwarding impl
//! per package class.

use crate::package::{
    AliasPackage, AnyPackage, CompleteAliasPackage, CompletePackage, DisplayMode, Link, Mirror,
    Package, PackageInterfaceHandle, RootAliasPackage, STABILITY_STABLE, stability_priority,
};
use crate::php_worker::rpc::entity::{Call, Crossing, Entity, PropertyAccess, entity_value};
use crate::php_worker::rpc::forwarding;
use crate::repository::{PlatformRepository, RepositoryInterfaceHandle};
use bstr::BString;
use indexmap::IndexMap;
use shirabe_php_rpc::{FromWire, PhpThrow, PluginValue, ToWire};
use shirabe_php_shim::{PhpMixed, bformat};

impl Entity for PackageInterfaceHandle {
    const EXPECTED: &'static [u8] = b"a package";

    fn address(&self) -> *const () {
        std::rc::Rc::as_ptr(self.as_rc()) as *const ()
    }

    fn class(&self) -> Result<&'static Crossing<Self>, PhpThrow> {
        Ok(match &*self.as_rc().borrow() {
            AnyPackage::Package(_) => &forwarding::package::package::CROSSING,
            AnyPackage::CompletePackage(_) => &forwarding::package::complete_package::CROSSING,
            AnyPackage::RootPackage(_) => &forwarding::package::root_package::CROSSING,
            AnyPackage::AliasPackage(_) => &forwarding::package::alias_package::CROSSING,
            AnyPackage::CompleteAliasPackage(_) => {
                &forwarding::package::complete_alias_package::CROSSING
            }
            AnyPackage::RootAliasPackage(_) => &forwarding::package::root_alias_package::CROSSING,
        })
    }

    // `AnyPackage::dup` carries `BasePackage::__clone` (repository reset, id = -1) and the
    // `RootAliasPackage::__clone` override.
    fn clone_entity(&self) -> Result<Self, PhpThrow> {
        let cloned = self.as_rc().borrow().dup();
        Ok(constructed(cloned))
    }

    // `BasePackage::$id` is the one instance property the packages expose so far.
    fn property(&self, access: PropertyAccess<'_>) -> Result<PluginValue, PhpThrow> {
        if access.name() != "id" {
            // TODO(plugin): the remaining package properties are widened on demand.
            return Err(PhpThrow::runtime(bformat!(
                "the property `{}` is not available over RPC yet",
                access.name()
            )));
        }
        match access {
            PropertyAccess::Get { .. } => Ok(self
                .as_rc()
                .borrow()
                .as_package_interface()
                .get_id()
                .to_wire()),
            PropertyAccess::Isset { .. } => Ok(PluginValue::Bool(true)),
            PropertyAccess::Set { value, .. } => {
                let id = i64::from_wire(value)?;
                self.as_rc()
                    .borrow_mut()
                    .as_package_interface_mut()
                    .set_id(id);
                Ok(PluginValue::Null)
            }
            PropertyAccess::Unset { .. } => Err(PhpThrow::runtime(
                "the package property `id` cannot be unset over RPC",
            )),
        }
    }
}

/// Hands a package to the child.
pub(crate) fn package_value(
    package: &std::rc::Rc<std::cell::RefCell<AnyPackage>>,
) -> Result<PluginValue, PhpThrow> {
    entity_value(&PackageInterfaceHandle::from_rc_unchecked(package.clone()))
}

impl ToWire for PackageInterfaceHandle {
    fn to_wire(self) -> PluginValue {
        entity_value(&self).expect("every package variant crosses as its own class")
    }
}

fn constructed(package: AnyPackage) -> PackageInterfaceHandle {
    PackageInterfaceHandle::from_rc_unchecked(std::rc::Rc::new(std::cell::RefCell::new(package)))
}

#[expect(clippy::disallowed_types, reason = "the message `expect` takes")]
const REAL: &str = "only a real package crosses as Composer\\Package\\Package or a subclass";
#[expect(clippy::disallowed_types, reason = "the message `expect` takes")]
const COMPLETE: &str = "only a complete package crosses as a class declaring its methods";
#[expect(clippy::disallowed_types, reason = "the message `expect` takes")]
const ROOT: &str = "only a root package crosses as a class declaring its methods";
#[expect(clippy::disallowed_types, reason = "the message `expect` takes")]
const ALIAS: &str =
    "only an alias package crosses as Composer\\Package\\AliasPackage or a subclass";

fn links(call: &Call<'_>) -> Result<IndexMap<BString, Link>, PhpThrow> {
    call.arg_or(0, IndexMap::new())
}

// TODO(plugin): the remaining PackageInterface surface is widened on demand.
impl forwarding::package::base_package::Forwarding for forwarding::package::base_package::PhpClass {
    type Handle = PackageInterfaceHandle;

    fn get_name(
        package: &PackageInterfaceHandle,
        _: &mut Call<'_>,
    ) -> Result<PluginValue, PhpThrow> {
        Ok(package
            .as_rc()
            .borrow()
            .as_package_interface()
            .get_name()
            .to_wire())
    }

    fn get_pretty_name(
        package: &PackageInterfaceHandle,
        _: &mut Call<'_>,
    ) -> Result<PluginValue, PhpThrow> {
        Ok(package
            .as_rc()
            .borrow()
            .as_package_interface()
            .get_pretty_name()
            .to_wire())
    }

    fn get_names(
        package: &PackageInterfaceHandle,
        call: &mut Call<'_>,
    ) -> Result<PluginValue, PhpThrow> {
        Ok(package
            .as_rc()
            .borrow()
            .as_package_interface()
            .get_names(call.arg_or(0, true)?)
            .to_wire())
    }

    fn set_id(
        package: &PackageInterfaceHandle,
        call: &mut Call<'_>,
    ) -> Result<PluginValue, PhpThrow> {
        let value = call.arg::<i64>(0)?;
        package
            .as_rc()
            .borrow_mut()
            .as_package_interface_mut()
            .set_id(value);
        Ok(PluginValue::Null)
    }

    fn get_id(package: &PackageInterfaceHandle, _: &mut Call<'_>) -> Result<PluginValue, PhpThrow> {
        Ok(package
            .as_rc()
            .borrow()
            .as_package_interface()
            .get_id()
            .to_wire())
    }

    fn is_dev(package: &PackageInterfaceHandle, _: &mut Call<'_>) -> Result<PluginValue, PhpThrow> {
        Ok(package
            .as_rc()
            .borrow()
            .as_package_interface()
            .is_dev()
            .to_wire())
    }

    fn get_type(
        package: &PackageInterfaceHandle,
        _: &mut Call<'_>,
    ) -> Result<PluginValue, PhpThrow> {
        Ok(package
            .as_rc()
            .borrow()
            .as_package_interface()
            .get_type()
            .to_wire())
    }

    fn get_target_dir(
        package: &PackageInterfaceHandle,
        _: &mut Call<'_>,
    ) -> Result<PluginValue, PhpThrow> {
        Ok(package
            .as_rc()
            .borrow()
            .as_package_interface()
            .get_target_dir()
            .to_wire())
    }

    fn get_extra(
        package: &PackageInterfaceHandle,
        _: &mut Call<'_>,
    ) -> Result<PluginValue, PhpThrow> {
        Ok(package
            .as_rc()
            .borrow()
            .as_package_interface()
            .get_extra()
            .to_wire())
    }

    fn set_installation_source(
        package: &PackageInterfaceHandle,
        call: &mut Call<'_>,
    ) -> Result<PluginValue, PhpThrow> {
        let value = call.arg::<Option<BString>>(0)?;
        package
            .as_rc()
            .borrow_mut()
            .as_package_interface_mut()
            .set_installation_source(value);
        Ok(PluginValue::Null)
    }

    fn get_installation_source(
        package: &PackageInterfaceHandle,
        _: &mut Call<'_>,
    ) -> Result<PluginValue, PhpThrow> {
        Ok(package
            .as_rc()
            .borrow()
            .as_package_interface()
            .get_installation_source()
            .to_wire())
    }

    fn get_source_type(
        package: &PackageInterfaceHandle,
        _: &mut Call<'_>,
    ) -> Result<PluginValue, PhpThrow> {
        Ok(package
            .as_rc()
            .borrow()
            .as_package_interface()
            .get_source_type()
            .to_wire())
    }

    fn get_source_url(
        package: &PackageInterfaceHandle,
        _: &mut Call<'_>,
    ) -> Result<PluginValue, PhpThrow> {
        Ok(package
            .as_rc()
            .borrow()
            .as_package_interface()
            .get_source_url()
            .to_wire())
    }

    fn get_source_urls(
        package: &PackageInterfaceHandle,
        _: &mut Call<'_>,
    ) -> Result<PluginValue, PhpThrow> {
        Ok(package
            .as_rc()
            .borrow()
            .as_package_interface()
            .get_source_urls()
            .to_wire())
    }

    fn get_source_reference(
        package: &PackageInterfaceHandle,
        _: &mut Call<'_>,
    ) -> Result<PluginValue, PhpThrow> {
        Ok(package
            .as_rc()
            .borrow()
            .as_package_interface()
            .get_source_reference()
            .to_wire())
    }

    fn get_source_mirrors(
        package: &PackageInterfaceHandle,
        _: &mut Call<'_>,
    ) -> Result<PluginValue, PhpThrow> {
        Ok(package
            .as_rc()
            .borrow()
            .as_package_interface()
            .get_source_mirrors()
            .to_wire())
    }

    fn set_source_mirrors(
        package: &PackageInterfaceHandle,
        call: &mut Call<'_>,
    ) -> Result<PluginValue, PhpThrow> {
        let value = call.arg::<Option<Vec<Mirror>>>(0)?;
        package
            .as_rc()
            .borrow_mut()
            .as_package_interface_mut()
            .set_source_mirrors(value);
        Ok(PluginValue::Null)
    }

    fn get_dist_type(
        package: &PackageInterfaceHandle,
        _: &mut Call<'_>,
    ) -> Result<PluginValue, PhpThrow> {
        Ok(package
            .as_rc()
            .borrow()
            .as_package_interface()
            .get_dist_type()
            .to_wire())
    }

    fn get_dist_url(
        package: &PackageInterfaceHandle,
        _: &mut Call<'_>,
    ) -> Result<PluginValue, PhpThrow> {
        Ok(package
            .as_rc()
            .borrow()
            .as_package_interface()
            .get_dist_url()
            .to_wire())
    }

    fn get_dist_urls(
        package: &PackageInterfaceHandle,
        _: &mut Call<'_>,
    ) -> Result<PluginValue, PhpThrow> {
        Ok(package
            .as_rc()
            .borrow()
            .as_package_interface()
            .get_dist_urls()
            .to_wire())
    }

    fn get_dist_reference(
        package: &PackageInterfaceHandle,
        _: &mut Call<'_>,
    ) -> Result<PluginValue, PhpThrow> {
        Ok(package
            .as_rc()
            .borrow()
            .as_package_interface()
            .get_dist_reference()
            .to_wire())
    }

    fn get_dist_sha1_checksum(
        package: &PackageInterfaceHandle,
        _: &mut Call<'_>,
    ) -> Result<PluginValue, PhpThrow> {
        Ok(package
            .as_rc()
            .borrow()
            .as_package_interface()
            .get_dist_sha1_checksum()
            .to_wire())
    }

    fn get_dist_mirrors(
        package: &PackageInterfaceHandle,
        _: &mut Call<'_>,
    ) -> Result<PluginValue, PhpThrow> {
        Ok(package
            .as_rc()
            .borrow()
            .as_package_interface()
            .get_dist_mirrors()
            .to_wire())
    }

    fn set_dist_mirrors(
        package: &PackageInterfaceHandle,
        call: &mut Call<'_>,
    ) -> Result<PluginValue, PhpThrow> {
        let value = call.arg::<Option<Vec<Mirror>>>(0)?;
        package
            .as_rc()
            .borrow_mut()
            .as_package_interface_mut()
            .set_dist_mirrors(value);
        Ok(PluginValue::Null)
    }

    fn get_version(
        package: &PackageInterfaceHandle,
        _: &mut Call<'_>,
    ) -> Result<PluginValue, PhpThrow> {
        Ok(package
            .as_rc()
            .borrow()
            .as_package_interface()
            .get_version()
            .to_wire())
    }

    fn get_pretty_version(
        package: &PackageInterfaceHandle,
        _: &mut Call<'_>,
    ) -> Result<PluginValue, PhpThrow> {
        Ok(package
            .as_rc()
            .borrow()
            .as_package_interface()
            .get_pretty_version()
            .to_wire())
    }

    fn get_full_pretty_version(
        package: &PackageInterfaceHandle,
        call: &mut Call<'_>,
    ) -> Result<PluginValue, PhpThrow> {
        let truncate = call.arg_or(0, true)?;
        let display_mode = call.arg_or(1, DisplayMode::SourceRefIfDev)?;
        Ok(package
            .as_rc()
            .borrow()
            .as_package_interface()
            .get_full_pretty_version(truncate, display_mode)
            .to_wire())
    }

    fn get_release_date(
        package: &PackageInterfaceHandle,
        _: &mut Call<'_>,
    ) -> Result<PluginValue, PhpThrow> {
        Ok(package
            .as_rc()
            .borrow()
            .as_package_interface()
            .get_release_date()
            .to_wire())
    }

    fn get_stability(
        package: &PackageInterfaceHandle,
        _: &mut Call<'_>,
    ) -> Result<PluginValue, PhpThrow> {
        Ok(package
            .as_rc()
            .borrow()
            .as_package_interface()
            .get_stability()
            .to_wire())
    }

    fn get_requires(
        package: &PackageInterfaceHandle,
        _: &mut Call<'_>,
    ) -> Result<PluginValue, PhpThrow> {
        Ok((*package
            .as_rc()
            .borrow()
            .as_package_interface()
            .get_requires())
        .clone()
        .to_wire())
    }

    fn get_conflicts(
        package: &PackageInterfaceHandle,
        _: &mut Call<'_>,
    ) -> Result<PluginValue, PhpThrow> {
        Ok((*package
            .as_rc()
            .borrow()
            .as_package_interface()
            .get_conflicts())
        .clone()
        .to_wire())
    }

    fn get_provides(
        package: &PackageInterfaceHandle,
        _: &mut Call<'_>,
    ) -> Result<PluginValue, PhpThrow> {
        Ok((*package
            .as_rc()
            .borrow()
            .as_package_interface()
            .get_provides())
        .clone()
        .to_wire())
    }

    fn get_replaces(
        package: &PackageInterfaceHandle,
        _: &mut Call<'_>,
    ) -> Result<PluginValue, PhpThrow> {
        Ok((*package
            .as_rc()
            .borrow()
            .as_package_interface()
            .get_replaces())
        .clone()
        .to_wire())
    }

    fn get_dev_requires(
        package: &PackageInterfaceHandle,
        _: &mut Call<'_>,
    ) -> Result<PluginValue, PhpThrow> {
        Ok((*package
            .as_rc()
            .borrow()
            .as_package_interface()
            .get_dev_requires())
        .clone()
        .to_wire())
    }

    fn get_suggests(
        package: &PackageInterfaceHandle,
        _: &mut Call<'_>,
    ) -> Result<PluginValue, PhpThrow> {
        Ok(package
            .as_rc()
            .borrow()
            .as_package_interface()
            .get_suggests()
            .to_wire())
    }

    fn get_autoload(
        package: &PackageInterfaceHandle,
        _: &mut Call<'_>,
    ) -> Result<PluginValue, PhpThrow> {
        Ok(package
            .as_rc()
            .borrow()
            .as_package_interface()
            .get_autoload()
            .to_wire())
    }

    fn get_dev_autoload(
        package: &PackageInterfaceHandle,
        _: &mut Call<'_>,
    ) -> Result<PluginValue, PhpThrow> {
        Ok(package
            .as_rc()
            .borrow()
            .as_package_interface()
            .get_dev_autoload()
            .to_wire())
    }

    fn get_include_paths(
        package: &PackageInterfaceHandle,
        _: &mut Call<'_>,
    ) -> Result<PluginValue, PhpThrow> {
        Ok(package
            .as_rc()
            .borrow()
            .as_package_interface()
            .get_include_paths()
            .to_wire())
    }

    fn get_php_ext(
        package: &PackageInterfaceHandle,
        _: &mut Call<'_>,
    ) -> Result<PluginValue, PhpThrow> {
        Ok(
            match package
                .as_rc()
                .borrow()
                .as_package_interface()
                .get_php_ext()
            {
                Some(config) => config.to_wire(),
                None => PluginValue::Null,
            },
        )
    }

    fn set_repository(
        package: &PackageInterfaceHandle,
        call: &mut Call<'_>,
    ) -> Result<PluginValue, PhpThrow> {
        let repository = call.arg::<RepositoryInterfaceHandle>(0)?;
        package
            .as_rc()
            .borrow_mut()
            .as_package_interface_mut()
            .set_repository(repository)
            .map_err(call.failed())?;
        Ok(PluginValue::Null)
    }

    fn get_repository(
        package: &PackageInterfaceHandle,
        _: &mut Call<'_>,
    ) -> Result<PluginValue, PhpThrow> {
        let repository = package
            .as_rc()
            .borrow()
            .as_package_interface()
            .get_repository();
        match repository {
            Some(repository) => entity_value(&repository),
            None => Ok(PluginValue::Null),
        }
    }

    fn get_binaries(
        package: &PackageInterfaceHandle,
        _: &mut Call<'_>,
    ) -> Result<PluginValue, PhpThrow> {
        Ok(package
            .as_rc()
            .borrow()
            .as_package_interface()
            .get_binaries()
            .to_wire())
    }

    fn get_unique_name(
        package: &PackageInterfaceHandle,
        _: &mut Call<'_>,
    ) -> Result<PluginValue, PhpThrow> {
        Ok(package
            .as_rc()
            .borrow()
            .as_package_interface()
            .get_unique_name()
            .to_wire())
    }

    fn get_notification_url(
        package: &PackageInterfaceHandle,
        _: &mut Call<'_>,
    ) -> Result<PluginValue, PhpThrow> {
        Ok(package
            .as_rc()
            .borrow()
            .as_package_interface()
            .get_notification_url()
            .to_wire())
    }

    fn __to_string(
        package: &PackageInterfaceHandle,
        _: &mut Call<'_>,
    ) -> Result<PluginValue, PhpThrow> {
        Ok(package
            .as_rc()
            .borrow()
            .as_package_interface()
            .get_unique_name()
            .to_wire())
    }

    fn get_pretty_string(
        package: &PackageInterfaceHandle,
        _: &mut Call<'_>,
    ) -> Result<PluginValue, PhpThrow> {
        Ok(package
            .as_rc()
            .borrow()
            .as_package_interface()
            .get_pretty_string()
            .to_wire())
    }

    fn is_default_branch(
        package: &PackageInterfaceHandle,
        _: &mut Call<'_>,
    ) -> Result<PluginValue, PhpThrow> {
        Ok(package
            .as_rc()
            .borrow()
            .as_package_interface()
            .is_default_branch()
            .to_wire())
    }

    fn get_transport_options(
        package: &PackageInterfaceHandle,
        _: &mut Call<'_>,
    ) -> Result<PluginValue, PhpThrow> {
        Ok(package
            .as_rc()
            .borrow()
            .as_package_interface()
            .get_transport_options()
            .to_wire())
    }

    fn set_transport_options(
        package: &PackageInterfaceHandle,
        call: &mut Call<'_>,
    ) -> Result<PluginValue, PhpThrow> {
        let options = call.arg_or(0, IndexMap::<BString, PhpMixed>::new())?;
        package
            .as_rc()
            .borrow_mut()
            .as_package_interface_mut()
            .set_transport_options(options);
        Ok(PluginValue::Null)
    }

    fn set_source_reference(
        package: &PackageInterfaceHandle,
        call: &mut Call<'_>,
    ) -> Result<PluginValue, PhpThrow> {
        let value = call.arg::<Option<BString>>(0)?;
        package
            .as_rc()
            .borrow_mut()
            .as_package_interface_mut()
            .set_source_reference(value);
        Ok(PluginValue::Null)
    }

    fn set_dist_url(
        package: &PackageInterfaceHandle,
        call: &mut Call<'_>,
    ) -> Result<PluginValue, PhpThrow> {
        let value = call.arg::<Option<BString>>(0)?;
        package
            .as_rc()
            .borrow_mut()
            .as_package_interface_mut()
            .set_dist_url(value);
        Ok(PluginValue::Null)
    }

    fn set_dist_type(
        package: &PackageInterfaceHandle,
        call: &mut Call<'_>,
    ) -> Result<PluginValue, PhpThrow> {
        let value = call.arg::<Option<BString>>(0)?;
        package
            .as_rc()
            .borrow_mut()
            .as_package_interface_mut()
            .set_dist_type(value);
        Ok(PluginValue::Null)
    }

    fn set_dist_reference(
        package: &PackageInterfaceHandle,
        call: &mut Call<'_>,
    ) -> Result<PluginValue, PhpThrow> {
        let value = call.arg::<Option<BString>>(0)?;
        package
            .as_rc()
            .borrow_mut()
            .as_package_interface_mut()
            .set_dist_reference(value);
        Ok(PluginValue::Null)
    }

    fn set_source_dist_references(
        package: &PackageInterfaceHandle,
        call: &mut Call<'_>,
    ) -> Result<PluginValue, PhpThrow> {
        let value = call.arg::<Option<BString>>(0)?.ok_or_else(|| {
            PhpThrow::runtime("setSourceDistReferences expects a reference string")
        })?;
        package
            .as_rc()
            .borrow_mut()
            .as_package_interface_mut()
            .set_source_dist_references(value);
        Ok(PluginValue::Null)
    }

    // `BasePackage`'s concrete methods are not forwarded by `PackageInterface`, so both are
    // computed from the interface here, as `VersionSelector` already does for the second.
    fn is_platform(
        package: &PackageInterfaceHandle,
        _: &mut Call<'_>,
    ) -> Result<PluginValue, PhpThrow> {
        Ok(package
            .as_rc()
            .borrow()
            .as_package_interface()
            .get_repository()
            .is_some_and(|repository| repository.is::<PlatformRepository>())
            .to_wire())
    }

    fn equals(
        package: &PackageInterfaceHandle,
        call: &mut Call<'_>,
    ) -> Result<PluginValue, PhpThrow> {
        let other = call.arg::<PackageInterfaceHandle>(0)?;
        Ok(package.equals(&other).to_wire())
    }

    fn get_stability_priority(
        package: &PackageInterfaceHandle,
        _: &mut Call<'_>,
    ) -> Result<PluginValue, PhpThrow> {
        Ok(stability_priority(
            package
                .as_rc()
                .borrow()
                .as_package_interface()
                .get_stability(),
        )
        .unwrap_or(STABILITY_STABLE)
        .to_wire())
    }
}

impl forwarding::package::package::Forwarding for forwarding::package::package::PhpClass {
    fn set_type(
        package: &PackageInterfaceHandle,
        call: &mut Call<'_>,
    ) -> Result<PluginValue, PhpThrow> {
        let value = call.arg::<BString>(0)?;
        package
            .as_rc()
            .borrow_mut()
            .as_package_mut()
            .expect(REAL)
            .set_type(value);
        Ok(PluginValue::Null)
    }

    fn set_target_dir(
        package: &PackageInterfaceHandle,
        call: &mut Call<'_>,
    ) -> Result<PluginValue, PhpThrow> {
        let value = call.arg::<Option<BString>>(0)?;
        package
            .as_rc()
            .borrow_mut()
            .as_package_mut()
            .expect(REAL)
            .set_target_dir(value);
        Ok(PluginValue::Null)
    }

    fn set_extra(
        package: &PackageInterfaceHandle,
        call: &mut Call<'_>,
    ) -> Result<PluginValue, PhpThrow> {
        let value = call.arg::<IndexMap<BString, PhpMixed>>(0)?;
        package
            .as_rc()
            .borrow_mut()
            .as_package_mut()
            .expect(REAL)
            .set_extra(value);
        Ok(PluginValue::Null)
    }

    fn set_binaries(
        package: &PackageInterfaceHandle,
        call: &mut Call<'_>,
    ) -> Result<PluginValue, PhpThrow> {
        let value = call.arg::<Vec<BString>>(0)?;
        package
            .as_rc()
            .borrow_mut()
            .as_package_mut()
            .expect(REAL)
            .set_binaries(value);
        Ok(PluginValue::Null)
    }

    fn set_source_type(
        package: &PackageInterfaceHandle,
        call: &mut Call<'_>,
    ) -> Result<PluginValue, PhpThrow> {
        let value = call.arg::<Option<BString>>(0)?;
        package
            .as_rc()
            .borrow_mut()
            .as_package_mut()
            .expect(REAL)
            .set_source_type(value);
        Ok(PluginValue::Null)
    }

    fn set_source_url(
        package: &PackageInterfaceHandle,
        call: &mut Call<'_>,
    ) -> Result<PluginValue, PhpThrow> {
        let value = call.arg::<Option<BString>>(0)?;
        package
            .as_rc()
            .borrow_mut()
            .as_package_interface_mut()
            .set_source_url(value);
        Ok(PluginValue::Null)
    }

    fn set_dist_sha1_checksum(
        package: &PackageInterfaceHandle,
        call: &mut Call<'_>,
    ) -> Result<PluginValue, PhpThrow> {
        let value = call.arg::<Option<BString>>(0)?;
        package
            .as_rc()
            .borrow_mut()
            .as_package_mut()
            .expect(REAL)
            .set_dist_sha1_checksum(value);
        Ok(PluginValue::Null)
    }

    fn set_release_date(
        package: &PackageInterfaceHandle,
        call: &mut Call<'_>,
    ) -> Result<PluginValue, PhpThrow> {
        let value = call.arg::<Option<chrono::DateTime<chrono::Utc>>>(0)?;
        package
            .as_rc()
            .borrow_mut()
            .as_package_mut()
            .expect(REAL)
            .set_release_date(value);
        Ok(PluginValue::Null)
    }

    fn set_requires(
        package: &PackageInterfaceHandle,
        call: &mut Call<'_>,
    ) -> Result<PluginValue, PhpThrow> {
        package
            .as_rc()
            .borrow_mut()
            .as_package_mut()
            .expect(REAL)
            .set_requires(links(call)?);
        Ok(PluginValue::Null)
    }

    fn set_conflicts(
        package: &PackageInterfaceHandle,
        call: &mut Call<'_>,
    ) -> Result<PluginValue, PhpThrow> {
        package
            .as_rc()
            .borrow_mut()
            .as_package_mut()
            .expect(REAL)
            .set_conflicts(links(call)?);
        Ok(PluginValue::Null)
    }

    fn set_provides(
        package: &PackageInterfaceHandle,
        call: &mut Call<'_>,
    ) -> Result<PluginValue, PhpThrow> {
        package
            .as_rc()
            .borrow_mut()
            .as_package_mut()
            .expect(REAL)
            .set_provides(links(call)?);
        Ok(PluginValue::Null)
    }

    fn set_replaces(
        package: &PackageInterfaceHandle,
        call: &mut Call<'_>,
    ) -> Result<PluginValue, PhpThrow> {
        package
            .as_rc()
            .borrow_mut()
            .as_package_mut()
            .expect(REAL)
            .set_replaces(links(call)?);
        Ok(PluginValue::Null)
    }

    fn set_dev_requires(
        package: &PackageInterfaceHandle,
        call: &mut Call<'_>,
    ) -> Result<PluginValue, PhpThrow> {
        package
            .as_rc()
            .borrow_mut()
            .as_package_mut()
            .expect(REAL)
            .set_dev_requires(links(call)?);
        Ok(PluginValue::Null)
    }

    fn set_suggests(
        package: &PackageInterfaceHandle,
        call: &mut Call<'_>,
    ) -> Result<PluginValue, PhpThrow> {
        let value = call.arg::<IndexMap<BString, BString>>(0)?;
        package
            .as_rc()
            .borrow_mut()
            .as_package_mut()
            .expect(REAL)
            .set_suggests(value);
        Ok(PluginValue::Null)
    }

    fn set_autoload(
        package: &PackageInterfaceHandle,
        call: &mut Call<'_>,
    ) -> Result<PluginValue, PhpThrow> {
        let value = call.arg::<IndexMap<BString, PhpMixed>>(0)?;
        package
            .as_rc()
            .borrow_mut()
            .as_package_mut()
            .expect(REAL)
            .set_autoload(value);
        Ok(PluginValue::Null)
    }

    fn set_dev_autoload(
        package: &PackageInterfaceHandle,
        call: &mut Call<'_>,
    ) -> Result<PluginValue, PhpThrow> {
        let value = call.arg::<IndexMap<BString, PhpMixed>>(0)?;
        package
            .as_rc()
            .borrow_mut()
            .as_package_mut()
            .expect(REAL)
            .set_dev_autoload(value);
        Ok(PluginValue::Null)
    }

    fn set_include_paths(
        package: &PackageInterfaceHandle,
        call: &mut Call<'_>,
    ) -> Result<PluginValue, PhpThrow> {
        let value = call.arg::<Vec<BString>>(0)?;
        package
            .as_rc()
            .borrow_mut()
            .as_package_mut()
            .expect(REAL)
            .set_include_paths(value);
        Ok(PluginValue::Null)
    }

    fn set_php_ext(
        package: &PackageInterfaceHandle,
        call: &mut Call<'_>,
    ) -> Result<PluginValue, PhpThrow> {
        let value = call.arg::<Option<IndexMap<BString, PhpMixed>>>(0)?;
        package
            .as_rc()
            .borrow_mut()
            .as_package_mut()
            .expect(REAL)
            .set_php_ext(value);
        Ok(PluginValue::Null)
    }

    fn set_notification_url(
        package: &PackageInterfaceHandle,
        call: &mut Call<'_>,
    ) -> Result<PluginValue, PhpThrow> {
        let value = call.arg::<BString>(0)?;
        package
            .as_rc()
            .borrow_mut()
            .as_package_mut()
            .expect(REAL)
            .set_notification_url(value);
        Ok(PluginValue::Null)
    }

    fn set_is_default_branch(
        package: &PackageInterfaceHandle,
        call: &mut Call<'_>,
    ) -> Result<PluginValue, PhpThrow> {
        let value = call.arg::<bool>(0)?;
        package
            .as_rc()
            .borrow_mut()
            .as_package_mut()
            .expect(REAL)
            .set_is_default_branch(value);
        Ok(PluginValue::Null)
    }

    fn replace_version(
        package: &PackageInterfaceHandle,
        call: &mut Call<'_>,
    ) -> Result<PluginValue, PhpThrow> {
        let version = call.arg::<BString>(0)?;
        let pretty_version = call.arg::<BString>(1)?;
        package
            .as_rc()
            .borrow_mut()
            .as_package_mut()
            .expect(REAL)
            .replace_version(version, pretty_version);
        Ok(PluginValue::Null)
    }

    fn __construct(call: &mut Call<'_>) -> Result<PackageInterfaceHandle, PhpThrow> {
        Ok(constructed(AnyPackage::Package(Package::new(
            call.arg(0)?,
            call.arg(1)?,
            call.arg(2)?,
        ))))
    }
}

impl forwarding::package::complete_package::Forwarding
    for forwarding::package::complete_package::PhpClass
{
    fn get_scripts(
        package: &PackageInterfaceHandle,
        _: &mut Call<'_>,
    ) -> Result<PluginValue, PhpThrow> {
        Ok(package
            .as_rc()
            .borrow()
            .as_complete_package_interface()
            .expect(COMPLETE)
            .get_scripts()
            .to_wire())
    }

    fn set_scripts(
        package: &PackageInterfaceHandle,
        call: &mut Call<'_>,
    ) -> Result<PluginValue, PhpThrow> {
        let value = call.arg::<IndexMap<BString, Vec<BString>>>(0)?;
        package
            .as_rc()
            .borrow_mut()
            .as_complete_package_interface_mut()
            .expect(COMPLETE)
            .set_scripts(value);
        Ok(PluginValue::Null)
    }

    fn get_repositories(
        package: &PackageInterfaceHandle,
        _: &mut Call<'_>,
    ) -> Result<PluginValue, PhpThrow> {
        Ok(package
            .as_rc()
            .borrow()
            .as_complete_package_interface()
            .expect(COMPLETE)
            .get_repositories()
            .to_wire())
    }

    fn set_repositories(
        package: &PackageInterfaceHandle,
        call: &mut Call<'_>,
    ) -> Result<PluginValue, PhpThrow> {
        let value = call.arg::<IndexMap<BString, PhpMixed>>(0)?;
        package
            .as_rc()
            .borrow_mut()
            .as_complete_package_interface_mut()
            .expect(COMPLETE)
            .set_repositories(value);
        Ok(PluginValue::Null)
    }

    fn get_license(
        package: &PackageInterfaceHandle,
        _: &mut Call<'_>,
    ) -> Result<PluginValue, PhpThrow> {
        Ok(package
            .as_rc()
            .borrow()
            .as_complete_package_interface()
            .expect(COMPLETE)
            .get_license()
            .to_wire())
    }

    fn set_license(
        package: &PackageInterfaceHandle,
        call: &mut Call<'_>,
    ) -> Result<PluginValue, PhpThrow> {
        let value = call.arg::<Vec<BString>>(0)?;
        package
            .as_rc()
            .borrow_mut()
            .as_complete_package_interface_mut()
            .expect(COMPLETE)
            .set_license(value);
        Ok(PluginValue::Null)
    }

    fn get_keywords(
        package: &PackageInterfaceHandle,
        _: &mut Call<'_>,
    ) -> Result<PluginValue, PhpThrow> {
        Ok(package
            .as_rc()
            .borrow()
            .as_complete_package_interface()
            .expect(COMPLETE)
            .get_keywords()
            .to_wire())
    }

    fn set_keywords(
        package: &PackageInterfaceHandle,
        call: &mut Call<'_>,
    ) -> Result<PluginValue, PhpThrow> {
        let value = call.arg::<Vec<BString>>(0)?;
        package
            .as_rc()
            .borrow_mut()
            .as_complete_package_interface_mut()
            .expect(COMPLETE)
            .set_keywords(value);
        Ok(PluginValue::Null)
    }

    fn get_description(
        package: &PackageInterfaceHandle,
        _: &mut Call<'_>,
    ) -> Result<PluginValue, PhpThrow> {
        Ok(package
            .as_rc()
            .borrow()
            .as_complete_package_interface()
            .expect(COMPLETE)
            .get_description()
            .to_wire())
    }

    fn set_description(
        package: &PackageInterfaceHandle,
        call: &mut Call<'_>,
    ) -> Result<PluginValue, PhpThrow> {
        let value = call.arg::<BString>(0)?;
        package
            .as_rc()
            .borrow_mut()
            .as_complete_package_interface_mut()
            .expect(COMPLETE)
            .set_description(value);
        Ok(PluginValue::Null)
    }

    fn get_homepage(
        package: &PackageInterfaceHandle,
        _: &mut Call<'_>,
    ) -> Result<PluginValue, PhpThrow> {
        Ok(package
            .as_rc()
            .borrow()
            .as_complete_package_interface()
            .expect(COMPLETE)
            .get_homepage()
            .to_wire())
    }

    fn set_homepage(
        package: &PackageInterfaceHandle,
        call: &mut Call<'_>,
    ) -> Result<PluginValue, PhpThrow> {
        let value = call.arg::<BString>(0)?;
        package
            .as_rc()
            .borrow_mut()
            .as_complete_package_interface_mut()
            .expect(COMPLETE)
            .set_homepage(value);
        Ok(PluginValue::Null)
    }

    fn get_authors(
        package: &PackageInterfaceHandle,
        _: &mut Call<'_>,
    ) -> Result<PluginValue, PhpThrow> {
        Ok(package
            .as_rc()
            .borrow()
            .as_complete_package_interface()
            .expect(COMPLETE)
            .get_authors()
            .to_wire())
    }

    fn set_authors(
        package: &PackageInterfaceHandle,
        call: &mut Call<'_>,
    ) -> Result<PluginValue, PhpThrow> {
        let value = call.arg::<Vec<IndexMap<BString, BString>>>(0)?;
        package
            .as_rc()
            .borrow_mut()
            .as_complete_package_interface_mut()
            .expect(COMPLETE)
            .set_authors(value);
        Ok(PluginValue::Null)
    }

    fn get_support(
        package: &PackageInterfaceHandle,
        _: &mut Call<'_>,
    ) -> Result<PluginValue, PhpThrow> {
        Ok(package
            .as_rc()
            .borrow()
            .as_complete_package_interface()
            .expect(COMPLETE)
            .get_support()
            .to_wire())
    }

    fn set_support(
        package: &PackageInterfaceHandle,
        call: &mut Call<'_>,
    ) -> Result<PluginValue, PhpThrow> {
        let value = call.arg::<IndexMap<BString, BString>>(0)?;
        package
            .as_rc()
            .borrow_mut()
            .as_complete_package_interface_mut()
            .expect(COMPLETE)
            .set_support(value);
        Ok(PluginValue::Null)
    }

    fn get_funding(
        package: &PackageInterfaceHandle,
        _: &mut Call<'_>,
    ) -> Result<PluginValue, PhpThrow> {
        Ok(package
            .as_rc()
            .borrow()
            .as_complete_package_interface()
            .expect(COMPLETE)
            .get_funding()
            .to_wire())
    }

    fn set_funding(
        package: &PackageInterfaceHandle,
        call: &mut Call<'_>,
    ) -> Result<PluginValue, PhpThrow> {
        let value = call.arg::<Vec<IndexMap<BString, PhpMixed>>>(0)?;
        package
            .as_rc()
            .borrow_mut()
            .as_complete_package_interface_mut()
            .expect(COMPLETE)
            .set_funding(value);
        Ok(PluginValue::Null)
    }

    fn is_abandoned(
        package: &PackageInterfaceHandle,
        _: &mut Call<'_>,
    ) -> Result<PluginValue, PhpThrow> {
        Ok(package
            .as_rc()
            .borrow()
            .as_complete_package_interface()
            .expect(COMPLETE)
            .is_abandoned()
            .to_wire())
    }

    fn get_replacement_package(
        package: &PackageInterfaceHandle,
        _: &mut Call<'_>,
    ) -> Result<PluginValue, PhpThrow> {
        Ok(package
            .as_rc()
            .borrow()
            .as_complete_package_interface()
            .expect(COMPLETE)
            .get_replacement_package()
            .to_wire())
    }

    fn set_abandoned(
        package: &PackageInterfaceHandle,
        call: &mut Call<'_>,
    ) -> Result<PluginValue, PhpThrow> {
        let value = call.arg::<PhpMixed>(0)?;
        package
            .as_rc()
            .borrow_mut()
            .as_complete_package_interface_mut()
            .expect(COMPLETE)
            .set_abandoned(value);
        Ok(PluginValue::Null)
    }

    fn get_archive_name(
        package: &PackageInterfaceHandle,
        _: &mut Call<'_>,
    ) -> Result<PluginValue, PhpThrow> {
        Ok(package
            .as_rc()
            .borrow()
            .as_complete_package_interface()
            .expect(COMPLETE)
            .get_archive_name()
            .to_wire())
    }

    fn set_archive_name(
        package: &PackageInterfaceHandle,
        call: &mut Call<'_>,
    ) -> Result<PluginValue, PhpThrow> {
        let value = call.arg::<BString>(0)?;
        package
            .as_rc()
            .borrow_mut()
            .as_complete_package_interface_mut()
            .expect(COMPLETE)
            .set_archive_name(value);
        Ok(PluginValue::Null)
    }

    fn get_archive_excludes(
        package: &PackageInterfaceHandle,
        _: &mut Call<'_>,
    ) -> Result<PluginValue, PhpThrow> {
        Ok(package
            .as_rc()
            .borrow()
            .as_complete_package_interface()
            .expect(COMPLETE)
            .get_archive_excludes()
            .to_wire())
    }

    fn set_archive_excludes(
        package: &PackageInterfaceHandle,
        call: &mut Call<'_>,
    ) -> Result<PluginValue, PhpThrow> {
        let value = call.arg::<Vec<BString>>(0)?;
        package
            .as_rc()
            .borrow_mut()
            .as_complete_package_interface_mut()
            .expect(COMPLETE)
            .set_archive_excludes(value);
        Ok(PluginValue::Null)
    }

    fn __construct(call: &mut Call<'_>) -> Result<PackageInterfaceHandle, PhpThrow> {
        Ok(constructed(AnyPackage::CompletePackage(
            CompletePackage::new(call.arg(0)?, call.arg(1)?, call.arg(2)?),
        )))
    }
}

// `RootPackage` inherits `Package`'s setters of the links, the autoload maps, the suggestions and
// the extra, which write the same base state.
impl forwarding::package::root_package::Forwarding for forwarding::package::root_package::PhpClass {
    fn get_aliases(
        package: &PackageInterfaceHandle,
        _: &mut Call<'_>,
    ) -> Result<PluginValue, PhpThrow> {
        Ok(package
            .as_rc()
            .borrow()
            .as_root_package_interface()
            .expect(ROOT)
            .get_aliases()
            .to_wire())
    }

    fn get_minimum_stability(
        package: &PackageInterfaceHandle,
        _: &mut Call<'_>,
    ) -> Result<PluginValue, PhpThrow> {
        Ok(PluginValue::string(
            package
                .as_rc()
                .borrow()
                .as_root_package_interface()
                .expect(ROOT)
                .get_minimum_stability(),
        ))
    }

    fn get_stability_flags(
        package: &PackageInterfaceHandle,
        _: &mut Call<'_>,
    ) -> Result<PluginValue, PhpThrow> {
        Ok(package
            .as_rc()
            .borrow()
            .as_root_package_interface()
            .expect(ROOT)
            .get_stability_flags()
            .to_wire())
    }

    fn get_references(
        package: &PackageInterfaceHandle,
        _: &mut Call<'_>,
    ) -> Result<PluginValue, PhpThrow> {
        Ok(package
            .as_rc()
            .borrow()
            .as_root_package_interface()
            .expect(ROOT)
            .get_references()
            .to_wire())
    }

    fn get_prefer_stable(
        package: &PackageInterfaceHandle,
        _: &mut Call<'_>,
    ) -> Result<PluginValue, PhpThrow> {
        Ok(package
            .as_rc()
            .borrow()
            .as_root_package_interface()
            .expect(ROOT)
            .get_prefer_stable()
            .to_wire())
    }

    fn get_config(
        package: &PackageInterfaceHandle,
        _: &mut Call<'_>,
    ) -> Result<PluginValue, PhpThrow> {
        Ok(package
            .as_rc()
            .borrow()
            .as_root_package_interface()
            .expect(ROOT)
            .get_config()
            .to_wire())
    }

    fn set_stability_flags(
        package: &PackageInterfaceHandle,
        call: &mut Call<'_>,
    ) -> Result<PluginValue, PhpThrow> {
        let value = call.arg::<IndexMap<BString, i64>>(0)?;
        package
            .as_rc()
            .borrow_mut()
            .as_root_package_interface_mut()
            .expect(ROOT)
            .set_stability_flags(value);
        Ok(PluginValue::Null)
    }

    fn set_minimum_stability(
        package: &PackageInterfaceHandle,
        call: &mut Call<'_>,
    ) -> Result<PluginValue, PhpThrow> {
        let value = call.arg::<BString>(0)?;
        package
            .as_rc()
            .borrow_mut()
            .as_root_package_interface_mut()
            .expect(ROOT)
            .set_minimum_stability(value);
        Ok(PluginValue::Null)
    }

    fn set_prefer_stable(
        package: &PackageInterfaceHandle,
        call: &mut Call<'_>,
    ) -> Result<PluginValue, PhpThrow> {
        let value = call.arg::<bool>(0)?;
        package
            .as_rc()
            .borrow_mut()
            .as_root_package_interface_mut()
            .expect(ROOT)
            .set_prefer_stable(value);
        Ok(PluginValue::Null)
    }

    fn set_config(
        package: &PackageInterfaceHandle,
        call: &mut Call<'_>,
    ) -> Result<PluginValue, PhpThrow> {
        let value = call.arg::<IndexMap<BString, PhpMixed>>(0)?;
        package
            .as_rc()
            .borrow_mut()
            .as_root_package_interface_mut()
            .expect(ROOT)
            .set_config(value);
        Ok(PluginValue::Null)
    }

    fn set_references(
        package: &PackageInterfaceHandle,
        call: &mut Call<'_>,
    ) -> Result<PluginValue, PhpThrow> {
        let value = call.arg::<IndexMap<BString, BString>>(0)?;
        package
            .as_rc()
            .borrow_mut()
            .as_root_package_interface_mut()
            .expect(ROOT)
            .set_references(value);
        Ok(PluginValue::Null)
    }

    fn set_aliases(
        package: &PackageInterfaceHandle,
        call: &mut Call<'_>,
    ) -> Result<PluginValue, PhpThrow> {
        let value = call.arg::<Vec<IndexMap<BString, BString>>>(0)?;
        package
            .as_rc()
            .borrow_mut()
            .as_root_package_interface_mut()
            .expect(ROOT)
            .set_aliases(value);
        Ok(PluginValue::Null)
    }
}

impl forwarding::package::alias_package::Forwarding
    for forwarding::package::alias_package::PhpClass
{
    // The subclasses narrow `getAliasOf`'s return type to their own alias target, but every
    // variant holds the one entity.
    fn get_alias_of(
        package: &PackageInterfaceHandle,
        _: &mut Call<'_>,
    ) -> Result<PluginValue, PhpThrow> {
        package_value(
            package
                .as_rc()
                .borrow()
                .as_alias_package()
                .expect(ALIAS)
                .get_alias_of()
                .as_rc(),
        )
    }

    fn set_root_package_alias(
        package: &PackageInterfaceHandle,
        call: &mut Call<'_>,
    ) -> Result<PluginValue, PhpThrow> {
        let value = call.arg::<bool>(0)?;
        package
            .as_rc()
            .borrow_mut()
            .as_alias_package_mut()
            .expect(ALIAS)
            .set_root_package_alias(value);
        Ok(PluginValue::Null)
    }

    fn is_root_package_alias(
        package: &PackageInterfaceHandle,
        _: &mut Call<'_>,
    ) -> Result<PluginValue, PhpThrow> {
        Ok(package
            .as_rc()
            .borrow()
            .as_alias_package()
            .expect(ALIAS)
            .is_root_package_alias()
            .to_wire())
    }

    fn has_self_version_requires(
        package: &PackageInterfaceHandle,
        _: &mut Call<'_>,
    ) -> Result<PluginValue, PhpThrow> {
        Ok(package
            .as_rc()
            .borrow()
            .as_alias_package()
            .expect(ALIAS)
            .has_self_version_requires()
            .to_wire())
    }

    // The alias target has to be a package that already lives on the Rust side; an alias of an
    // alias has no Rust representation, so its narrowing is an explicit error too.
    fn __construct(call: &mut Call<'_>) -> Result<PackageInterfaceHandle, PhpThrow> {
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
}

impl forwarding::package::complete_alias_package::Forwarding
    for forwarding::package::complete_alias_package::PhpClass
{
    fn get_scripts(
        package: &PackageInterfaceHandle,
        _: &mut Call<'_>,
    ) -> Result<PluginValue, PhpThrow> {
        Ok(package
            .as_rc()
            .borrow()
            .as_complete_package_interface()
            .expect(COMPLETE)
            .get_scripts()
            .to_wire())
    }

    fn set_scripts(
        package: &PackageInterfaceHandle,
        call: &mut Call<'_>,
    ) -> Result<PluginValue, PhpThrow> {
        let value = call.arg::<IndexMap<BString, Vec<BString>>>(0)?;
        package
            .as_rc()
            .borrow_mut()
            .as_complete_package_interface_mut()
            .expect(COMPLETE)
            .set_scripts(value);
        Ok(PluginValue::Null)
    }

    fn get_repositories(
        package: &PackageInterfaceHandle,
        _: &mut Call<'_>,
    ) -> Result<PluginValue, PhpThrow> {
        Ok(package
            .as_rc()
            .borrow()
            .as_complete_package_interface()
            .expect(COMPLETE)
            .get_repositories()
            .to_wire())
    }

    fn set_repositories(
        package: &PackageInterfaceHandle,
        call: &mut Call<'_>,
    ) -> Result<PluginValue, PhpThrow> {
        let value = call.arg::<IndexMap<BString, PhpMixed>>(0)?;
        package
            .as_rc()
            .borrow_mut()
            .as_complete_package_interface_mut()
            .expect(COMPLETE)
            .set_repositories(value);
        Ok(PluginValue::Null)
    }

    fn get_license(
        package: &PackageInterfaceHandle,
        _: &mut Call<'_>,
    ) -> Result<PluginValue, PhpThrow> {
        Ok(package
            .as_rc()
            .borrow()
            .as_complete_package_interface()
            .expect(COMPLETE)
            .get_license()
            .to_wire())
    }

    fn set_license(
        package: &PackageInterfaceHandle,
        call: &mut Call<'_>,
    ) -> Result<PluginValue, PhpThrow> {
        let value = call.arg::<Vec<BString>>(0)?;
        package
            .as_rc()
            .borrow_mut()
            .as_complete_package_interface_mut()
            .expect(COMPLETE)
            .set_license(value);
        Ok(PluginValue::Null)
    }

    fn get_keywords(
        package: &PackageInterfaceHandle,
        _: &mut Call<'_>,
    ) -> Result<PluginValue, PhpThrow> {
        Ok(package
            .as_rc()
            .borrow()
            .as_complete_package_interface()
            .expect(COMPLETE)
            .get_keywords()
            .to_wire())
    }

    fn set_keywords(
        package: &PackageInterfaceHandle,
        call: &mut Call<'_>,
    ) -> Result<PluginValue, PhpThrow> {
        let value = call.arg::<Vec<BString>>(0)?;
        package
            .as_rc()
            .borrow_mut()
            .as_complete_package_interface_mut()
            .expect(COMPLETE)
            .set_keywords(value);
        Ok(PluginValue::Null)
    }

    fn get_description(
        package: &PackageInterfaceHandle,
        _: &mut Call<'_>,
    ) -> Result<PluginValue, PhpThrow> {
        Ok(package
            .as_rc()
            .borrow()
            .as_complete_package_interface()
            .expect(COMPLETE)
            .get_description()
            .to_wire())
    }

    fn set_description(
        package: &PackageInterfaceHandle,
        call: &mut Call<'_>,
    ) -> Result<PluginValue, PhpThrow> {
        let value = call.arg::<BString>(0)?;
        package
            .as_rc()
            .borrow_mut()
            .as_complete_package_interface_mut()
            .expect(COMPLETE)
            .set_description(value);
        Ok(PluginValue::Null)
    }

    fn get_homepage(
        package: &PackageInterfaceHandle,
        _: &mut Call<'_>,
    ) -> Result<PluginValue, PhpThrow> {
        Ok(package
            .as_rc()
            .borrow()
            .as_complete_package_interface()
            .expect(COMPLETE)
            .get_homepage()
            .to_wire())
    }

    fn set_homepage(
        package: &PackageInterfaceHandle,
        call: &mut Call<'_>,
    ) -> Result<PluginValue, PhpThrow> {
        let value = call.arg::<BString>(0)?;
        package
            .as_rc()
            .borrow_mut()
            .as_complete_package_interface_mut()
            .expect(COMPLETE)
            .set_homepage(value);
        Ok(PluginValue::Null)
    }

    fn get_authors(
        package: &PackageInterfaceHandle,
        _: &mut Call<'_>,
    ) -> Result<PluginValue, PhpThrow> {
        Ok(package
            .as_rc()
            .borrow()
            .as_complete_package_interface()
            .expect(COMPLETE)
            .get_authors()
            .to_wire())
    }

    fn set_authors(
        package: &PackageInterfaceHandle,
        call: &mut Call<'_>,
    ) -> Result<PluginValue, PhpThrow> {
        let value = call.arg::<Vec<IndexMap<BString, BString>>>(0)?;
        package
            .as_rc()
            .borrow_mut()
            .as_complete_package_interface_mut()
            .expect(COMPLETE)
            .set_authors(value);
        Ok(PluginValue::Null)
    }

    fn get_support(
        package: &PackageInterfaceHandle,
        _: &mut Call<'_>,
    ) -> Result<PluginValue, PhpThrow> {
        Ok(package
            .as_rc()
            .borrow()
            .as_complete_package_interface()
            .expect(COMPLETE)
            .get_support()
            .to_wire())
    }

    fn set_support(
        package: &PackageInterfaceHandle,
        call: &mut Call<'_>,
    ) -> Result<PluginValue, PhpThrow> {
        let value = call.arg::<IndexMap<BString, BString>>(0)?;
        package
            .as_rc()
            .borrow_mut()
            .as_complete_package_interface_mut()
            .expect(COMPLETE)
            .set_support(value);
        Ok(PluginValue::Null)
    }

    fn get_funding(
        package: &PackageInterfaceHandle,
        _: &mut Call<'_>,
    ) -> Result<PluginValue, PhpThrow> {
        Ok(package
            .as_rc()
            .borrow()
            .as_complete_package_interface()
            .expect(COMPLETE)
            .get_funding()
            .to_wire())
    }

    fn set_funding(
        package: &PackageInterfaceHandle,
        call: &mut Call<'_>,
    ) -> Result<PluginValue, PhpThrow> {
        let value = call.arg::<Vec<IndexMap<BString, PhpMixed>>>(0)?;
        package
            .as_rc()
            .borrow_mut()
            .as_complete_package_interface_mut()
            .expect(COMPLETE)
            .set_funding(value);
        Ok(PluginValue::Null)
    }

    fn is_abandoned(
        package: &PackageInterfaceHandle,
        _: &mut Call<'_>,
    ) -> Result<PluginValue, PhpThrow> {
        Ok(package
            .as_rc()
            .borrow()
            .as_complete_package_interface()
            .expect(COMPLETE)
            .is_abandoned()
            .to_wire())
    }

    fn get_replacement_package(
        package: &PackageInterfaceHandle,
        _: &mut Call<'_>,
    ) -> Result<PluginValue, PhpThrow> {
        Ok(package
            .as_rc()
            .borrow()
            .as_complete_package_interface()
            .expect(COMPLETE)
            .get_replacement_package()
            .to_wire())
    }

    fn set_abandoned(
        package: &PackageInterfaceHandle,
        call: &mut Call<'_>,
    ) -> Result<PluginValue, PhpThrow> {
        let value = call.arg::<PhpMixed>(0)?;
        package
            .as_rc()
            .borrow_mut()
            .as_complete_package_interface_mut()
            .expect(COMPLETE)
            .set_abandoned(value);
        Ok(PluginValue::Null)
    }

    fn get_archive_name(
        package: &PackageInterfaceHandle,
        _: &mut Call<'_>,
    ) -> Result<PluginValue, PhpThrow> {
        Ok(package
            .as_rc()
            .borrow()
            .as_complete_package_interface()
            .expect(COMPLETE)
            .get_archive_name()
            .to_wire())
    }

    fn set_archive_name(
        package: &PackageInterfaceHandle,
        call: &mut Call<'_>,
    ) -> Result<PluginValue, PhpThrow> {
        let value = call.arg::<BString>(0)?;
        package
            .as_rc()
            .borrow_mut()
            .as_complete_package_interface_mut()
            .expect(COMPLETE)
            .set_archive_name(value);
        Ok(PluginValue::Null)
    }

    fn get_archive_excludes(
        package: &PackageInterfaceHandle,
        _: &mut Call<'_>,
    ) -> Result<PluginValue, PhpThrow> {
        Ok(package
            .as_rc()
            .borrow()
            .as_complete_package_interface()
            .expect(COMPLETE)
            .get_archive_excludes()
            .to_wire())
    }

    fn set_archive_excludes(
        package: &PackageInterfaceHandle,
        call: &mut Call<'_>,
    ) -> Result<PluginValue, PhpThrow> {
        let value = call.arg::<Vec<BString>>(0)?;
        package
            .as_rc()
            .borrow_mut()
            .as_complete_package_interface_mut()
            .expect(COMPLETE)
            .set_archive_excludes(value);
        Ok(PluginValue::Null)
    }

    // The alias target has to be a package that already lives on the Rust side; an alias of an
    // alias has no Rust representation, so its narrowing is an explicit error too.
    fn __construct(call: &mut Call<'_>) -> Result<PackageInterfaceHandle, PhpThrow> {
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
            CompleteAliasPackage::new(alias_of, call.arg(1)?, call.arg(2)?)
                .map_err(call.failed())?,
        )))
    }
}

// `RootAliasPackage` overrides the setters `Package` declares to write through to its alias
// target, which is what `RootPackageInterface`'s setters do on the Rust side.
impl forwarding::package::root_alias_package::Forwarding
    for forwarding::package::root_alias_package::PhpClass
{
    fn get_aliases(
        package: &PackageInterfaceHandle,
        _: &mut Call<'_>,
    ) -> Result<PluginValue, PhpThrow> {
        Ok(package
            .as_rc()
            .borrow()
            .as_root_package_interface()
            .expect(ROOT)
            .get_aliases()
            .to_wire())
    }

    fn get_minimum_stability(
        package: &PackageInterfaceHandle,
        _: &mut Call<'_>,
    ) -> Result<PluginValue, PhpThrow> {
        Ok(PluginValue::string(
            package
                .as_rc()
                .borrow()
                .as_root_package_interface()
                .expect(ROOT)
                .get_minimum_stability(),
        ))
    }

    fn get_stability_flags(
        package: &PackageInterfaceHandle,
        _: &mut Call<'_>,
    ) -> Result<PluginValue, PhpThrow> {
        Ok(package
            .as_rc()
            .borrow()
            .as_root_package_interface()
            .expect(ROOT)
            .get_stability_flags()
            .to_wire())
    }

    fn get_references(
        package: &PackageInterfaceHandle,
        _: &mut Call<'_>,
    ) -> Result<PluginValue, PhpThrow> {
        Ok(package
            .as_rc()
            .borrow()
            .as_root_package_interface()
            .expect(ROOT)
            .get_references()
            .to_wire())
    }

    fn get_prefer_stable(
        package: &PackageInterfaceHandle,
        _: &mut Call<'_>,
    ) -> Result<PluginValue, PhpThrow> {
        Ok(package
            .as_rc()
            .borrow()
            .as_root_package_interface()
            .expect(ROOT)
            .get_prefer_stable()
            .to_wire())
    }

    fn get_config(
        package: &PackageInterfaceHandle,
        _: &mut Call<'_>,
    ) -> Result<PluginValue, PhpThrow> {
        Ok(package
            .as_rc()
            .borrow()
            .as_root_package_interface()
            .expect(ROOT)
            .get_config()
            .to_wire())
    }

    fn set_stability_flags(
        package: &PackageInterfaceHandle,
        call: &mut Call<'_>,
    ) -> Result<PluginValue, PhpThrow> {
        let value = call.arg::<IndexMap<BString, i64>>(0)?;
        package
            .as_rc()
            .borrow_mut()
            .as_root_package_interface_mut()
            .expect(ROOT)
            .set_stability_flags(value);
        Ok(PluginValue::Null)
    }

    fn set_minimum_stability(
        package: &PackageInterfaceHandle,
        call: &mut Call<'_>,
    ) -> Result<PluginValue, PhpThrow> {
        let value = call.arg::<BString>(0)?;
        package
            .as_rc()
            .borrow_mut()
            .as_root_package_interface_mut()
            .expect(ROOT)
            .set_minimum_stability(value);
        Ok(PluginValue::Null)
    }

    fn set_prefer_stable(
        package: &PackageInterfaceHandle,
        call: &mut Call<'_>,
    ) -> Result<PluginValue, PhpThrow> {
        let value = call.arg::<bool>(0)?;
        package
            .as_rc()
            .borrow_mut()
            .as_root_package_interface_mut()
            .expect(ROOT)
            .set_prefer_stable(value);
        Ok(PluginValue::Null)
    }

    fn set_config(
        package: &PackageInterfaceHandle,
        call: &mut Call<'_>,
    ) -> Result<PluginValue, PhpThrow> {
        let value = call.arg::<IndexMap<BString, PhpMixed>>(0)?;
        package
            .as_rc()
            .borrow_mut()
            .as_root_package_interface_mut()
            .expect(ROOT)
            .set_config(value);
        Ok(PluginValue::Null)
    }

    fn set_references(
        package: &PackageInterfaceHandle,
        call: &mut Call<'_>,
    ) -> Result<PluginValue, PhpThrow> {
        let value = call.arg::<IndexMap<BString, BString>>(0)?;
        package
            .as_rc()
            .borrow_mut()
            .as_root_package_interface_mut()
            .expect(ROOT)
            .set_references(value);
        Ok(PluginValue::Null)
    }

    fn set_aliases(
        package: &PackageInterfaceHandle,
        call: &mut Call<'_>,
    ) -> Result<PluginValue, PhpThrow> {
        let value = call.arg::<Vec<IndexMap<BString, BString>>>(0)?;
        package
            .as_rc()
            .borrow_mut()
            .as_root_package_interface_mut()
            .expect(ROOT)
            .set_aliases(value);
        Ok(PluginValue::Null)
    }

    fn set_requires(
        package: &PackageInterfaceHandle,
        call: &mut Call<'_>,
    ) -> Result<PluginValue, PhpThrow> {
        package
            .as_rc()
            .borrow_mut()
            .as_root_package_interface_mut()
            .expect(ROOT)
            .set_requires(links(call)?)
            .map_err(call.failed())?;
        Ok(PluginValue::Null)
    }

    fn set_dev_requires(
        package: &PackageInterfaceHandle,
        call: &mut Call<'_>,
    ) -> Result<PluginValue, PhpThrow> {
        package
            .as_rc()
            .borrow_mut()
            .as_root_package_interface_mut()
            .expect(ROOT)
            .set_dev_requires(links(call)?)
            .map_err(call.failed())?;
        Ok(PluginValue::Null)
    }

    fn set_conflicts(
        package: &PackageInterfaceHandle,
        call: &mut Call<'_>,
    ) -> Result<PluginValue, PhpThrow> {
        package
            .as_rc()
            .borrow_mut()
            .as_root_package_interface_mut()
            .expect(ROOT)
            .set_conflicts(links(call)?)
            .map_err(call.failed())?;
        Ok(PluginValue::Null)
    }

    fn set_provides(
        package: &PackageInterfaceHandle,
        call: &mut Call<'_>,
    ) -> Result<PluginValue, PhpThrow> {
        package
            .as_rc()
            .borrow_mut()
            .as_root_package_interface_mut()
            .expect(ROOT)
            .set_provides(links(call)?)
            .map_err(call.failed())?;
        Ok(PluginValue::Null)
    }

    fn set_replaces(
        package: &PackageInterfaceHandle,
        call: &mut Call<'_>,
    ) -> Result<PluginValue, PhpThrow> {
        package
            .as_rc()
            .borrow_mut()
            .as_root_package_interface_mut()
            .expect(ROOT)
            .set_replaces(links(call)?)
            .map_err(call.failed())?;
        Ok(PluginValue::Null)
    }

    fn set_autoload(
        package: &PackageInterfaceHandle,
        call: &mut Call<'_>,
    ) -> Result<PluginValue, PhpThrow> {
        let value = call.arg::<IndexMap<BString, PhpMixed>>(0)?;
        package
            .as_rc()
            .borrow_mut()
            .as_root_package_interface_mut()
            .expect(ROOT)
            .set_autoload(value);
        Ok(PluginValue::Null)
    }

    fn set_dev_autoload(
        package: &PackageInterfaceHandle,
        call: &mut Call<'_>,
    ) -> Result<PluginValue, PhpThrow> {
        let value = call.arg::<IndexMap<BString, PhpMixed>>(0)?;
        package
            .as_rc()
            .borrow_mut()
            .as_root_package_interface_mut()
            .expect(ROOT)
            .set_dev_autoload(value);
        Ok(PluginValue::Null)
    }

    fn set_suggests(
        package: &PackageInterfaceHandle,
        call: &mut Call<'_>,
    ) -> Result<PluginValue, PhpThrow> {
        let value = call.arg::<IndexMap<BString, BString>>(0)?;
        package
            .as_rc()
            .borrow_mut()
            .as_root_package_interface_mut()
            .expect(ROOT)
            .set_suggests(value);
        Ok(PluginValue::Null)
    }

    fn set_extra(
        package: &PackageInterfaceHandle,
        call: &mut Call<'_>,
    ) -> Result<PluginValue, PhpThrow> {
        let value = call.arg::<IndexMap<BString, PhpMixed>>(0)?;
        package
            .as_rc()
            .borrow_mut()
            .as_root_package_interface_mut()
            .expect(ROOT)
            .set_extra(value);
        Ok(PluginValue::Null)
    }

    // The alias target has to be a package that already lives on the Rust side; an alias of an
    // alias has no Rust representation, so its narrowing is an explicit error too.
    fn __construct(call: &mut Call<'_>) -> Result<PackageInterfaceHandle, PhpThrow> {
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
}
