//! The methods a `Composer\Composer` entity answers over RPC.

use crate::composer::ComposerHandle;
use crate::php_worker::rpc::entity::{Call, Crossing, Entity, entity_value};
use crate::php_worker::rpc::forwarding;
use crate::php_worker::rpc::table::package_handle_value;
use shirabe_php_rpc::{PhpThrow, PluginValue};

impl Entity for ComposerHandle {
    const EXPECTED: &'static [u8] = b"a Composer";

    fn address(&self) -> *const () {
        std::rc::Rc::as_ptr(self.as_rc()) as *const ()
    }

    fn class(&self) -> Result<&'static Crossing<Self>, PhpThrow> {
        Ok(&forwarding::composer::CROSSING)
    }

    // `getLoop()` itself never reaches the Rust side: the Loop is a two-world class, and the stub
    // builds the worker-side one over the two services the Rust-side Loop drives.
    fn worker_method(&self, call: &mut Call<'_>) -> Option<Result<PluginValue, PhpThrow>> {
        if call.method() != "__shirabe_loop_services" {
            return None;
        }
        let r#loop = self.borrow().get_loop();
        let downloader = r#loop.borrow().get_http_downloader().clone();
        let process = r#loop.borrow().get_process_executor().cloned();
        Some((|| {
            Ok(PluginValue::List(vec![
                entity_value(&downloader)?,
                match process {
                    Some(process) => entity_value(&process)?,
                    None => PluginValue::Null,
                },
            ]))
        })())
    }
}

// TODO(plugin): the setters, `isGlobal`/`setGlobal` and the remaining getters of the object
// graph become reachable over RPC on demand.
impl forwarding::partial_composer::Forwarding for forwarding::partial_composer::PhpClass {
    type Handle = ComposerHandle;

    fn get_package(composer: &Self::Handle, _: &mut Call<'_>) -> Result<PluginValue, PhpThrow> {
        let package = composer.borrow().get_package().as_rc().clone();
        Ok(package_handle_value(&package))
    }

    fn get_config(composer: &Self::Handle, _: &mut Call<'_>) -> Result<PluginValue, PhpThrow> {
        entity_value(&composer.borrow().get_config())
    }

    fn get_repository_manager(
        composer: &Self::Handle,
        _: &mut Call<'_>,
    ) -> Result<PluginValue, PhpThrow> {
        entity_value(&composer.borrow().get_repository_manager())
    }

    fn get_installation_manager(
        composer: &Self::Handle,
        _: &mut Call<'_>,
    ) -> Result<PluginValue, PhpThrow> {
        entity_value(&composer.borrow().get_installation_manager())
    }

    fn get_event_dispatcher(
        composer: &Self::Handle,
        _: &mut Call<'_>,
    ) -> Result<PluginValue, PhpThrow> {
        entity_value(&composer.borrow().get_event_dispatcher())
    }
}

impl forwarding::composer::Forwarding for forwarding::composer::PhpClass {
    fn get_locker(composer: &ComposerHandle, _: &mut Call<'_>) -> Result<PluginValue, PhpThrow> {
        entity_value(&composer.borrow().get_locker())
    }

    fn get_download_manager(
        composer: &ComposerHandle,
        _: &mut Call<'_>,
    ) -> Result<PluginValue, PhpThrow> {
        entity_value(&composer.borrow().get_download_manager())
    }

    fn get_autoload_generator(
        composer: &ComposerHandle,
        _: &mut Call<'_>,
    ) -> Result<PluginValue, PhpThrow> {
        entity_value(&composer.borrow().get_autoload_generator())
    }
}
