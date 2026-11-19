//! The methods a `Composer\Util\Filesystem` entity answers over RPC.

use crate::util::{__php_binding_filesystem, Filesystem};
use shirabe_php_binding_macros::php_entity;

// A Filesystem shares no state with the object graph beyond the process executor it runs
// subprocesses through, so a plugin-built one is a complete instance rather than a second view on
// a Rust-side service.
php_entity! {
    handle: std::rc::Rc<std::cell::RefCell<Filesystem>>,
    type_error: "a Filesystem",
    classes: {
        r"Composer\Util\Filesystem": {
            constructor: {
                adapters: __php_binding_filesystem,
                into: |fs| std::rc::Rc::new(std::cell::RefCell::new(fs)),
            },
        },
    },
    methods: {
        r"Composer\Util\Filesystem": {
            handwritten: { module: promise, members: [removeDirectoryAsync] },
            instance: {
                adapters: __php_binding_filesystem,
                target: Filesystem,
                borrow: |fs| fs.borrow(),
                borrow_mut: |fs| fs.borrow_mut(),
            },
        },
    },
}

/// The members answering with a promise.
mod promise {
    use crate::php_worker::job;
    use crate::php_worker::job::JobOwner;
    use crate::php_worker::job::rpc::{poll_once, resolved_promise};
    use crate::php_worker::rpc::dispatch::error_throw;
    use crate::php_worker::rpc::entity::Call;
    use crate::util::Filesystem;
    use bstr::BString;
    use shirabe_php_rpc::{PhpThrow, RpcValue, ToRpcValue};
    use shirabe_php_shim::path_from_bytes;

    pub(super) fn remove_directory_async(
        fs: &std::rc::Rc<std::cell::RefCell<Filesystem>>,
        call: &mut Call<'_>,
    ) -> Result<RpcValue, PhpThrow> {
        let directory = std::path::PathBuf::from(path_from_bytes(&call.arg::<BString>(0)?));
        let executor = fs.borrow_mut().get_process_handle();
        let fs = fs.clone();
        let mut future: std::pin::Pin<Box<dyn std::future::Future<Output = anyhow::Result<bool>>>> =
            Box::pin(async move { Filesystem::remove_directory_async_via(&fs, &directory).await });
        // PHP settles the edge cases and starts the removal process before the call returns, and
        // raises what goes wrong up to there out of the call.
        match poll_once(&mut future) {
            std::task::Poll::Ready(removed) => {
                resolved_promise(removed.map_err(call.failed())?.to_rpc_value())
            }
            std::task::Poll::Pending => Ok(job::register(
                JobOwner::process_executor(&executor),
                executor,
                std::rc::Rc::new(std::cell::Cell::new(true)),
                PhpThrow::runtime("Aborted process"),
                Box::pin(async move {
                    future
                        .await
                        .map(|removed| removed.to_rpc_value())
                        .map_err(|error| error_throw(b"removeDirectoryAsync failed", &error))
                }),
            )),
        }
    }
}
