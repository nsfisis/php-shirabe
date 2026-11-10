//! The methods a `Composer\Package\Locker` entity answers over RPC.

use crate::package::{Locker, LockerInterface, PackageInterfaceHandle};
use crate::php_worker::rpc::entities::autoload_generator::root_package;
use crate::php_worker::rpc::entity::{Call, Crossing, Entity, entity_value};
use crate::php_worker::rpc::forwarding;
use crate::repository::RepositoryInterfaceHandle;
use bstr::BString;
use indexmap::IndexMap;
use shirabe_php_rpc::{PhpThrow, PluginValue, ToPluginValue};
use shirabe_php_shim::PhpMixed;

impl Entity for std::rc::Rc<std::cell::RefCell<dyn LockerInterface>> {
    const EXPECTED: &'static [u8] = b"a Locker";

    fn address(&self) -> *const () {
        std::rc::Rc::as_ptr(self) as *const ()
    }

    fn class(&self) -> Result<&'static Crossing<Self>, PhpThrow> {
        Ok(&forwarding::package::locker::CROSSING)
    }
}

impl forwarding::package::locker::Forwarding for forwarding::package::locker::PhpClass {
    type Handle = std::rc::Rc<std::cell::RefCell<dyn LockerInterface>>;

    fn is_locked(locker: &Self::Handle, _: &mut Call<'_>) -> Result<PluginValue, PhpThrow> {
        Ok(locker.borrow_mut().is_locked().to_plugin_value())
    }

    fn is_fresh(locker: &Self::Handle, call: &mut Call<'_>) -> Result<PluginValue, PhpThrow> {
        Ok(locker
            .borrow_mut()
            .is_fresh()
            .map_err(call.failed())?
            .to_plugin_value())
    }

    fn get_locked_repository(
        locker: &Self::Handle,
        call: &mut Call<'_>,
    ) -> Result<PluginValue, PhpThrow> {
        let repository = locker
            .borrow_mut()
            .get_locked_repository(call.arg_or(0, false)?)
            .map_err(call.failed())?;
        entity_value(&RepositoryInterfaceHandle::from(repository))
    }

    fn get_dev_package_names(
        locker: &Self::Handle,
        call: &mut Call<'_>,
    ) -> Result<PluginValue, PhpThrow> {
        Ok(locker
            .borrow_mut()
            .get_dev_package_names()
            .map_err(call.failed())?
            .to_plugin_value())
    }

    fn get_platform_requirements(
        locker: &Self::Handle,
        call: &mut Call<'_>,
    ) -> Result<PluginValue, PhpThrow> {
        Ok(locker
            .borrow_mut()
            .get_platform_requirements(call.arg_or(0, false)?)
            .map_err(call.failed())?
            .to_plugin_value())
    }

    fn get_minimum_stability(
        locker: &Self::Handle,
        call: &mut Call<'_>,
    ) -> Result<PluginValue, PhpThrow> {
        Ok(locker
            .borrow_mut()
            .get_minimum_stability()
            .map_err(call.failed())?
            .to_plugin_value())
    }

    fn get_stability_flags(
        locker: &Self::Handle,
        call: &mut Call<'_>,
    ) -> Result<PluginValue, PhpThrow> {
        Ok(locker
            .borrow_mut()
            .get_stability_flags()
            .map_err(call.failed())?
            .to_plugin_value())
    }

    fn get_prefer_stable(
        locker: &Self::Handle,
        call: &mut Call<'_>,
    ) -> Result<PluginValue, PhpThrow> {
        Ok(locker
            .borrow_mut()
            .get_prefer_stable()
            .map_err(call.failed())?
            .to_plugin_value())
    }

    fn get_prefer_lowest(
        locker: &Self::Handle,
        call: &mut Call<'_>,
    ) -> Result<PluginValue, PhpThrow> {
        Ok(locker
            .borrow_mut()
            .get_prefer_lowest()
            .map_err(call.failed())?
            .to_plugin_value())
    }

    fn get_platform_overrides(
        locker: &Self::Handle,
        call: &mut Call<'_>,
    ) -> Result<PluginValue, PhpThrow> {
        Ok(locker
            .borrow_mut()
            .get_platform_overrides()
            .map_err(call.failed())?
            .to_plugin_value())
    }

    fn get_aliases(locker: &Self::Handle, call: &mut Call<'_>) -> Result<PluginValue, PhpThrow> {
        Ok(locker
            .borrow_mut()
            .get_aliases()
            .map_err(call.failed())?
            .to_plugin_value())
    }

    fn get_plugin_api(locker: &Self::Handle, call: &mut Call<'_>) -> Result<PluginValue, PhpThrow> {
        Ok(locker
            .borrow_mut()
            .get_plugin_api()
            .map_err(call.failed())?
            .to_plugin_value())
    }

    fn get_lock_data(locker: &Self::Handle, call: &mut Call<'_>) -> Result<PluginValue, PhpThrow> {
        Ok(locker
            .borrow_mut()
            .get_lock_data()
            .map_err(call.failed())?
            .to_plugin_value())
    }

    fn set_lock_data(locker: &Self::Handle, call: &mut Call<'_>) -> Result<PluginValue, PhpThrow> {
        let written = locker
            .borrow_mut()
            .set_lock_data(
                call.arg::<Vec<PackageInterfaceHandle>>(0)?,
                call.arg::<Option<Vec<PackageInterfaceHandle>>>(1)?,
                call.arg::<IndexMap<BString, BString>>(2)?,
                call.arg::<IndexMap<BString, BString>>(3)?,
                call.arg::<Vec<IndexMap<BString, PhpMixed>>>(4)?,
                call.arg::<BString>(5)?.as_ref(),
                call.arg::<IndexMap<BString, i64>>(6)?,
                call.arg::<bool>(7)?,
                call.arg::<bool>(8)?,
                call.arg::<IndexMap<BString, PhpMixed>>(9)?,
                call.arg_or::<bool>(10, true)?,
            )
            .map_err(call.failed())?;
        Ok(written.to_plugin_value())
    }

    fn get_missing_requirement_info(
        locker: &Self::Handle,
        call: &mut Call<'_>,
    ) -> Result<PluginValue, PhpThrow> {
        let package = root_package(call, 0)?;
        let include_dev = call.arg::<bool>(1)?;
        Ok(locker
            .borrow_mut()
            .get_missing_requirement_info(package, include_dev)
            .map_err(call.failed())?
            .to_plugin_value())
    }

    fn get_content_hash(call: &mut Call<'_>) -> Result<PluginValue, PhpThrow> {
        Locker::get_content_hash(call.arg::<BString>(0)?)
            .map(PluginValue::string)
            .map_err(call.failed())
    }

    // TODO(plugin): `JsonFile` has no proxy stub, so the lock file cannot be handed out.
    fn get_json_file(_: &Self::Handle, call: &mut Call<'_>) -> Result<PluginValue, PhpThrow> {
        Err(call.unavailable("JsonFile cannot cross the boundary"))
    }

    // TODO(plugin): `JsonFile` has no proxy stub, so no plugin holds a JsonFile it could pass.
    fn update_hash(_: &Self::Handle, call: &mut Call<'_>) -> Result<PluginValue, PhpThrow> {
        Err(call.unavailable("JsonFile cannot cross the boundary"))
    }
}
