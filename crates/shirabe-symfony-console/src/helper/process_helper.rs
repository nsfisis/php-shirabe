//! ref: composer/vendor/symfony/console/Helper/ProcessHelper.php sha1:fa7a183ca1b817982c97017e48d81084a1c914a9

use crate::helper::{DebugFormatterHelper, Helper, HelperInterface, HelperSet};
use crate::output::{ConsoleOutputInterface, OutputInterface, output_interface};
use bstr::{BStr, BString};
use shirabe_php_shim::{
    AsAny, InvalidArgumentException, PhpMixed, bconcat, spl_object_hash, str_replace,
};
use shirabe_symfony_process::exception::ProcessFailedException;
use shirabe_symfony_process::process::Process;

/// The ProcessHelper class provides helpers to run external processes.
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
    String(BString),
    Process(Process),
}

impl ProcessHelper {
    /// Runs an external process.
    pub fn run(
        &self,
        output: std::rc::Rc<std::cell::RefCell<dyn OutputInterface>>,
        cmd: ProcessHelperCmd,
        error: Option<&BStr>,
        callback: Option<Box<dyn FnMut(&BStr, &BStr)>>,
        verbosity: i64,
    ) -> anyhow::Result<Process> {
        // `class_exists(Process::class)` guards against the optional symfony/process
        // component being absent; in this port the component is always available.

        // PHP: `if ($output instanceof ConsoleOutputInterface) { $output =
        // $output->getErrorOutput(); }`. ConsoleOutput is the only OutputInterface
        // implementor that also implements ConsoleOutputInterface, so the check
        // reduces to a downcast to the concrete type.
        let output: std::rc::Rc<std::cell::RefCell<dyn OutputInterface>> = {
            let redirected = AsAny::as_any(&*output.borrow())
                .downcast_ref::<crate::output::ConsoleOutput>()
                .map(|console| console.get_error_output());
            redirected.unwrap_or(output)
        };

        let formatter: std::rc::Rc<std::cell::RefCell<DebugFormatterHelper>> = self
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
                let command: Vec<BString> = cmd
                    .iter()
                    .map(|element| match element {
                        ProcessHelperCmdElement::String(s) => s.clone(),
                        ProcessHelperCmdElement::Process(_) => unreachable!(),
                    })
                    .collect();
                process = Process::new(command, None, None, PhpMixed::Null, Some(60.0))?;
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
                anyhow::bail!(InvalidArgumentException::new(
                    "Invalid command provided to \"ProcessHelper::run()\": the command should be an array whose first element is either the path to the binary to run or a \"Process\" object.",
                ));
            }
        }

        if verbosity <= output.borrow().get_verbosity() {
            let started = Self::formatter_start(
                &formatter,
                BStr::new(&spl_object_hash(&process)),
                Self::escape_string(process.get_command_line()).as_ref(),
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
        let env: indexmap::IndexMap<BString, PhpMixed> = cmd
            .iter()
            .enumerate()
            .filter_map(|(i, element)| match element {
                ProcessHelperCmdElement::String(s) => {
                    Some((bconcat!(i), PhpMixed::string(s.clone())))
                }
                ProcessHelperCmdElement::Process(_) => None,
            })
            .collect();
        let callback: Option<Box<dyn FnMut(&BStr, &BStr) -> anyhow::Result<bool>>> =
            callback.map(|mut cb| {
                Box::new(
                    move |r#type: &BStr, buffer: &BStr| -> anyhow::Result<bool> {
                        cb(r#type, buffer);
                        Ok(false)
                    },
                ) as Box<dyn FnMut(&BStr, &BStr) -> anyhow::Result<bool>>
            });
        process.run(callback, env)?;

        if verbosity <= output.borrow().get_verbosity() {
            let message = if process.is_successful() {
                BString::from("Command ran successfully")
            } else {
                bconcat!(
                    process
                        .get_exit_code()
                        .map(|code| bconcat!(code))
                        .unwrap_or_default(),
                    " Command did not run successfully"
                )
            };
            let stopped = Self::formatter_stop(
                &formatter,
                BStr::new(&spl_object_hash(&process)),
                message.as_ref(),
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
                &[bconcat!("<error>", Self::escape_string(error), "</error>")],
                output_interface::OUTPUT_NORMAL,
            );
        }

        Ok(process)
    }

    /// Runs the process.
    ///
    /// This is identical to run() except that an exception is thrown if the process
    /// exits with a non-zero exit code.
    pub fn must_run(
        &self,
        output: std::rc::Rc<std::cell::RefCell<dyn OutputInterface>>,
        cmd: ProcessHelperCmd,
        error: Option<&BStr>,
        callback: Option<Box<dyn FnMut(&BStr, &BStr)>>,
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
        output: std::rc::Rc<std::cell::RefCell<dyn OutputInterface>>,
        process: &Process,
        mut callback: Option<Box<dyn FnMut(&BStr, &BStr)>>,
    ) -> Box<dyn FnMut(&BStr, &BStr)> {
        // PHP: `if ($output instanceof ConsoleOutputInterface) { $output =
        // $output->getErrorOutput(); }`. ConsoleOutput is the only OutputInterface
        // implementor that also implements ConsoleOutputInterface, so the check
        // reduces to a downcast to the concrete type.
        let output: std::rc::Rc<std::cell::RefCell<dyn OutputInterface>> = {
            let redirected = AsAny::as_any(&*output.borrow())
                .downcast_ref::<crate::output::ConsoleOutput>()
                .map(|console| console.get_error_output());
            redirected.unwrap_or(output)
        };

        let formatter: std::rc::Rc<std::cell::RefCell<DebugFormatterHelper>> = self
            .get_helper_set()
            .unwrap()
            .borrow()
            .get_debug_formatter();

        let object_hash = spl_object_hash(process);

        Box::new(move |r#type: &BStr, buffer: &BStr| {
            let progressed = Self::formatter_progress(
                &formatter,
                BStr::new(&object_hash),
                Self::escape_string(buffer).as_ref(),
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

    fn escape_string(str: impl AsRef<[u8]>) -> BString {
        str_replace("<", "\\<", str)
    }

    fn formatter_start(
        formatter: &std::rc::Rc<std::cell::RefCell<DebugFormatterHelper>>,
        id: &BStr,
        message: &BStr,
    ) -> BString {
        formatter.borrow_mut().start(id, message, "RUN".into())
    }

    fn formatter_stop(
        formatter: &std::rc::Rc<std::cell::RefCell<DebugFormatterHelper>>,
        id: &BStr,
        message: &BStr,
        successful: bool,
    ) -> BString {
        formatter
            .borrow_mut()
            .stop(id, message, successful, "RES".into())
    }

    fn formatter_progress(
        formatter: &std::rc::Rc<std::cell::RefCell<DebugFormatterHelper>>,
        id: &BStr,
        buffer: &BStr,
        error: bool,
    ) -> BString {
        formatter
            .borrow_mut()
            .progress(id, buffer, error, "OUT".into(), "ERR".into())
    }
}

impl HelperInterface for ProcessHelper {
    fn set_helper_set(&mut self, helper_set: Option<std::rc::Rc<std::cell::RefCell<HelperSet>>>) {
        self.inner.set_helper_set(helper_set);
    }

    fn get_helper_set(&self) -> Option<std::rc::Rc<std::cell::RefCell<HelperSet>>> {
        self.inner.get_helper_set()
    }

    fn get_name(&self) -> BString {
        "process".into()
    }
}
