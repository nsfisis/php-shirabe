//! The methods a `Composer\Installer\InstallationManager` entity answers over RPC.

use crate::installer::InstallationManagerInterface;
use crate::package::PackageInterfaceHandle;
use crate::plugin::proxy::{forget_php_installer_proxy, php_installer_proxy, php_is_a};
use crate::plugin::rpc::codec::arg;
use crate::plugin::rpc::dispatch::unavailable_method;
use bstr::{BStr, ByteSlice};
use shirabe_php_rpc::{PhpObjHandle, PhpThrow, PluginValue, ToWire};
use shirabe_php_shim::{bformat, error_chain_message};

pub(crate) fn dispatch_installation_manager_method(
    im: &std::rc::Rc<std::cell::RefCell<dyn InstallationManagerInterface>>,
    method_name: &BStr,
    args: &[PluginValue],
) -> Result<PluginValue, PhpThrow> {
    match method_name.as_bytes() {
        b"getInstallPath" => {
            let package = arg::<PackageInterfaceHandle>(method_name, args, 0)?;
            Ok(im.borrow().get_install_path(package).to_wire())
        }
        b"addInstaller" | b"removeInstaller" => {
            let handle = arg::<PhpObjHandle>(method_name, args, 0)?;
            if !php_is_a(
                &handle,
                BStr::new("Composer\\Installer\\InstallerInterface"),
            )
            .map_err(|error| {
                PhpThrow::runtime(bformat!(
                    "{method_name} could not type-check its argument: {}",
                    error_chain_message(&error)
                ))
            })? {
                return Err(PhpThrow::runtime(bformat!(
                    "{method_name} expects a Composer\\Installer\\InstallerInterface, got {}",
                    handle.class()
                )));
            }
            let installer = php_installer_proxy(handle.clone());
            match method_name.as_bytes() {
                b"addInstaller" => im.borrow().add_installer(installer),
                b"removeInstaller" => {
                    im.borrow().remove_installer(&*installer);
                    forget_php_installer_proxy(&handle);
                }
                _ => unreachable!(),
            }
            Ok(PluginValue::Null)
        }
        // TODO(plugin): the remaining InstallationManager surface is widened on demand,
        // driven by explicit errors from real plugins.
        other => Err(unavailable_method("InstallationManager", other)),
    }
}
