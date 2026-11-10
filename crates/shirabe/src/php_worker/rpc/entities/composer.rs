//! The methods a `Composer\Composer` entity answers over RPC.

use crate::composer::ComposerHandle;
use crate::php_worker::rpc::dispatch::unavailable_method;
use crate::php_worker::rpc::entity::entity_value;
use crate::php_worker::rpc::table::{
    RustEntity, config_handle_value, package_handle_value, register_entity, rust_handle_value,
};
use bstr::{BStr, ByteSlice};
use shirabe_php_rpc::{PhpThrow, PluginValue};

pub(crate) fn dispatch_composer_method(
    composer: &ComposerHandle,
    method_name: &BStr,
) -> Result<PluginValue, PhpThrow> {
    match method_name.as_bytes() {
        b"getRepositoryManager" => {
            let rm = composer.borrow().get_repository_manager();
            let rhandle = register_entity(RustEntity::RepositoryManager(rm));
            Ok(rust_handle_value(
                rhandle,
                "Composer\\Repository\\RepositoryManager",
            ))
        }
        b"getInstallationManager" => {
            let im = composer.borrow().get_installation_manager();
            let rhandle = register_entity(RustEntity::InstallationManager(im));
            Ok(rust_handle_value(
                rhandle,
                "Composer\\Installer\\InstallationManager",
            ))
        }
        b"getPackage" => {
            let package = composer.borrow().get_package().as_rc().clone();
            Ok(package_handle_value(&package))
        }
        b"getEventDispatcher" => {
            let dispatcher = composer.borrow().get_event_dispatcher();
            let rhandle = register_entity(RustEntity::EventDispatcher(dispatcher));
            Ok(rust_handle_value(
                rhandle,
                "Composer\\EventDispatcher\\EventDispatcher",
            ))
        }
        b"getConfig" => Ok(config_handle_value(&composer.borrow().get_config())),
        b"getAutoloadGenerator" => {
            let generator = composer.borrow().get_autoload_generator();
            let rhandle = register_entity(RustEntity::AutoloadGenerator(generator));
            Ok(rust_handle_value(
                rhandle,
                "Composer\\Autoload\\AutoloadGenerator",
            ))
        }
        b"getDownloadManager" => {
            let dm = composer.borrow().get_download_manager();
            let rhandle = register_entity(RustEntity::DownloadManager(dm));
            Ok(rust_handle_value(
                rhandle,
                "Composer\\Downloader\\DownloadManager",
            ))
        }
        // `getLoop()` itself never reaches here: the Loop is a two-world class, and the stub builds
        // the worker-side one over the two services the Rust-side Loop drives.
        b"__shirabe_loop_services" => {
            let r#loop = composer.borrow().get_loop();
            let downloader = r#loop.borrow().get_http_downloader().clone();
            let process = r#loop.borrow().get_process_executor().cloned();
            Ok(PluginValue::List(vec![
                rust_handle_value(
                    register_entity(RustEntity::HttpDownloader(downloader)),
                    "Composer\\Util\\HttpDownloader",
                ),
                match process {
                    Some(process) => rust_handle_value(
                        register_entity(RustEntity::ProcessExecutor(process)),
                        "Composer\\Util\\ProcessExecutor",
                    ),
                    None => PluginValue::Null,
                },
            ]))
        }
        b"getLocker" => entity_value(&composer.borrow().get_locker()),
        // TODO(plugin): the remaining Composer object graph (getPluginManager, ...)
        // becomes reachable over RPC on demand, driven by explicit errors from real plugins.
        other => Err(unavailable_method("Composer", other)),
    }
}
