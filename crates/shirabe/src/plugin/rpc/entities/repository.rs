//! The methods a `Composer\Repository\RepositoryInterface` entity answers over RPC.

use crate::package::PackageInterfaceHandle;
use crate::plugin::rpc::codec::arg;
use crate::plugin::rpc::dispatch::{error_throw, unavailable_method};
use crate::plugin::rpc::table::package_handle_value;
use crate::repository::RepositoryInterfaceHandle;
use bstr::{BStr, ByteSlice};
use shirabe_php_rpc::{PhpThrow, PluginValue, ToWire};
use shirabe_php_shim::bformat;

pub(crate) fn dispatch_repository_method(
    repository: &RepositoryInterfaceHandle,
    method_name: &BStr,
    args: &[PluginValue],
) -> Result<PluginValue, PhpThrow> {
    match method_name.as_bytes() {
        b"hasPackage" => {
            let package = arg::<PackageInterfaceHandle>(method_name, args, 0)?;
            let has = repository
                .has_package(package)
                .map_err(|error| error_throw(b"hasPackage failed", &error))?;
            Ok(has.to_wire())
        }
        b"addPackage" | b"removePackage" => {
            let package = arg::<PackageInterfaceHandle>(method_name, args, 0)?;
            let mut borrowed = repository.borrow_mut();
            let writable = borrowed
                .as_writable_repository_interface_mut()
                .ok_or_else(|| {
                    PhpThrow::runtime(bformat!("the repository behind this handle is not writable, so `{method_name}` cannot be called on it"))
                })?;
            let outcome = match method_name.as_bytes() {
                b"addPackage" => writable.add_package(package),
                b"removePackage" => writable.remove_package(package),
                _ => unreachable!(),
            };
            outcome
                .map_err(|error| error_throw(bformat!("{method_name} failed over RPC"), &error))?;
            Ok(PluginValue::Null)
        }
        b"getPackages" => {
            let packages = repository
                .borrow_mut()
                .get_packages()
                .map_err(|error| error_throw(b"getPackages failed", &error))?;
            let mut items = Vec::with_capacity(packages.len());
            for package in packages {
                items.push(package_handle_value(package.as_rc()));
            }
            Ok(PluginValue::List(items))
        }
        b"getCanonicalPackages" => {
            let mut borrowed = repository.borrow_mut();
            let writable = borrowed
                .as_writable_repository_interface_mut()
                .ok_or_else(|| {
                    PhpThrow::runtime("the repository behind this handle is not writable, so `getCanonicalPackages` cannot be called on it")
                })?;
            let packages = writable
                .get_canonical_packages()
                .map_err(|error| error_throw(b"getCanonicalPackages failed", &error))?;
            Ok(packages.to_wire())
        }
        // TODO(plugin): the remaining RepositoryInterface surface is widened on demand,
        // driven by explicit errors from real plugins.
        other => Err(unavailable_method("repository", other)),
    }
}
