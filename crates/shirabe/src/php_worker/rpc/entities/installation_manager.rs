//! The methods a `Composer\Installer\InstallationManager` entity answers over RPC.

use crate::installer::{
    __php_binding_installation_manager_interface, InstallationManagerInterface,
};
use crate::php_worker::proxy::{forget_php_installer_proxy, php_installer_proxy};
use crate::php_worker::rpc::dispatch::error_throw;
use crate::php_worker::rpc::entity::Call;
use crate::php_worker::runtime;
use bstr::BStr;
use shirabe_php_binding_macros::php_entity;
use shirabe_php_rpc::{PhpEntityHandle, PhpThrow, RpcValue};
use shirabe_php_shim::bformat;

// TODO(plugin): the remaining InstallationManager surface is widened on demand.
php_entity! {
    handle: std::rc::Rc<dyn InstallationManagerInterface>,
    type_error: "an InstallationManager",
    classes: {
        r"Composer\Installer\InstallationManager": { constructor: unimplemented },
    },
    methods: {
        r"Composer\Installer\InstallationManager": {
            handwritten: { module: self, members: [addInstaller, removeInstaller] },
            instance: {
                adapters: __php_binding_installation_manager_interface,
                target: dyn InstallationManagerInterface,
                deref: |im| &**im,
            },
            unimplemented: [
                disablePlugins,
                download,
                ensureBinariesPresence,
                execute,
                getInstaller,
                install,
                isPackageInstalled,
                markAliasInstalled,
                markAliasUninstalled,
                notifyInstalls,
                reset,
                setOutputProgress,
                uninstall,
                update,
            ],
        },
    },
}

/// The installer argument of `addInstaller()` / `removeInstaller()`, type-checked the way the PHP
/// signature does.
fn installer_arg(call: &Call<'_>) -> Result<PhpEntityHandle, PhpThrow> {
    let handle = call.arg::<PhpEntityHandle>(0)?;
    let is_installer = runtime::is_a(
        &handle,
        BStr::new("Composer\\Installer\\InstallerInterface"),
    )
    .map_err(|error| {
        error_throw(
            bformat!("{} could not type-check its argument", call.method()),
            &error,
        )
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

fn add_installer(
    im: &std::rc::Rc<dyn InstallationManagerInterface>,
    call: &mut Call<'_>,
) -> Result<RpcValue, PhpThrow> {
    let handle = installer_arg(call)?;
    im.add_installer(php_installer_proxy(handle));
    Ok(RpcValue::Null)
}

fn remove_installer(
    im: &std::rc::Rc<dyn InstallationManagerInterface>,
    call: &mut Call<'_>,
) -> Result<RpcValue, PhpThrow> {
    let handle = installer_arg(call)?;
    let installer = php_installer_proxy(handle.clone());
    im.remove_installer(&*installer);
    forget_php_installer_proxy(&handle);
    Ok(RpcValue::Null)
}
