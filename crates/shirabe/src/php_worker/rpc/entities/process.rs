//! The methods a `Symfony\Component\Process\Process` entity answers over RPC.

use crate::php_worker::rpc::entity::Call;
use bstr::BString;
use indexmap::IndexMap;
use shirabe_php_binding_macros::php_entity;
use shirabe_php_rpc::{PhpThrow, RpcValue, ToRpcValue};
use shirabe_php_shim::PhpMixed;

shirabe_php_rpc::rpc_value_record! {
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

// A child a `ProcessExecutor` job ran to its exit. `Process` is a type of another crate, whose
// members are answered by hand.
php_entity! {
    handle: std::rc::Rc<std::cell::RefCell<shirabe_symfony_process::Process>>,
    type_error: "a Process",
    classes: {
        r"Shirabe\RustProcessStub": {},
    },
    methods: {
        r"Shirabe\RustProcessStub": {
            handwritten: { module: self },
        },
    },
}

/// Everything the stub answers about the terminated process behind it, read in one round
/// trip.
fn __shirabe_facts(
    process: &std::rc::Rc<std::cell::RefCell<shirabe_symfony_process::Process>>,
    call: &mut Call<'_>,
) -> Result<RpcValue, PhpThrow> {
    let failed = call.failed();
    let mut process = process.borrow_mut();
    Ok(ProcessFacts {
        exit_code: process.get_exit_code(),
        exit_code_text: process.get_exit_code_text(),
        output: process.get_output().map_err(&failed)?,
        error_output: process.get_error_output().map_err(&failed)?,
        signaled: process.has_been_signaled().map_err(&failed)?,
        term_signal: process.get_term_signal().map_err(&failed)?,
        stopped: process.has_been_stopped().map_err(&failed)?,
        stop_signal: process.get_stop_signal().map_err(&failed)?,
        command_line: process.get_command_line(),
        working_directory: process.get_working_directory(),
        env: process.get_env().clone(),
        timeout: process.get_timeout(),
        start_time: process.get_start_time().map_err(&failed)?,
    }
    .to_rpc_value())
}
