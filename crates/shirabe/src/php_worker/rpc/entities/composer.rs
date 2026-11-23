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
    type_error: "a Composer",
    identity: |composer| std::rc::Rc::as_ptr(composer.as_rc()) as *const (),
    classes: {
        r"Composer\PartialComposer": { ancestor: true, constructor: unimplemented },
        r"Composer\Composer": { constructor: unimplemented },
    },
    methods: {
        r"Composer\PartialComposer": {
            instance: {
                adapters: __php_binding_partial_composer,
                target: PartialComposer,
                deref: |composer| composer.as_partial(),
            },
            unimplemented: [
                isGlobal,
                setConfig,
                setEventDispatcher,
                setGlobal,
                setInstallationManager,
                setLoop,
                setPackage,
                setRepositoryManager,
            ],
        },
        r"Composer\Composer": {
            instance: {
                adapters: __php_binding_composer,
                target: Composer,
                deref: |composer| &**composer,
            },
            unimplemented: [
                getArchiveManager,
                getPluginManager,
                setArchiveManager,
                setAutoloadGenerator,
                setDownloadManager,
                setLocker,
                setPluginManager,
            ],
        },
    },
    handlers: { worker_method: loop_services },
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
    let r#loop = composer.get_loop();
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
