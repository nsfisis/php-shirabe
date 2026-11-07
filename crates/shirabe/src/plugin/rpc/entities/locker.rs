//! The methods a `Composer\Package\Locker` entity answers over RPC.

use crate::package::{LockerInterface, PackageInterfaceHandle};
use crate::plugin::rpc::codec::{arg, arg_or};
use crate::plugin::rpc::dispatch::{error_throw, unavailable_method};
use crate::plugin::rpc::entities::autoload_generator::root_package_arg;
use crate::plugin::rpc::table::repository_handle_value;
use bstr::{BStr, BString, ByteSlice};
use indexmap::IndexMap;
use shirabe_php_rpc::{PhpThrow, PluginValue, ToWire};
use shirabe_php_shim::{PhpMixed, bformat};

pub(crate) fn dispatch_locker_method(
    locker: &std::rc::Rc<std::cell::RefCell<dyn LockerInterface>>,
    method_name: &BStr,
    args: &[PluginValue],
) -> Result<PluginValue, PhpThrow> {
    let failed =
        |error: anyhow::Error| error_throw(bformat!("{method_name} failed over RPC"), &error);
    let with_dev_reqs = || arg_or::<bool>(method_name, args, 0, false);
    match method_name.as_bytes() {
        b"isLocked" => Ok(locker.borrow_mut().is_locked().to_wire()),
        b"isFresh" => Ok(locker.borrow_mut().is_fresh().map_err(failed)?.to_wire()),
        b"getLockedRepository" => {
            let repository = locker
                .borrow_mut()
                .get_locked_repository(with_dev_reqs()?)
                .map_err(failed)?;
            repository_handle_value(&repository.into())
        }
        b"getDevPackageNames" => Ok(locker
            .borrow_mut()
            .get_dev_package_names()
            .map_err(failed)?
            .to_wire()),
        b"getPlatformRequirements" => Ok(locker
            .borrow_mut()
            .get_platform_requirements(with_dev_reqs()?)
            .map_err(failed)?
            .to_wire()),
        b"getMinimumStability" => Ok(locker
            .borrow_mut()
            .get_minimum_stability()
            .map_err(failed)?
            .to_wire()),
        b"getStabilityFlags" => Ok(locker
            .borrow_mut()
            .get_stability_flags()
            .map_err(failed)?
            .to_wire()),
        b"getPreferStable" => Ok(locker
            .borrow_mut()
            .get_prefer_stable()
            .map_err(failed)?
            .to_wire()),
        b"getPreferLowest" => Ok(locker
            .borrow_mut()
            .get_prefer_lowest()
            .map_err(failed)?
            .to_wire()),
        b"getPlatformOverrides" => Ok(locker
            .borrow_mut()
            .get_platform_overrides()
            .map_err(failed)?
            .to_wire()),
        b"getAliases" => Ok(locker.borrow_mut().get_aliases().map_err(failed)?.to_wire()),
        b"getPluginApi" => Ok(locker
            .borrow_mut()
            .get_plugin_api()
            .map_err(failed)?
            .to_wire()),
        b"getLockData" => Ok(locker
            .borrow_mut()
            .get_lock_data()
            .map_err(failed)?
            .to_wire()),
        b"setLockData" => {
            let written = locker
                .borrow_mut()
                .set_lock_data(
                    arg::<Vec<PackageInterfaceHandle>>(method_name, args, 0)?,
                    arg::<Option<Vec<PackageInterfaceHandle>>>(method_name, args, 1)?,
                    arg::<IndexMap<BString, BString>>(method_name, args, 2)?,
                    arg::<IndexMap<BString, BString>>(method_name, args, 3)?,
                    arg::<Vec<IndexMap<BString, PhpMixed>>>(method_name, args, 4)?,
                    arg::<BString>(method_name, args, 5)?.as_ref(),
                    arg::<IndexMap<BString, i64>>(method_name, args, 6)?,
                    arg::<bool>(method_name, args, 7)?,
                    arg::<bool>(method_name, args, 8)?,
                    arg::<IndexMap<BString, PhpMixed>>(method_name, args, 9)?,
                    arg_or::<bool>(method_name, args, 10, true)?,
                )
                .map_err(failed)?;
            Ok(written.to_wire())
        }
        b"getMissingRequirementInfo" => {
            let package = root_package_arg(method_name, args, 0)?;
            let include_dev = arg::<bool>(method_name, args, 1)?;
            Ok(locker
                .borrow_mut()
                .get_missing_requirement_info(package, include_dev)
                .map_err(failed)?
                .to_wire())
        }
        // TODO(plugin): `JsonFile` has no proxy stub, so the lock file cannot be handed out and
        // no plugin holds a JsonFile it could pass to `updateHash()`.
        b"getJsonFile" | b"updateHash" => Err(PhpThrow::runtime(bformat!(
            "the Locker method `{method_name}` is not available over RPC yet: JsonFile cannot cross the boundary"
        ))),
        other => Err(unavailable_method("Locker", other)),
    }
}
