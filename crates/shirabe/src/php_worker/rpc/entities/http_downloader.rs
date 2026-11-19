//! The methods a `Composer\Util\HttpDownloader` entity answers over RPC: whether the downloader
//! behind it belongs to the object graph or was built by plugin code writing
//! `new HttpDownloader(...)`, both run their requests out of the Rust process, which is what keeps
//! the authentication a run collects, the TLS defaults and the parallel-request budget
//! single-sourced across the boundary.

use crate::php_worker::job;
use crate::php_worker::job::JobOwner;
use crate::php_worker::job::rpc::{
    HTTP_DOWNLOADER_TICK_CAP, dispatch_tick_method, rejected_promise, resolved_promise,
};
use crate::php_worker::rpc::dispatch::error_throw;
use crate::php_worker::rpc::entity::Call;
use crate::util::{
    __php_binding_http_downloader, HttpDownloader, HttpDownloaderJob, StreamContextOptions,
};
use bstr::{BStr, BString, ByteSlice as _};
use shirabe_php_binding_macros::php_entity;
use shirabe_php_rpc::{PhpThrow, RpcValue, ToRpcValue};
use shirabe_php_shim::{bformat, path_from_bytes};

// A downloader carries its own options, TLS defaults and request backends; what it shares with
// the graph is the IO it collects authentication into and the config it reads, both of which it
// receives as the proxies the plugin already holds.
php_entity! {
    handle: std::rc::Rc<std::cell::RefCell<HttpDownloader>>,
    type_error: "an HttpDownloader",
    classes: {
        r"Composer\Util\HttpDownloader": {
            constructor: {
                adapters: __php_binding_http_downloader,
                into: |downloader| std::rc::Rc::new(std::cell::RefCell::new(downloader)),
            },
        },
    },
    methods: {
        r"Composer\Util\HttpDownloader": {
            handwritten: { module: self, members: [add, addCopy, wait, countActiveJobs] },
            instance: {
                adapters: __php_binding_http_downloader,
                target: HttpDownloader,
                borrow: |downloader| downloader.borrow(),
                borrow_mut: |downloader| downloader.borrow_mut(),
            },
        },
    },
}

/// `add()` and `addCopy()`: `to` is the target of the copy, and the options follow it.
fn add_job(
    downloader: &std::rc::Rc<std::cell::RefCell<HttpDownloader>>,
    call: &Call<'_>,
    to: Option<BString>,
    options_position: usize,
) -> Result<RpcValue, PhpThrow> {
    let url = call.arg::<BString>(0)?;
    let options = call.arg::<StreamContextOptions>(options_position)?;
    let failed = call.failed();
    let started = std::rc::Rc::new(std::cell::Cell::new(false));
    // PHP raises the empty-url check and the async gate out of `add()`/`addCopy()` itself,
    // before the promise exists; only a failed request becomes a rejection.
    let job = downloader
        .borrow()
        .add_job(
            url.as_bstr(),
            to.as_ref().map(path_from_bytes),
            options,
            started.clone(),
        )
        .map_err(&failed)?;
    match job {
        HttpDownloaderJob::Settled(Ok(response)) => resolved_promise(response.to_rpc_value()),
        HttpDownloaderJob::Settled(Err(error)) => rejected_promise(failed(error)),
        HttpDownloaderJob::Pending(future) => {
            let method = call.method().to_owned();
            Ok(job::register(
                JobOwner::http_downloader(downloader),
                downloader.clone(),
                started,
                error_throw(
                    BStr::new(""),
                    &HttpDownloader::canceled(url.as_bstr()).into(),
                ),
                Box::pin(async move {
                    future
                        .await
                        .map(ToRpcValue::to_rpc_value)
                        .map_err(|error| error_throw(bformat!("{method} failed"), &error))
                }),
            ))
        }
    }
}

fn add(
    downloader: &std::rc::Rc<std::cell::RefCell<HttpDownloader>>,
    call: &mut Call<'_>,
) -> Result<RpcValue, PhpThrow> {
    add_job(downloader, call, None, 1)
}

fn add_copy(
    downloader: &std::rc::Rc<std::cell::RefCell<HttpDownloader>>,
    call: &mut Call<'_>,
) -> Result<RpcValue, PhpThrow> {
    let to = call.arg::<BString>(1)?;
    add_job(downloader, call, Some(to), 2)
}

fn wait(
    downloader: &std::rc::Rc<std::cell::RefCell<HttpDownloader>>,
    call: &mut Call<'_>,
) -> Result<RpcValue, PhpThrow> {
    dispatch_tick_method(
        JobOwner::http_downloader(downloader),
        HTTP_DOWNLOADER_TICK_CAP,
        call.method(),
        call.args(),
    )
}

fn count_active_jobs(
    downloader: &std::rc::Rc<std::cell::RefCell<HttpDownloader>>,
    call: &mut Call<'_>,
) -> Result<RpcValue, PhpThrow> {
    dispatch_tick_method(
        JobOwner::http_downloader(downloader),
        HTTP_DOWNLOADER_TICK_CAP,
        call.method(),
        call.args(),
    )
}
