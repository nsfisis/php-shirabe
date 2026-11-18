//! The methods a `Composer\Util\ProcessExecutor` entity answers over RPC.

use crate::io::IOInterface;
use crate::php_worker::job;
use crate::php_worker::job::JobOwner;
use crate::php_worker::job::rpc::{
    PROCESS_EXECUTOR_TICK_CAP, dispatch_tick_method, poll_once, rejected_promise,
};
use crate::php_worker::rpc::dispatch::error_throw;
use crate::php_worker::rpc::entities::event_dispatcher::{call_php_callable, is_php_callable};
use crate::php_worker::rpc::entity::{Bindings, Call, Entity, entity_value};
use crate::php_worker::rpc::forwarding;
use crate::util::{CommandLine, ProcessExecutor};
use bstr::{BStr, BString};
use shirabe_php_rpc::{FromRpcValue, PhpThrow, RpcValue, ToRpcValue, codec};
use shirabe_php_shim::{Catch as _, LogicException, path_from_bytes};

impl Entity for std::rc::Rc<std::cell::RefCell<ProcessExecutor>> {
    const EXPECTED: &'static [u8] = b"a ProcessExecutor";

    fn address(&self) -> *const () {
        std::rc::Rc::as_ptr(self) as *const ()
    }

    fn class(&self) -> Result<&'static Bindings<Self>, PhpThrow> {
        Ok(&forwarding::util::process_executor::BINDINGS)
    }
}

fn cwd(call: &Call<'_>, position: usize) -> Result<Option<std::path::PathBuf>, PhpThrow> {
    match call.args().get(position) {
        None | Some(RpcValue::Null) => Ok(None),
        _ => call
            .arg::<BString>(position)
            .map(|cwd| Some(std::path::PathBuf::from(path_from_bytes(&cwd)))),
    }
}

/// Serves the `ProcessExecutor` proxy stub, whether the executor behind it belongs to the object
/// graph or was built by plugin code writing `new ProcessExecutor(...)`. Both run their children in
/// the Rust process, which is what keeps the timeout, the executable path cache and the job budget
/// single-sourced across the boundary instead of forking a copy per side.
impl forwarding::util::process_executor::Forwarding
    for forwarding::util::process_executor::PhpClass
{
    type Handle = std::rc::Rc<std::cell::RefCell<ProcessExecutor>>;

    // The job queue and the async permits of a process executor are its own state, and the
    // timeout it runs children under is the Rust side's; a plugin-built one is a complete
    // instance of the former holding the latter, not a second view on the graph's executor.
    fn __construct(call: &mut Call<'_>) -> Result<Self::Handle, PhpThrow> {
        let io = call.optional_entity::<std::rc::Rc<std::cell::RefCell<dyn IOInterface>>>(0)?;
        Ok(std::rc::Rc::new(std::cell::RefCell::new(
            ProcessExecutor::new(io),
        )))
    }

    fn execute(process: &Self::Handle, call: &mut Call<'_>) -> Result<RpcValue, PhpThrow> {
        let failed = call.failed();
        let command = exec_command_arg(call, 0)?;
        let cwd = cwd(call, 2)?;
        // PHP selects the output handling with `func_num_args() > 1` and `is_callable($output)`;
        // the stub reproduces the caller's arity, so an absent argument is the forwarding case.
        let Some(output) = call.args().get(1) else {
            return process
                .borrow()
                .execute(command, ProcessExecutor::FORWARD_OUTPUT, cwd.as_deref())
                .map(|code| code.to_rpc_value())
                .map_err(failed);
        };
        if is_php_callable(output).map_err(&failed)? {
            let callable = output.clone();
            let callback: Box<dyn FnMut(&BStr, &BStr) -> anyhow::Result<bool>> =
                Box::new(move |r#type: &BStr, buffer: &BStr| {
                    call_php_callable(
                        &callable,
                        vec![RpcValue::string(r#type), RpcValue::string(buffer)],
                    )?;
                    Ok(false)
                });
            return process
                .borrow()
                .execute(command, callback, cwd.as_deref())
                .map(|code| code.to_rpc_value())
                .map_err(failed);
        }
        let mut captured: Option<BString> = None;
        let code = process
            .borrow()
            .execute(command, &mut captured, cwd.as_deref())
            .map_err(failed)?;
        // PHP leaves `$output` alone when the child was signaled before the assignment, which is
        // what leaving it out of the out-params reproduces.
        if let Some(captured) = captured {
            call.set_out_param(1, RpcValue::string(captured));
        }
        Ok(code.to_rpc_value())
    }

    fn execute_tty(process: &Self::Handle, call: &mut Call<'_>) -> Result<RpcValue, PhpThrow> {
        Ok(process
            .borrow()
            .execute_tty(exec_command_arg(call, 0)?, cwd(call, 1)?.as_deref())
            .map_err(call.failed())?
            .to_rpc_value())
    }

    fn split_lines(process: &Self::Handle, call: &mut Call<'_>) -> Result<RpcValue, PhpThrow> {
        let output = call.arg::<Option<BString>>(0)?.unwrap_or_default();
        Ok(RpcValue::List(
            process
                .borrow()
                .split_lines(&output)
                .into_iter()
                .map(RpcValue::string)
                .collect(),
        ))
    }

    fn get_error_output(process: &Self::Handle, _: &mut Call<'_>) -> Result<RpcValue, PhpThrow> {
        Ok(RpcValue::string(process.borrow().get_error_output()))
    }

    fn requires_git_dir_env(
        process: &Self::Handle,
        call: &mut Call<'_>,
    ) -> Result<RpcValue, PhpThrow> {
        Ok(process
            .borrow()
            .requires_git_dir_env(&exec_command_arg(call, 0)?)
            .to_rpc_value())
    }

    fn set_max_jobs(process: &Self::Handle, call: &mut Call<'_>) -> Result<RpcValue, PhpThrow> {
        process.borrow().set_max_jobs(call.arg::<i64>(0)?);
        Ok(RpcValue::Null)
    }

    fn reset_max_jobs(process: &Self::Handle, _: &mut Call<'_>) -> Result<RpcValue, PhpThrow> {
        process.borrow().reset_max_jobs();
        Ok(RpcValue::Null)
    }

    fn enable_async(process: &Self::Handle, _: &mut Call<'_>) -> Result<RpcValue, PhpThrow> {
        process.borrow().enable_async();
        Ok(RpcValue::Null)
    }

    fn execute_async(process: &Self::Handle, call: &mut Call<'_>) -> Result<RpcValue, PhpThrow> {
        let failed = call.failed();
        let command = exec_command_arg(call, 0)?;
        let cwd = cwd(call, 1)?;
        let started = std::rc::Rc::new(std::cell::Cell::new(false));
        let mut future =
            process
                .borrow()
                .execute_async_job(command, cwd.as_deref(), Some(started.clone()));
        let resolve = |child: shirabe_symfony_process::Process| {
            entity_value(&std::rc::Rc::new(std::cell::RefCell::new(child)))
        };
        let future: std::pin::Pin<
            Box<dyn std::future::Future<Output = Result<RpcValue, PhpThrow>>>,
        > = match poll_once(&mut future) {
            // PHP raises the async gate out of `executeAsync()` itself, and it is the port's only
            // `LogicException` here; a child that fails to start rejects the promise.
            std::task::Poll::Ready(Err(error)) if error.is_instanceof::<LogicException>() => {
                return Err(failed(error));
            }
            std::task::Poll::Ready(Err(error)) => return rejected_promise(failed(error)),
            // The child exited within the poll. PHP resolves only from `countActiveJobs()`, so
            // the result waits in the registry for the next tick.
            std::task::Poll::Ready(Ok(child)) => Box::pin(async move { resolve(child) }),
            std::task::Poll::Pending => Box::pin(async move {
                resolve(
                    future
                        .await
                        .map_err(|error| error_throw(b"executeAsync failed", &error))?,
                )
            }),
        };
        Ok(job::register(
            JobOwner::process_executor(process),
            process.clone(),
            started,
            // TODO(async): cancelling drops the future and the `Process` in it, whose drop stops
            // the child with SIGTERM; PHP sends SIGINT first and then calls `stop(1)`.
            PhpThrow::runtime("Aborted process"),
            future,
        ))
    }

    fn wait(process: &Self::Handle, call: &mut Call<'_>) -> Result<RpcValue, PhpThrow> {
        dispatch_tick_method(
            JobOwner::process_executor(process),
            PROCESS_EXECUTOR_TICK_CAP,
            call.method(),
            call.args(),
        )
    }

    fn count_active_jobs(
        process: &Self::Handle,
        call: &mut Call<'_>,
    ) -> Result<RpcValue, PhpThrow> {
        dispatch_tick_method(
            JobOwner::process_executor(process),
            PROCESS_EXECUTOR_TICK_CAP,
            call.method(),
            call.args(),
        )
    }

    fn get_timeout(_: &mut Call<'_>) -> Result<RpcValue, PhpThrow> {
        Ok(ProcessExecutor::get_timeout().to_rpc_value())
    }

    fn set_timeout(call: &mut Call<'_>) -> Result<RpcValue, PhpThrow> {
        ProcessExecutor::set_timeout(call.arg::<i64>(0)?);
        Ok(RpcValue::Null)
    }
}

/// Decodes the `string|non-empty-list<string>` a process executor takes as its command.
fn exec_command_arg(call: &Call<'_>, position: usize) -> Result<CommandLine, PhpThrow> {
    let value = call.args().get(position).unwrap_or(&codec::NULL);
    match value {
        RpcValue::String(bytes) => Ok(CommandLine::Shell(bytes.clone())),
        RpcValue::List(_) => Vec::<BString>::from_rpc_value(value).map(CommandLine::Args),
        other => Err(codec::unexpected(
            "a command string or argument list",
            other,
        )),
    }
    .map_err(|throw| call.arg_throw(position, throw))
}

#[cfg(test)]
mod tests {
    use super::*;
    use indexmap::IndexMap;

    #[test]
    fn a_process_command_keeps_its_bytes() {
        let bytes = BString::from(b"echo \xff".as_slice());

        let args = [RpcValue::String(bytes.clone())];
        let mut out_params = IndexMap::new();
        let call = Call::new(
            BStr::new("Composer\\Util\\ProcessExecutor"),
            BStr::new("execute"),
            &args,
            &mut out_params,
        );
        match exec_command_arg(&call, 0).unwrap() {
            CommandLine::Shell(command) => {
                assert_eq!(bytes, command)
            }
            other => panic!("expected a shell command, got {other:?}"),
        }
    }
}
