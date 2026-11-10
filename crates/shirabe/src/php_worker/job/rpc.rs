//! The RPC side of the job registry (`docs/dev/plugin-async.md`): ticking the jobs an entity counts
//! against, and promises answered without a job.

use crate::php_worker::call::PhpCall;
use crate::php_worker::job;
use crate::php_worker::job::JobOwner;
use crate::php_worker::rpc::args::arg;
use crate::util::sync_executor::block_on;
use bstr::{BStr, ByteSlice as _};
use shirabe_php_rpc::{PhpThrow, PluginValue};
use shirabe_php_shim::{bformat, error_chain_message};

/// Polls a job future that was just built, the one time a job is advanced outside a tick.
/// Composer starts a process inside `executeAsync()` itself when the job budget has room, and
/// this poll is what takes the future that far.
pub(crate) fn poll_once<T>(
    future: &mut std::pin::Pin<Box<dyn std::future::Future<Output = T>>>,
) -> std::task::Poll<T> {
    block_on(std::future::poll_fn(|cx| {
        std::task::Poll::Ready(future.as_mut().poll(cx))
    }))
}

/// How long one tick of each service may block with no job settling, matching the granularity of
/// Composer's own wait loops: the `curl_multi_select()` timeout of `CurlDownloader::tick()`, and
/// the `usleep()` between the ticks of `ProcessExecutor::wait()`.
pub(crate) const HTTP_DOWNLOADER_TICK_CAP: std::time::Duration = std::time::Duration::from_secs(5);
pub(crate) const PROCESS_EXECUTOR_TICK_CAP: std::time::Duration =
    std::time::Duration::from_millis(1);

/// Serves `wait(?int $index)` and `countActiveJobs(?int $index)` of the two services, whose
/// `$index` is a job id.
pub(crate) fn dispatch_tick_method(
    owner: JobOwner,
    cap: std::time::Duration,
    method_name: &BStr,
    args: &[PluginValue],
) -> Result<PluginValue, PhpThrow> {
    let index = match args.first() {
        None | Some(PluginValue::Null) => None,
        _ => Some(arg::<i64>(method_name, args, 0)? as u64),
    };
    match method_name.as_bytes() {
        b"countActiveJobs" => Ok(PluginValue::Int(job::tick(owner, cap, index)?)),
        b"wait" => {
            while job::tick(owner, cap, index)? != 0 {}
            Ok(PluginValue::Null)
        }
        _ => unreachable!(),
    }
}

/// A `\React\Promise\PromiseInterface` already fulfilled with `value`, built in the worker so
/// PHP callers get the object type their signatures declare. The answer is the descriptor the
/// worker exchanges for that promise, to be passed on as the answer of the call being served.
pub(crate) fn resolved_promise(value: PluginValue) -> Result<PluginValue, PhpThrow> {
    match PhpCall::Function(BStr::new("__shirabe_resolved_promise")).try_run(vec![value]) {
        Ok(Ok(promise)) => Ok(promise),
        Ok(Err(throw)) => Err(throw),
        Err(error) => Err(PhpThrow::runtime(bformat!(
            "creating a resolved promise in the plugin process failed: {}",
            error_chain_message(&error)
        ))),
    }
}

/// A `\React\Promise\PromiseInterface` already rejected with the exception `throw` describes,
/// built in the worker. A failed request reaches the plugin as a rejection it handles, the way it
/// does under Composer, rather than as a throw out of the call that started it.
pub(crate) fn rejected_promise(throw: PhpThrow) -> Result<PluginValue, PhpThrow> {
    match PhpCall::Function(BStr::new("__shirabe_rejected_promise"))
        .try_run(vec![throw.to_plugin_value()])
    {
        Ok(Ok(promise)) => Ok(promise),
        Ok(Err(throw)) => Err(throw),
        Err(error) => Err(PhpThrow::runtime(bformat!(
            "creating a rejected promise in the plugin process failed: {}",
            error_chain_message(&error)
        ))),
    }
}
