//! The methods a `Composer\Installer\InstallationManager` entity answers over RPC.

use crate::installer::InstallationManagerInterface;
use crate::package::PackageInterfaceHandle;
use crate::php_worker::proxy::{forget_php_installer_proxy, php_installer_proxy};
use crate::php_worker::rpc::entity::{Call, Crossing, Entity};
use crate::php_worker::rpc::forwarding;
use crate::php_worker::runtime;
use bstr::BStr;
use shirabe_php_rpc::{PhpObjHandle, PhpThrow, PluginValue, ToWire};
use shirabe_php_shim::{bformat, error_chain_message};

impl Entity for std::rc::Rc<std::cell::RefCell<dyn InstallationManagerInterface>> {
    const EXPECTED: &'static [u8] = b"an InstallationManager";

    fn address(&self) -> *const () {
        std::rc::Rc::as_ptr(self) as *const ()
    }

    fn class(&self) -> Result<&'static Crossing<Self>, PhpThrow> {
        Ok(&forwarding::installer::installation_manager::CROSSING)
    }
}

/// The installer argument of `addInstaller()` / `removeInstaller()`, type-checked the way the PHP
/// signature does.
fn installer_arg(call: &Call<'_>) -> Result<PhpObjHandle, PhpThrow> {
    let handle = call.arg::<PhpObjHandle>(0)?;
    let is_installer = runtime::is_a(
        &handle,
        BStr::new("Composer\\Installer\\InstallerInterface"),
    )
    .map_err(|error| {
        PhpThrow::runtime(bformat!(
            "{} could not type-check its argument: {}",
            call.method(),
            error_chain_message(&error)
        ))
    })?;
    if !is_installer {
        return Err(PhpThrow::runtime(bformat!(
            "{} expects a Composer\\Installer\\InstallerInterface, got {}",
            call.method(),
            handle.class()
        )));
    }
    Ok(handle)
}

// TODO(plugin): the remaining InstallationManager surface is widened on demand.
impl forwarding::installer::installation_manager::Forwarding
    for forwarding::installer::installation_manager::PhpClass
{
    type Handle = std::rc::Rc<std::cell::RefCell<dyn InstallationManagerInterface>>;

    fn get_install_path(im: &Self::Handle, call: &mut Call<'_>) -> Result<PluginValue, PhpThrow> {
        let package = call.arg::<PackageInterfaceHandle>(0)?;
        Ok(im.borrow().get_install_path(package).to_wire())
    }

    fn add_installer(im: &Self::Handle, call: &mut Call<'_>) -> Result<PluginValue, PhpThrow> {
        let handle = installer_arg(call)?;
        im.borrow().add_installer(php_installer_proxy(handle));
        Ok(PluginValue::Null)
    }

    fn remove_installer(im: &Self::Handle, call: &mut Call<'_>) -> Result<PluginValue, PhpThrow> {
        let handle = installer_arg(call)?;
        let installer = php_installer_proxy(handle.clone());
        im.borrow().remove_installer(&*installer);
        forget_php_installer_proxy(&handle);
        Ok(PluginValue::Null)
    }
}
