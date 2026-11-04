//! The methods a `Composer\Util\ProcessExecutor` entity answers over RPC.

use crate::plugin::job_registry;
use crate::plugin::job_registry::JobOwner;
use crate::plugin::rpc::codec::{arg, arg_throw};
use crate::plugin::rpc::dispatch::{error_throw, unavailable_method};
use crate::plugin::rpc::entities::event_dispatcher::{call_php_callable, is_php_callable};
use crate::plugin::rpc::jobs::{
    PROCESS_EXECUTOR_TICK_CAP, dispatch_tick_method, poll_once, rejected_promise,
};
use crate::plugin::rpc::table::{RustEntity, register_entity, rust_handle_value};
use bstr::{BStr, BString, ByteSlice};
use indexmap::IndexMap;
use shirabe_php_rpc::{FromWire, PhpThrow, PluginValue, ToWire, wire};
use shirabe_php_shim::{Catch as _, LogicException, bconcat, path_from_bytes};

/// Serves the `ProcessExecutor` proxy stub, whether the executor behind it belongs to the object
/// graph or was built by plugin code writing `new ProcessExecutor(...)`. Both run their children in
/// the Rust process, which is what keeps the timeout, the executable path cache and the job budget
/// single-sourced across the boundary instead of forking a copy per world.
pub(crate) fn dispatch_process_executor_method(
    process: &std::rc::Rc<std::cell::RefCell<crate::util::ProcessExecutor>>,
    method_name: &BStr,
    args: &[PluginValue],
    out_params: &mut IndexMap<u32, PluginValue>,
) -> Result<PluginValue, PhpThrow> {
    let failed = |error: anyhow::Error| error_throw(bconcat!(method_name, " failed"), &error);
    let cwd_arg = |position: usize| -> Result<Option<std::path::PathBuf>, PhpThrow> {
        match args.get(position) {
            None | Some(PluginValue::Null) => Ok(None),
            _ => arg::<BString>(method_name, args, position)
                .map(|cwd| Some(std::path::PathBuf::from(path_from_bytes(&cwd)))),
        }
    };
    match method_name.as_bytes() {
        b"execute" => {
            let command = exec_command_arg(method_name, args, 0)?;
            let cwd = cwd_arg(2)?;
            // PHP selects the output handling with `func_num_args() > 1` and `is_callable($output)`;
            // the stub reproduces the caller's arity, so an absent argument is the forwarding case.
            let Some(output) = args.get(1) else {
                return process
                    .borrow()
                    .execute(
                        command,
                        crate::util::ProcessExecutor::FORWARD_OUTPUT,
                        cwd.as_deref(),
                    )
                    .map(|code| code.to_wire())
                    .map_err(failed);
            };
            if is_php_callable(output).map_err(failed)? {
                let callable = output.clone();
                let callback: Box<dyn FnMut(&BStr, &BStr) -> anyhow::Result<bool>> =
                    Box::new(move |r#type: &BStr, buffer: &BStr| {
                        call_php_callable(
                            &callable,
                            vec![PluginValue::string(r#type), PluginValue::string(buffer)],
                        )?;
                        Ok(false)
                    });
                return process
                    .borrow()
                    .execute(command, callback, cwd.as_deref())
                    .map(|code| code.to_wire())
                    .map_err(failed);
            }
            let mut captured: Option<BString> = None;
            let code = process
                .borrow()
                .execute(command, &mut captured, cwd.as_deref())
                .map_err(failed)?;
            // PHP leaves `$output` alone when the child was signaled before the assignment, which
            // is what leaving it out of the out-params reproduces.
            if let Some(captured) = captured {
                out_params.insert(1, PluginValue::string(captured));
            }
            Ok(code.to_wire())
        }
        b"executeTty" => Ok(process
            .borrow()
            .execute_tty(
                exec_command_arg(method_name, args, 0)?,
                cwd_arg(1)?.as_deref(),
            )
            .map_err(failed)?
            .to_wire()),
        b"splitLines" => {
            let output = match args.first() {
                None | Some(PluginValue::Null) => BString::default(),
                _ => arg::<BString>(method_name, args, 0)?,
            };
            Ok(PluginValue::List(
                process
                    .borrow()
                    .split_lines(&output)
                    .into_iter()
                    .map(PluginValue::string)
                    .collect(),
            ))
        }
        b"getErrorOutput" => Ok(PluginValue::string(process.borrow().get_error_output())),
        b"requiresGitDirEnv" => Ok(process
            .borrow()
            .requires_git_dir_env(&exec_command_arg(method_name, args, 0)?)
            .to_wire()),
        b"setMaxJobs" => {
            process
                .borrow()
                .set_max_jobs(arg::<i64>(method_name, args, 0)?);
            Ok(PluginValue::Null)
        }
        b"resetMaxJobs" => {
            process.borrow().reset_max_jobs();
            Ok(PluginValue::Null)
        }
        b"enableAsync" => {
            process.borrow().enable_async();
            Ok(PluginValue::Null)
        }
        b"executeAsync" => {
            let command = exec_command_arg(method_name, args, 0)?;
            let cwd = cwd_arg(1)?;
            let started = std::rc::Rc::new(std::cell::Cell::new(false));
            let mut future =
                process
                    .borrow()
                    .execute_async_job(command, cwd.as_deref(), Some(started.clone()));
            let resolve = |child: shirabe_symfony_process::Process| {
                rust_handle_value(
                    register_entity(RustEntity::Process(std::rc::Rc::new(
                        std::cell::RefCell::new(child),
                    ))),
                    BStr::new("Shirabe\\RustProcessStub"),
                )
            };
            let future: std::pin::Pin<
                Box<dyn std::future::Future<Output = Result<PluginValue, PhpThrow>>>,
            > = match poll_once(&mut future) {
                // PHP raises the async gate out of `executeAsync()` itself, and it is the port's
                // only `LogicException` here; a child that fails to start rejects the promise.
                std::task::Poll::Ready(Err(error)) if error.is_instanceof::<LogicException>() => {
                    return Err(failed(error));
                }
                std::task::Poll::Ready(Err(error)) => return rejected_promise(failed(error)),
                // The child exited within the poll. PHP resolves only from `countActiveJobs()`,
                // so the result waits in the registry for the next tick.
                std::task::Poll::Ready(Ok(child)) => Box::pin(async move { Ok(resolve(child)) }),
                std::task::Poll::Pending => Box::pin(async move {
                    future
                        .await
                        .map(resolve)
                        .map_err(|error| error_throw(b"executeAsync failed", &error))
                }),
            };
            Ok(job_registry::register(
                JobOwner::process_executor(process),
                process.clone(),
                started,
                // TODO(async): cancelling drops the future and the `Process` in it, whose drop
                // stops the child with SIGTERM; PHP sends SIGINT first and then calls `stop(1)`.
                PhpThrow::runtime("Aborted process"),
                future,
            ))
        }
        b"wait" | b"countActiveJobs" => dispatch_tick_method(
            JobOwner::process_executor(process),
            PROCESS_EXECUTOR_TICK_CAP,
            method_name,
            args,
        ),
        other => Err(unavailable_method("ProcessExecutor", other)),
    }
}

/// Decodes the `string|non-empty-list<string>` a process executor takes as its command.
fn exec_command_arg(
    method: &BStr,
    args: &[PluginValue],
    position: usize,
) -> Result<crate::util::process_executor::CommandLine, PhpThrow> {
    let value = args.get(position).unwrap_or(&wire::NULL);
    match value {
        PluginValue::String(bytes) => Ok(crate::util::process_executor::CommandLine::Shell(
            bytes.clone(),
        )),
        PluginValue::List(_) => {
            Vec::<BString>::from_wire(value).map(crate::util::process_executor::CommandLine::Args)
        }
        other => Err(wire::unexpected("a command string or argument list", other)),
    }
    .map_err(|throw| arg_throw(method, position, throw))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_process_command_keeps_its_bytes() {
        let bytes = BString::from(b"echo \xff".as_slice());

        match exec_command_arg(
            BStr::new("execute"),
            &[PluginValue::String(bytes.clone())],
            0,
        )
        .unwrap()
        {
            crate::util::process_executor::CommandLine::Shell(command) => {
                assert_eq!(bytes, command)
            }
            other => panic!("expected a shell command, got {other:?}"),
        }
    }
}
