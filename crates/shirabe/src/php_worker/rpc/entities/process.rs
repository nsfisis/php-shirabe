//! The methods a `Symfony\Component\Process\Process` entity answers over RPC.

use crate::php_worker::rpc::dispatch::{error_throw, unavailable_method};
use bstr::{BStr, BString};
use indexmap::IndexMap;
use shirabe_php_rpc::{PhpThrow, PluginValue, ToWire};
use shirabe_php_shim::PhpMixed;

shirabe_php_rpc::wire_record! {
    /// Everything `Shirabe\RustProcessStub` answers about the terminated process behind it.
    #[derive(Debug)]
    struct ProcessFacts {
        exit_code: Option<i64>,
        exit_code_text: Option<BString>,
        output: BString,
        error_output: BString,
        signaled: bool,
        term_signal: i64,
        stopped: bool,
        stop_signal: i64,
        command_line: BString,
        working_directory: Option<std::path::PathBuf>,
        env: IndexMap<BString, PhpMixed>,
        timeout: Option<f64>,
        start_time: f64,
    }
}

/// Serves `Shirabe\RustProcessStub`: everything the stub answers about the terminated process
/// behind it, read in one round trip.
pub(crate) fn dispatch_process_method(
    process: &std::rc::Rc<std::cell::RefCell<shirabe_symfony_process::Process>>,
    method_name: &BStr,
) -> Result<PluginValue, PhpThrow> {
    if method_name != "__shirabe_facts" {
        return Err(unavailable_method("Process", method_name));
    }
    let failed = |error: anyhow::Error| error_throw(b"reading the process failed", &error);
    let mut process = process.borrow_mut();
    Ok(ProcessFacts {
        exit_code: process.get_exit_code(),
        exit_code_text: process.get_exit_code_text(),
        output: process.get_output().map_err(failed)?,
        error_output: process.get_error_output().map_err(failed)?,
        signaled: process.has_been_signaled().map_err(failed)?,
        term_signal: process.get_term_signal().map_err(failed)?,
        stopped: process.has_been_stopped().map_err(failed)?,
        stop_signal: process.get_stop_signal().map_err(failed)?,
        command_line: process.get_command_line(),
        working_directory: process.get_working_directory(),
        env: process.get_env().clone(),
        timeout: process.get_timeout(),
        start_time: process.get_start_time().map_err(failed)?,
    }
    .to_wire())
}
