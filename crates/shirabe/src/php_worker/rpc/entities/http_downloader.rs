//! The methods a `Composer\Util\HttpDownloader` entity answers over RPC.

use crate::php_worker::codec::response_to_wire;
use crate::php_worker::job;
use crate::php_worker::job::JobOwner;
use crate::php_worker::job::rpc::{
    HTTP_DOWNLOADER_TICK_CAP, dispatch_tick_method, rejected_promise, resolved_promise,
};
use crate::php_worker::rpc::args::{arg, arg_or};
use crate::php_worker::rpc::dispatch::{error_throw, unavailable_method};
use crate::util::{HttpDownloader, HttpDownloaderJob, StreamContextOptions};
use bstr::{BStr, BString, ByteSlice};
use shirabe_php_rpc::{PhpThrow, PluginValue, ToWire};
use shirabe_php_shim::{bformat, path_from_bytes};

/// Serves the `HttpDownloader` proxy stub, whether the downloader behind it belongs to the object
/// graph or was built by plugin code writing `new HttpDownloader(...)`. Both run their requests
/// out of the Rust process, which is what keeps the authentication a run collects, the TLS
/// defaults and the parallel-request budget single-sourced across the boundary.
pub(crate) fn dispatch_http_downloader_method(
    downloader: &std::rc::Rc<std::cell::RefCell<HttpDownloader>>,
    method_name: &BStr,
    args: &[PluginValue],
) -> Result<PluginValue, PhpThrow> {
    match method_name.as_bytes() {
        b"getOptions" => Ok(downloader.borrow().get_options().clone().to_wire()),
        b"setOptions" => {
            let options = arg::<StreamContextOptions>(method_name, args, 0)?;
            downloader.borrow_mut().set_options(options);
            Ok(PluginValue::Null)
        }
        b"get" => {
            let url = arg::<BString>(method_name, args, 0)?;
            let options =
                arg_or::<StreamContextOptions>(method_name, args, 1, StreamContextOptions::new())?;
            let response = downloader
                .borrow()
                .get(&url, options)
                .map_err(|error| error_throw(b"get failed", &error))?;
            Ok(response_to_wire(&response))
        }
        b"copy" => {
            let url = arg::<BString>(method_name, args, 0)?;
            let to = arg::<BString>(method_name, args, 1)?;
            let options =
                arg_or::<StreamContextOptions>(method_name, args, 2, StreamContextOptions::new())?;
            let response = downloader
                .borrow()
                .copy(&url, path_from_bytes(&to), options)
                .map_err(|error| error_throw(b"copy failed", &error))?;
            Ok(response_to_wire(&response))
        }
        b"add" | b"addCopy" => {
            let url = arg::<BString>(method_name, args, 0)?;
            let (to, options_position) = match method_name.as_bytes() {
                b"add" => (None, 1),
                b"addCopy" => (Some(arg::<BString>(method_name, args, 1)?), 2),
                _ => unreachable!(),
            };
            let options = arg_or::<StreamContextOptions>(
                method_name,
                args,
                options_position,
                StreamContextOptions::new(),
            )?;
            let failed =
                |error: anyhow::Error| error_throw(bformat!("{method_name} failed"), &error);
            let started = std::rc::Rc::new(std::cell::Cell::new(false));
            // PHP raises the empty-url check and the async gate out of `add()`/`addCopy()`
            // itself, before the promise exists; only a failed request becomes a rejection.
            let job = downloader
                .borrow()
                .add_job(
                    url.as_bstr(),
                    to.as_ref().map(path_from_bytes),
                    options,
                    started.clone(),
                )
                .map_err(failed)?;
            match job {
                HttpDownloaderJob::Settled(Ok(response)) => {
                    resolved_promise(response_to_wire(&response))
                }
                HttpDownloaderJob::Settled(Err(error)) => rejected_promise(failed(error)),
                HttpDownloaderJob::Pending(future) => {
                    let method = method_name.to_owned();
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
                                .map(|response| response_to_wire(&response))
                                .map_err(|error| error_throw(bformat!("{method} failed"), &error))
                        }),
                    ))
                }
            }
        }
        b"enableAsync" => {
            downloader.borrow_mut().enable_async();
            Ok(PluginValue::Null)
        }
        b"wait" | b"countActiveJobs" => dispatch_tick_method(
            JobOwner::http_downloader(downloader),
            HTTP_DOWNLOADER_TICK_CAP,
            method_name,
            args,
        ),
        other => Err(unavailable_method("HttpDownloader", other)),
    }
}
