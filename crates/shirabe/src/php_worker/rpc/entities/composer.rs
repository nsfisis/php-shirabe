//! The methods a `Composer\Composer` entity answers over RPC.

use crate::composer::{
    __php_binding_composer, __php_binding_partial_composer, Composer, ComposerHandle,
    PartialComposer,
};
use crate::php_worker::rpc::entity::{Call, entity_value};
use shirabe_php_binding_macros::php_entity;
use shirabe_php_rpc::{PhpThrow, RpcValue};

// TODO(plugin): the setters, `isGlobal`/`setGlobal` and the remaining getters of the object
// graph become reachable over RPC on demand.
php_entity! {
    handle: ComposerHandle,
    expected: "a Composer",
    address: |composer| std::rc::Rc::as_ptr(composer.as_rc()) as *const (),
    class: |_| Ok(br"Composer\Composer"),
    classes: [r"Composer\PartialComposer", r"Composer\Composer"],
    worker_method: loop_services,
    unimplemented: [
        r"Composer\Composer::__construct",
        r"Composer\Composer::getArchiveManager",
        r"Composer\Composer::getPluginManager",
        r"Composer\Composer::setArchiveManager",
        r"Composer\Composer::setAutoloadGenerator",
        r"Composer\Composer::setDownloadManager",
        r"Composer\Composer::setLocker",
        r"Composer\Composer::setPluginManager",
        r"Composer\PartialComposer::__construct",
        r"Composer\PartialComposer::isGlobal",
        r"Composer\PartialComposer::setConfig",
        r"Composer\PartialComposer::setEventDispatcher",
        r"Composer\PartialComposer::setGlobal",
        r"Composer\PartialComposer::setInstallationManager",
        r"Composer\PartialComposer::setLoop",
        r"Composer\PartialComposer::setPackage",
        r"Composer\PartialComposer::setRepositoryManager",
    ],
    types: {
        r"Composer\PartialComposer": instance __php_binding_partial_composer for PartialComposer {
            via: |composer| std::cell::Ref::map(composer.borrow(), Composer::as_partial),
        },
        r"Composer\Composer": instance __php_binding_composer for Composer {
            via: |composer| composer.borrow(),
        },
    },
}

// `getLoop()` itself never reaches the Rust side: the Loop is a two-world class, and the stub
// builds the worker-side one over the two services the Rust-side Loop drives.
fn loop_services(
    composer: &ComposerHandle,
    call: &mut Call<'_>,
) -> Option<Result<RpcValue, PhpThrow>> {
    if call.method() != "__shirabe_loop_services" {
        return None;
    }
    let r#loop = composer.borrow().get_loop();
    let downloader = r#loop.borrow().get_http_downloader().clone();
    let process = r#loop.borrow().get_process_executor().cloned();
    Some((|| {
        Ok(RpcValue::List(vec![
            entity_value(&downloader)?,
            match process {
                Some(process) => entity_value(&process)?,
                None => RpcValue::Null,
            },
        ]))
    })())
}
