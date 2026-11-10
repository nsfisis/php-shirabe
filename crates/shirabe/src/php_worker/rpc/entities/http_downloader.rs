//! The methods a `Composer\Util\HttpDownloader` entity answers over RPC: whether the downloader
//! behind it belongs to the object graph or was built by plugin code writing
//! `new HttpDownloader(...)`, both run their requests out of the Rust process, which is what keeps
//! the authentication a run collects, the TLS defaults and the parallel-request budget
//! single-sourced across the boundary.

use crate::config::Config;
use crate::io::IOInterface;
use crate::php_worker::codec::response_to_plugin_value;
use crate::php_worker::job;
use crate::php_worker::job::JobOwner;
use crate::php_worker::job::rpc::{
    HTTP_DOWNLOADER_TICK_CAP, dispatch_tick_method, rejected_promise, resolved_promise,
};
use crate::php_worker::rpc::dispatch::error_throw;
use crate::php_worker::rpc::entity::{Call, Crossing, Entity};
use crate::php_worker::rpc::forwarding;
use crate::util::{HttpDownloader, HttpDownloaderJob, StreamContextOptions};
use bstr::{BStr, BString, ByteSlice as _};
use shirabe_php_rpc::{PhpThrow, PluginValue, ToPluginValue};
use shirabe_php_shim::{bformat, path_from_bytes};

impl Entity for std::rc::Rc<std::cell::RefCell<HttpDownloader>> {
    const EXPECTED: &'static [u8] = b"an HttpDownloader";

    fn address(&self) -> *const () {
        std::rc::Rc::as_ptr(self) as *const ()
    }

    fn class(&self) -> Result<&'static Crossing<Self>, PhpThrow> {
        Ok(&forwarding::util::http_downloader::CROSSING)
    }
}

/// `add()` and `addCopy()`: `to` is the target of the copy, and the options follow it.
fn add_job(
    downloader: &std::rc::Rc<std::cell::RefCell<HttpDownloader>>,
    call: &Call<'_>,
    to: Option<BString>,
    options_position: usize,
) -> Result<PluginValue, PhpThrow> {
    let url = call.arg::<BString>(0)?;
    let options = call.arg_or(options_position, StreamContextOptions::new())?;
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
        HttpDownloaderJob::Settled(Ok(response)) => {
            resolved_promise(response_to_plugin_value(&response))
        }
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
                        .map(|response| response_to_plugin_value(&response))
                        .map_err(|error| error_throw(bformat!("{method} failed"), &error))
                }),
            ))
        }
    }
}

impl forwarding::util::http_downloader::Forwarding for forwarding::util::http_downloader::PhpClass {
    type Handle = std::rc::Rc<std::cell::RefCell<HttpDownloader>>;

    // A downloader carries its own options, TLS defaults and request backends; what it shares
    // with the graph is the IO it collects authentication into and the config it reads, both of
    // which it receives as the proxies the plugin already holds.
    fn __construct(call: &mut Call<'_>) -> Result<Self::Handle, PhpThrow> {
        let downloader = HttpDownloader::new(
            call.entity::<std::rc::Rc<std::cell::RefCell<dyn IOInterface>>>(0)?,
            call.entity::<std::rc::Rc<std::cell::RefCell<Config>>>(1)?,
            call.arg_or(2, StreamContextOptions::new())?,
            call.arg_or(3, false)?,
        )
        .map_err(call.failed())?;
        Ok(std::rc::Rc::new(std::cell::RefCell::new(downloader)))
    }

    fn get_options(downloader: &Self::Handle, _: &mut Call<'_>) -> Result<PluginValue, PhpThrow> {
        Ok(downloader.borrow().get_options().clone().to_plugin_value())
    }

    fn set_options(
        downloader: &Self::Handle,
        call: &mut Call<'_>,
    ) -> Result<PluginValue, PhpThrow> {
        let options = call.arg::<StreamContextOptions>(0)?;
        downloader.borrow_mut().set_options(options);
        Ok(PluginValue::Null)
    }

    fn get(downloader: &Self::Handle, call: &mut Call<'_>) -> Result<PluginValue, PhpThrow> {
        let url = call.arg::<BString>(0)?;
        let options = call.arg_or(1, StreamContextOptions::new())?;
        let response = downloader
            .borrow()
            .get(&url, options)
            .map_err(call.failed())?;
        Ok(response_to_plugin_value(&response))
    }

    fn copy(downloader: &Self::Handle, call: &mut Call<'_>) -> Result<PluginValue, PhpThrow> {
        let url = call.arg::<BString>(0)?;
        let to = call.arg::<BString>(1)?;
        let options = call.arg_or(2, StreamContextOptions::new())?;
        let response = downloader
            .borrow()
            .copy(&url, path_from_bytes(&to), options)
            .map_err(call.failed())?;
        Ok(response_to_plugin_value(&response))
    }

    fn add(downloader: &Self::Handle, call: &mut Call<'_>) -> Result<PluginValue, PhpThrow> {
        add_job(downloader, call, None, 1)
    }

    fn add_copy(downloader: &Self::Handle, call: &mut Call<'_>) -> Result<PluginValue, PhpThrow> {
        let to = call.arg::<BString>(1)?;
        add_job(downloader, call, Some(to), 2)
    }

    fn enable_async(downloader: &Self::Handle, _: &mut Call<'_>) -> Result<PluginValue, PhpThrow> {
        downloader.borrow_mut().enable_async();
        Ok(PluginValue::Null)
    }

    fn wait(downloader: &Self::Handle, call: &mut Call<'_>) -> Result<PluginValue, PhpThrow> {
        dispatch_tick_method(
            JobOwner::http_downloader(downloader),
            HTTP_DOWNLOADER_TICK_CAP,
            call.method(),
            call.args(),
        )
    }

    fn count_active_jobs(
        downloader: &Self::Handle,
        call: &mut Call<'_>,
    ) -> Result<PluginValue, PhpThrow> {
        dispatch_tick_method(
            JobOwner::http_downloader(downloader),
            HTTP_DOWNLOADER_TICK_CAP,
            call.method(),
            call.args(),
        )
    }
}
