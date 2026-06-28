//! ref: composer/vendor/symfony/console/Helper/ProcessHelper.php

use crate::symfony::console::helper::debug_formatter_helper::DebugFormatterHelper;
use crate::symfony::console::helper::helper::Helper;
use crate::symfony::console::helper::helper_interface::HelperInterface;
use crate::symfony::console::helper::helper_set::HelperSet;
use crate::symfony::console::output::ConsoleOutputInterface;
use crate::symfony::console::output::output_interface::{self, OutputInterface};
use crate::symfony::process::exception::process_failed_exception::ProcessFailedException;
use crate::symfony::process::process::Process;
use std::cell::RefCell;
use std::rc::Rc;

/// The ProcessHelper class provides helpers to run external processes.
///
/// @final
#[derive(Debug, Default)]
pub struct ProcessHelper {
    inner: Helper,
}

/// `$cmd` is either a `Process` instance or an array whose first element is a
/// binary path (string) or a `Process`, followed by extra environment entries.
#[derive(Debug)]
pub enum ProcessHelperCmd {
    Process(Process),
    Array(Vec<ProcessHelperCmdElement>),
}

#[derive(Debug)]
pub enum ProcessHelperCmdElement {
    String(String),
    Process(Process),
}

impl ProcessHelper {
    /// Runs an external process.
    ///
    /// @param array|Process $cmd      An instance of Process or an array of the command and arguments
    /// @param callable|null $callback A PHP callback to run whenever there is some
    ///                                output available on STDOUT or STDERR
    pub fn run(
        &self,
        output: Rc<RefCell<dyn OutputInterface>>,
        cmd: ProcessHelperCmd,
        error: Option<&str>,
        callback: Option<Box<dyn FnMut(&str, &str)>>,
        verbosity: i64,
    ) -> anyhow::Result<Process> {
        // `class_exists(Process::class)` guards against the optional symfony/process
        // component being absent; in this port the component is always available.

        // PHP: `if ($output instanceof ConsoleOutputInterface) { $output =
        // $output->getErrorOutput(); }`. ConsoleOutput is the only OutputInterface
        // implementor that also implements ConsoleOutputInterface, so the check
        // reduces to a downcast to the concrete type.
        let output: Rc<RefCell<dyn OutputInterface>> = {
            let redirected = shirabe_php_shim::AsAny::as_any(&*output.borrow())
                .downcast_ref::<crate::symfony::console::output::console_output::ConsoleOutput>()
                .map(|console| console.get_error_output());
            redirected.unwrap_or(output)
        };

        let formatter: Rc<RefCell<DebugFormatterHelper>> = self
            .get_helper_set()
            .unwrap()
            .borrow()
            .get_debug_formatter();

        // Normalize $cmd: a single Process becomes a one-element array.
        let mut cmd = match cmd {
            ProcessHelperCmd::Process(process) => {
                vec![ProcessHelperCmdElement::Process(process)]
            }
            ProcessHelperCmd::Array(cmd) => cmd,
        };

        // `!\is_array($cmd)` cannot happen given the enum, so the TypeError branch
        // is unreachable here.

        let mut process: Process;
        match cmd.first() {
            Some(ProcessHelperCmdElement::String(_)) => {
                let command: Vec<String> = cmd
                    .iter()
                    .map(|element| match element {
                        ProcessHelperCmdElement::String(s) => s.clone(),
                        ProcessHelperCmdElement::Process(_) => unreachable!(),
                    })
                    .collect();
                process = Process::new(
                    command,
                    None,
                    None,
                    shirabe_php_shim::PhpMixed::Null,
                    Some(60.0),
                )?;
                cmd = vec![];
            }
            Some(ProcessHelperCmdElement::Process(_)) => {
                let first = cmd.remove(0);
                process = match first {
                    ProcessHelperCmdElement::Process(process) => process,
                    ProcessHelperCmdElement::String(_) => unreachable!(),
                };
            }
            None => {
                anyhow::bail!(shirabe_php_shim::InvalidArgumentException {
                    message: format!(
                        "Invalid command provided to \"{}()\": the command should be an array whose first element is either the path to the binary to run or a \"Process\" object.",
                        shirabe_php_shim::PhpMixed::String("ProcessHelper::run".to_string()),
                    ),
                    code: 0,
                });
            }
        }

        if verbosity <= output.borrow().get_verbosity() {
            let started = Self::formatter_start(
                &formatter,
                &shirabe_php_shim::spl_object_hash_process(&process),
                &self.escape_string(&process.get_command_line()),
            );
            output
                .borrow()
                .write(&[started], false, output_interface::OUTPUT_NORMAL);
        }

        let callback = if output.borrow().is_debug() {
            Some(self.wrap_callback(output.clone(), &process, callback))
        } else {
            callback
        };

        // PHP passes the remaining `$cmd` array as the `$env` argument to Process::run.
        let env: indexmap::IndexMap<String, shirabe_php_shim::PhpMixed> = cmd
            .iter()
            .enumerate()
            .filter_map(|(i, element)| match element {
                ProcessHelperCmdElement::String(s) => {
                    Some((i.to_string(), shirabe_php_shim::PhpMixed::String(s.clone())))
                }
                ProcessHelperCmdElement::Process(_) => None,
            })
            .collect();
        let callback: Option<Box<dyn FnMut(&str, &str) -> bool>> = callback.map(|mut cb| {
            Box::new(move |r#type: &str, buffer: &str| -> bool {
                cb(r#type, buffer);
                false
            }) as Box<dyn FnMut(&str, &str) -> bool>
        });
        process.run(callback, env)?;

        if verbosity <= output.borrow().get_verbosity() {
            let message = if process.is_successful() {
                "Command ran successfully".to_string()
            } else {
                format!(
                    "{} Command did not run successfully",
                    match process.get_exit_code() {
                        Some(code) => shirabe_php_shim::PhpMixed::Int(code),
                        None => shirabe_php_shim::PhpMixed::Null,
                    },
                )
            };
            let stopped = Self::formatter_stop(
                &formatter,
                &shirabe_php_shim::spl_object_hash_process(&process),
                &message,
                process.is_successful(),
            );
            output
                .borrow()
                .write(&[stopped], false, output_interface::OUTPUT_NORMAL);
        }

        if !process.is_successful()
            && let Some(error) = error
        {
            output.borrow().writeln(
                &[format!("<error>{}</error>", self.escape_string(error))],
                output_interface::OUTPUT_NORMAL,
            );
        }

        Ok(process)
    }

    /// Runs the process.
    ///
    /// This is identical to run() except that an exception is thrown if the process
    /// exits with a non-zero exit code.
    ///
    /// @param array|Process $cmd      An instance of Process or a command to run
    /// @param callable|null $callback A PHP callback to run whenever there is some
    ///                                output available on STDOUT or STDERR
    ///
    /// @throws ProcessFailedException
    ///
    /// @see run()
    pub fn must_run(
        &self,
        output: Rc<RefCell<dyn OutputInterface>>,
        cmd: ProcessHelperCmd,
        error: Option<&str>,
        callback: Option<Box<dyn FnMut(&str, &str)>>,
    ) -> anyhow::Result<Process> {
        let mut process = self.run(
            output,
            cmd,
            error,
            callback,
            output_interface::VERBOSITY_VERY_VERBOSE,
        )?;

        if !process.is_successful() {
            anyhow::bail!(ProcessFailedException::new(&mut process)?);
        }

        Ok(process)
    }

    /// Wraps a Process callback to add debugging output.
    pub fn wrap_callback(
        &self,
        output: Rc<RefCell<dyn OutputInterface>>,
        process: &Process,
        mut callback: Option<Box<dyn FnMut(&str, &str)>>,
    ) -> Box<dyn FnMut(&str, &str)> {
        // PHP: `if ($output instanceof ConsoleOutputInterface) { $output =
        // $output->getErrorOutput(); }`. ConsoleOutput is the only OutputInterface
        // implementor that also implements ConsoleOutputInterface, so the check
        // reduces to a downcast to the concrete type.
        let output: Rc<RefCell<dyn OutputInterface>> = {
            let redirected = shirabe_php_shim::AsAny::as_any(&*output.borrow())
                .downcast_ref::<crate::symfony::console::output::console_output::ConsoleOutput>()
                .map(|console| console.get_error_output());
            redirected.unwrap_or(output)
        };

        let formatter: Rc<RefCell<DebugFormatterHelper>> = self
            .get_helper_set()
            .unwrap()
            .borrow()
            .get_debug_formatter();

        let object_hash = shirabe_php_shim::spl_object_hash_process(process);

        Box::new(move |r#type: &str, buffer: &str| {
            let progressed = Self::formatter_progress(
                &formatter,
                &object_hash,
                &Self::escape_string_static(buffer),
                Process::ERR == r#type,
            );
            output
                .borrow()
                .write(&[progressed], false, output_interface::OUTPUT_NORMAL);

            if let Some(callback) = callback.as_mut() {
                callback(r#type, buffer);
            }
        })
    }

    fn escape_string(&self, str: &str) -> String {
        shirabe_php_shim::str_replace("<", "\\<", str)
    }

    fn escape_string_static(str: &str) -> String {
        shirabe_php_shim::str_replace("<", "\\<", str)
    }

    fn formatter_start(
        formatter: &Rc<RefCell<DebugFormatterHelper>>,
        id: &str,
        message: &str,
    ) -> String {
        formatter.borrow_mut().start(id, message, "RUN")
    }

    fn formatter_stop(
        formatter: &Rc<RefCell<DebugFormatterHelper>>,
        id: &str,
        message: &str,
        successful: bool,
    ) -> String {
        formatter.borrow_mut().stop(id, message, successful, "RES")
    }

    fn formatter_progress(
        formatter: &Rc<RefCell<DebugFormatterHelper>>,
        id: &str,
        buffer: &str,
        error: bool,
    ) -> String {
        formatter
            .borrow_mut()
            .progress(id, buffer, error, "OUT", "ERR")
    }
}

impl HelperInterface for ProcessHelper {
    fn set_helper_set(&mut self, helper_set: Option<Rc<RefCell<HelperSet>>>) {
        self.inner.set_helper_set(helper_set);
    }

    fn get_helper_set(&self) -> Option<Rc<RefCell<HelperSet>>> {
        self.inner.get_helper_set()
    }

    fn get_name(&self) -> String {
        "process".to_string()
    }
}
