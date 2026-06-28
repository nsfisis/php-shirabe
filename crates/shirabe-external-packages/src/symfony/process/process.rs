//! ref: composer/vendor/symfony/process/Process.php

use crate::symfony::process::exception::invalid_argument_exception::InvalidArgumentException;
use crate::symfony::process::exception::logic_exception::LogicException;
use crate::symfony::process::exception::process_failed_exception::ProcessFailedException;
use crate::symfony::process::exception::process_signaled_exception::ProcessSignaledException;
use crate::symfony::process::exception::process_timed_out_exception::ProcessTimedOutException;
use crate::symfony::process::exception::runtime_exception::RuntimeException;
use crate::symfony::process::executable_finder::ExecutableFinder;
use crate::symfony::process::pipes::pipes_interface::PipesInterface;
use crate::symfony::process::pipes::unix_pipes::UnixPipes;
use crate::symfony::process::pipes::windows_pipes::WindowsPipes;
use crate::symfony::process::process_utils::ProcessUtils;
use indexmap::IndexMap;
use shirabe_php_shim::{Descriptor, PhpMixed, PhpResource};
use std::sync::OnceLock;

/// A user-supplied callback invoked with the output type ("out"/"err") and a chunk of output.
pub type UserCallback = Box<dyn FnMut(&str, &str) -> bool>;

/// The callback built by `build_callback`. It receives the owning Process so it can append output
/// to the internal buffers, mirroring the `$this`-capturing closure produced in PHP.
type ProcessCallback = Box<dyn FnMut(&mut Process, &str, &str) -> bool>;

/// PHP `$this->commandline` is `array|string`.
#[derive(Debug, Clone)]
enum CommandLine {
    Array(Vec<String>),
    String(String),
}

/// Process is a thin wrapper around proc_* functions to easily
/// start independent PHP processes.
pub struct Process {
    callback: Option<ProcessCallback>,
    has_callback: bool,
    commandline: CommandLine,
    cwd: Option<String>,
    env: IndexMap<String, PhpMixed>,
    input: PhpMixed,
    starttime: Option<f64>,
    last_output_time: Option<f64>,
    timeout: Option<f64>,
    idle_timeout: Option<f64>,
    exitcode: Option<i64>,
    fallback_status: IndexMap<String, PhpMixed>,
    process_information: Option<IndexMap<String, PhpMixed>>,
    output_disabled: bool,
    stdout: Option<PhpResource>,
    stderr: Option<PhpResource>,
    process: Option<PhpResource>,
    status: String,
    incremental_output_offset: i64,
    incremental_error_output_offset: i64,
    tty: bool,
    pty: bool,
    options: IndexMap<String, PhpMixed>,
    use_file_handles: bool,
    process_pipes: Option<Box<dyn PipesInterface>>,
    latest_signal: Option<i64>,
    cached_exit_code: Option<i64>,
}

impl std::fmt::Debug for Process {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Process")
            .field("commandline", &self.commandline)
            .field("cwd", &self.cwd)
            .field("status", &self.status)
            .field("exitcode", &self.exitcode)
            .finish_non_exhaustive()
    }
}

fn descriptor(items: &[&str]) -> Descriptor {
    match items {
        ["pipe", mode] => Descriptor::Pipe(mode.to_string()),
        ["file", path, mode] => Descriptor::File(path.to_string(), mode.to_string()),
        ["pty"] => Descriptor::Pty,
        _ => panic!("unsupported descriptor spec: {:?}", items),
    }
}

/// PHP `(string)` cast for an environment value or stream payload.
fn to_php_string(value: &PhpMixed) -> String {
    match value {
        PhpMixed::String(s) => s.clone(),
        PhpMixed::Int(i) => i.to_string(),
        PhpMixed::Float(f) => f.to_string(),
        PhpMixed::Bool(b) => {
            if *b {
                "1".to_string()
            } else {
                String::new()
            }
        }
        _ => String::new(),
    }
}

impl Process {
    pub const ERR: &'static str = "err";
    pub const OUT: &'static str = "out";

    pub const STATUS_READY: &'static str = "ready";
    pub const STATUS_STARTED: &'static str = "started";
    pub const STATUS_TERMINATED: &'static str = "terminated";

    pub const STDIN: i64 = 0;
    pub const STDOUT: i64 = 1;
    pub const STDERR: i64 = 2;

    // Timeout Precision in seconds.
    pub const TIMEOUT_PRECISION: f64 = 0.2;

    pub const ITER_NON_BLOCKING: i64 = 1;
    pub const ITER_KEEP_OUTPUT: i64 = 2;
    pub const ITER_SKIP_OUT: i64 = 4;
    pub const ITER_SKIP_ERR: i64 = 8;

    /// Exit codes translation table.
    fn exit_code_text(code: i64) -> Option<&'static str> {
        Some(match code {
            0 => "OK",
            1 => "General error",
            2 => "Misuse of shell builtins",

            126 => "Invoked command cannot execute",
            127 => "Command not found",
            128 => "Invalid exit argument",

            // signals
            129 => "Hangup",
            130 => "Interrupt",
            131 => "Quit and dump core",
            132 => "Illegal instruction",
            133 => "Trace/breakpoint trap",
            134 => "Process aborted",
            135 => "Bus error: \"access to undefined portion of memory object\"",
            136 => "Floating point exception: \"erroneous arithmetic operation\"",
            137 => "Kill (terminate immediately)",
            138 => "User-defined 1",
            139 => "Segmentation violation",
            140 => "User-defined 2",
            141 => "Write to pipe with no one reading",
            142 => "Signal raised by alarm",
            143 => "Termination (request to terminate)",
            // 144 - not defined
            145 => "Child process terminated, stopped (or continued*)",
            146 => "Continue if stopped",
            147 => "Stop executing temporarily",
            148 => "Terminal stop signal",
            149 => "Background process attempting to read from tty (\"in\")",
            150 => "Background process attempting to write to tty (\"out\")",
            151 => "Urgent data available on socket",
            152 => "CPU time limit exceeded",
            153 => "File size limit exceeded",
            154 => "Signal raised by timer counting virtual time: \"virtual timer expired\"",
            155 => "Profiling timer expired",
            // 156 - not defined
            157 => "Pollable event",
            // 158 - not defined
            159 => "Bad syscall",
            _ => return None,
        })
    }

    fn empty() -> Self {
        let mut options = IndexMap::new();
        options.insert("suppress_errors".to_string(), PhpMixed::Bool(true));
        options.insert("bypass_shell".to_string(), PhpMixed::Bool(true));

        Self {
            callback: None,
            has_callback: false,
            commandline: CommandLine::Array(Vec::new()),
            cwd: None,
            env: IndexMap::new(),
            input: PhpMixed::Null,
            starttime: None,
            last_output_time: None,
            timeout: None,
            idle_timeout: None,
            exitcode: None,
            fallback_status: IndexMap::new(),
            process_information: None,
            output_disabled: false,
            stdout: None,
            stderr: None,
            process: None,
            status: Self::STATUS_READY.to_string(),
            incremental_output_offset: 0,
            incremental_error_output_offset: 0,
            tty: false,
            pty: false,
            options,
            use_file_handles: false,
            process_pipes: None,
            latest_signal: None,
            cached_exit_code: None,
        }
    }

    pub fn new(
        command: Vec<String>,
        cwd: Option<String>,
        env: Option<IndexMap<String, String>>,
        input: PhpMixed,
        timeout: Option<f64>,
    ) -> anyhow::Result<Self> {
        if !shirabe_php_shim::function_exists("proc_open") {
            return Err(LogicException::new(
                "The Process class relies on proc_open, which is not available on your PHP installation.".to_string(),
            )
            .into());
        }

        let mut this = Self::empty();
        this.commandline = CommandLine::Array(command);
        this.cwd = cwd;

        // on Windows, if the cwd changed via chdir(), proc_open defaults to the dir where PHP was started
        // PHP: null === $this->cwd && (\defined('ZEND_THREAD_SAFE') || '\\' === \DIRECTORY_SEPARATOR)
        // `\defined('ZEND_THREAD_SAFE')` is unconditionally true in modern PHP (the constant always
        // exists; its value merely reflects the NTS/ZTS build), so the disjunction is always true and
        // the cwd is defaulted to getcwd() whenever it was null.
        if this.cwd.is_none() {
            this.cwd = shirabe_php_shim::getcwd();
        }
        if let Some(env) = env {
            this.set_env(
                env.into_iter()
                    .map(|(k, v)| (k, PhpMixed::String(v)))
                    .collect(),
            );
        }

        this.set_input(input)?;
        this.set_timeout(timeout)?;
        this.use_file_handles = shirabe_php_shim::DIRECTORY_SEPARATOR == "\\";
        this.pty = false;

        Ok(this)
    }

    /// Creates a Process instance as a command-line to be run in a shell wrapper.
    pub fn from_shell_commandline(
        command: &str,
        cwd: Option<&str>,
        env: Option<IndexMap<String, String>>,
        input: PhpMixed,
        timeout: Option<f64>,
    ) -> anyhow::Result<Self> {
        let mut process = Self::new(Vec::new(), cwd.map(String::from), env, input, timeout)?;
        process.commandline = CommandLine::String(command.to_string());

        Ok(process)
    }

    pub fn __sleep(&self) -> anyhow::Result<Vec<String>> {
        Err(shirabe_php_shim::BadMethodCallException {
            message: "Cannot serialize Symfony\\Component\\Process\\Process".to_string(),
            code: 0,
        }
        .into())
    }

    pub fn __wakeup(&self) -> anyhow::Result<()> {
        Err(shirabe_php_shim::BadMethodCallException {
            message: "Cannot unserialize Symfony\\Component\\Process\\Process".to_string(),
            code: 0,
        }
        .into())
    }

    /// Runs the process.
    pub fn run(
        &mut self,
        callback: Option<UserCallback>,
        env: IndexMap<String, PhpMixed>,
    ) -> anyhow::Result<i64> {
        self.start(callback, env)?;

        self.wait(None)
    }

    /// Runs the process and throws if it exits with a non-zero exit code.
    pub fn must_run(
        &mut self,
        callback: Option<UserCallback>,
        env: IndexMap<String, PhpMixed>,
    ) -> anyhow::Result<&mut Self> {
        if 0 != self.run(callback, env)? {
            return Err(ProcessFailedException::new(self)?.into());
        }

        Ok(self)
    }

    /// Starts the process and returns after writing the input to STDIN.
    pub fn start(
        &mut self,
        callback: Option<UserCallback>,
        mut env: IndexMap<String, PhpMixed>,
    ) -> anyhow::Result<()> {
        if self.is_running() {
            return Err(RuntimeException::new("Process is already running.".to_string()).into());
        }

        self.reset_process_data();
        self.starttime = Some(shirabe_php_shim::microtime());
        self.last_output_time = self.starttime;
        let has_callback = callback.is_some();
        self.callback = Some(self.build_callback(callback));
        self.has_callback = has_callback;
        let mut descriptors = self.get_descriptors();

        if !self.env.is_empty() {
            // non-Windows: $env += $this->env;
            for (k, v) in &self.env {
                env.entry(k.clone()).or_insert_with(|| v.clone());
            }
        }

        for (k, v) in self.get_default_env() {
            env.entry(k).or_insert(v);
        }

        let mut commandline = match &self.commandline {
            CommandLine::Array(args) => {
                let mut cmd = args
                    .iter()
                    .map(|a| self.escape_argument(Some(a)))
                    .collect::<Vec<_>>()
                    .join(" ");

                if shirabe_php_shim::DIRECTORY_SEPARATOR != "\\" {
                    // exec is mandatory to deal with sending a signal to the process
                    cmd = format!("exec {}", cmd);
                }
                cmd
            }
            CommandLine::String(s) => self.replace_placeholders(s, &env)?,
        };

        if shirabe_php_shim::DIRECTORY_SEPARATOR == "\\" {
            commandline = self.prepare_windows_command_line(&commandline, &mut env)?;
        } else if !self.use_file_handles && self.is_sigchild_enabled() {
            // last exit code is output on the fourth pipe and caught to work around --enable-sigchild
            descriptors.push(descriptor(&["pipe", "w"]));

            commandline = format!("{{ ({}) <&3 3<&- 3>/dev/null & }} 3<&0;", commandline);
            commandline.push_str(
                "pid=$!; echo $pid >&3; wait $pid 2>/dev/null; code=$?; echo $code >&3; exit $code",
            );

            // Workaround for the bug, when PTS functionality is enabled.
            let _pts_workaround = shirabe_php_shim::fopen("Process.php", "r");
        }

        let mut env_pairs: Vec<String> = Vec::new();
        for (k, v) in &env {
            let is_false = matches!(v, PhpMixed::Bool(false));
            if !is_false && !["argc", "argv", "ARGC", "ARGV"].contains(&k.as_str()) {
                env_pairs.push(format!("{}={}", k, to_php_string(v)));
            }
        }

        if !self
            .cwd
            .as_deref()
            .map(shirabe_php_shim::is_dir)
            .unwrap_or(false)
        {
            return Err(RuntimeException::new(format!(
                "The provided cwd \"{}\" does not exist.",
                self.cwd.as_deref().unwrap_or("")
            ))
            .into());
        }

        let cwd = self.cwd.clone();
        let options = self.options.clone();
        let process = {
            let pipes = self.process_pipes.as_mut().unwrap().pipes_mut();
            shirabe_php_shim::proc_open(
                &commandline,
                &descriptors,
                pipes,
                cwd.as_deref(),
                Some(&env_pairs),
                Some(&options),
            )
        };
        self.process = process.ok();

        if self.process.is_none() {
            return Err(
                RuntimeException::new("Unable to launch a new process.".to_string()).into(),
            );
        }
        self.status = Self::STATUS_STARTED.to_string();

        if descriptors.len() > 3 {
            let pipe3 = self
                .process_pipes
                .as_ref()
                .unwrap()
                .pipes()
                .get(&3)
                .cloned();
            let pid = pipe3
                .and_then(|p| shirabe_php_shim::fgets(&p, None))
                .map(|s| s.trim().parse::<i64>().unwrap_or(0))
                .unwrap_or(0);
            self.fallback_status
                .insert("pid".to_string(), PhpMixed::Int(pid));
        }

        if self.tty {
            return Ok(());
        }

        self.update_status(false);
        self.check_timeout()?;
        Ok(())
    }

    /// Restarts the process. The process is cloned before being started.
    pub fn restart(
        &mut self,
        callback: Option<UserCallback>,
        env: IndexMap<String, PhpMixed>,
    ) -> anyhow::Result<Process> {
        if self.is_running() {
            return Err(RuntimeException::new("Process is already running.".to_string()).into());
        }

        let mut process = self.clone_process();
        process.start(callback, env)?;

        Ok(process)
    }

    /// Waits for the process to terminate.
    pub fn wait(&mut self, callback: Option<UserCallback>) -> anyhow::Result<i64> {
        self.require_process_is_started("wait")?;

        self.update_status(false);

        if let Some(callback) = callback {
            if !self.process_pipes.as_ref().unwrap().have_read_support() {
                self.stop(0.0, None);
                return Err(LogicException::new(
                    "Pass the callback to the \"Process::start\" method or call enableOutput to use a callback with \"Process::wait\".".to_string(),
                )
                .into());
            }
            self.callback = Some(self.build_callback(Some(callback)));
        }

        loop {
            self.check_timeout()?;
            let running = self.is_running()
                && (shirabe_php_shim::DIRECTORY_SEPARATOR == "\\"
                    || self.process_pipes.as_ref().unwrap().are_open());
            self.read_pipes(
                running,
                shirabe_php_shim::DIRECTORY_SEPARATOR != "\\" || !running,
            );
            if !running {
                break;
            }
        }

        while self.is_running() {
            self.check_timeout()?;
            shirabe_php_shim::usleep(1000);
        }

        let signaled = self
            .process_information
            .as_ref()
            .and_then(|i| i.get("signaled"))
            .map(shirabe_php_shim::php_truthy)
            .unwrap_or(false);
        let termsig = self
            .process_information
            .as_ref()
            .and_then(|i| i.get("termsig"))
            .and_then(|v| v.as_int());
        if signaled && termsig != self.latest_signal {
            return Err(ProcessSignaledException::new(self)?.into());
        }

        Ok(self.exitcode.unwrap_or(0))
    }

    /// Waits until the callback returns true.
    pub fn wait_until(&mut self, callback: UserCallback) -> anyhow::Result<bool> {
        self.require_process_is_started("waitUntil")?;
        self.update_status(false);

        if !self.process_pipes.as_ref().unwrap().have_read_support() {
            self.stop(0.0, None);
            return Err(LogicException::new(
                "Pass the callback to the \"Process::start\" method or call enableOutput to use a callback with \"Process::waitUntil\".".to_string(),
            )
            .into());
        }
        let mut callback = self.build_callback(Some(callback));

        let mut ready = false;
        loop {
            self.check_timeout()?;
            let running = if shirabe_php_shim::DIRECTORY_SEPARATOR == "\\" {
                self.is_running()
            } else {
                self.process_pipes.as_ref().unwrap().are_open()
            };
            let output = self.process_pipes.as_mut().unwrap().read_and_write(
                running,
                shirabe_php_shim::DIRECTORY_SEPARATOR != "\\" || !running,
            );

            for (r#type, data) in output {
                if r#type != 3 {
                    let r = callback(
                        self,
                        if Self::STDOUT == r#type {
                            Self::OUT
                        } else {
                            Self::ERR
                        },
                        &data,
                    );
                    ready = r || ready;
                } else if !self.fallback_status.contains_key("signaled") {
                    self.fallback_status.insert(
                        "exitcode".to_string(),
                        PhpMixed::Int(data.trim().parse().unwrap_or(0)),
                    );
                }
            }
            if ready {
                return Ok(true);
            }
            if !running {
                return Ok(false);
            }

            shirabe_php_shim::usleep(1000);
        }
    }

    /// Returns the Pid (process identifier), if applicable.
    pub fn get_pid(&mut self) -> Option<i64> {
        if self.is_running() {
            self.process_information
                .as_ref()
                .and_then(|i| i.get("pid"))
                .and_then(|v| v.as_int())
        } else {
            None
        }
    }

    /// Sends a POSIX signal to the process.
    pub fn signal(&mut self, signal: i64) -> anyhow::Result<&mut Self> {
        self.do_signal(signal, true)?;

        Ok(self)
    }

    /// Disables fetching output and error output from the underlying process.
    pub fn disable_output(&mut self) -> anyhow::Result<&mut Self> {
        if self.is_running() {
            return Err(RuntimeException::new(
                "Disabling output while the process is running is not possible.".to_string(),
            )
            .into());
        }
        if self.idle_timeout.is_some() {
            return Err(LogicException::new(
                "Output cannot be disabled while an idle timeout is set.".to_string(),
            )
            .into());
        }

        self.output_disabled = true;

        Ok(self)
    }

    /// Enables fetching output and error output from the underlying process.
    pub fn enable_output(&mut self) -> anyhow::Result<&mut Self> {
        if self.is_running() {
            return Err(RuntimeException::new(
                "Enabling output while the process is running is not possible.".to_string(),
            )
            .into());
        }

        self.output_disabled = false;

        Ok(self)
    }

    pub fn is_output_disabled(&self) -> bool {
        self.output_disabled
    }

    /// Returns the current output of the process (STDOUT).
    pub fn get_output(&mut self) -> anyhow::Result<String> {
        self.read_pipes_for_output("getOutput", false)?;

        Ok(
            shirabe_php_shim::stream_get_contents3(self.stdout.as_ref().unwrap(), -1, 0)
                .unwrap_or_default(),
        )
    }

    /// Returns the output incrementally.
    pub fn get_incremental_output(&mut self) -> anyhow::Result<String> {
        self.read_pipes_for_output("getIncrementalOutput", false)?;

        let latest = shirabe_php_shim::stream_get_contents3(
            self.stdout.as_ref().unwrap(),
            -1,
            self.incremental_output_offset,
        );
        self.incremental_output_offset =
            shirabe_php_shim::ftell(self.stdout.as_ref().unwrap()).unwrap_or(0);

        Ok(latest.unwrap_or_default())
    }

    /// Returns an iterator to the output of the process, with the output type as keys.
    ///
    /// PHP returns a `\Generator`; lacking generators, this collects the yielded chunks eagerly.
    pub fn get_iterator(&mut self, flags: i64) -> anyhow::Result<Vec<(String, String)>> {
        self.read_pipes_for_output("getIterator", false)?;

        let clear_output = (Self::ITER_KEEP_OUTPUT & flags) == 0;
        let blocking = (Self::ITER_NON_BLOCKING & flags) == 0;
        let yield_out = (Self::ITER_SKIP_OUT & flags) == 0;
        let yield_err = (Self::ITER_SKIP_ERR & flags) == 0;

        let mut yields = Vec::new();
        while self.callback.is_some()
            || (yield_out && !shirabe_php_shim::feof(self.stdout.as_ref().unwrap()))
            || (yield_err && !shirabe_php_shim::feof(self.stderr.as_ref().unwrap()))
        {
            let mut got_out = false;
            let mut got_err = false;

            if yield_out {
                let out = shirabe_php_shim::stream_get_contents3(
                    self.stdout.as_ref().unwrap(),
                    -1,
                    self.incremental_output_offset,
                )
                .unwrap_or_default();

                if !out.is_empty() {
                    got_out = true;
                    if clear_output {
                        self.clear_output();
                    } else {
                        self.incremental_output_offset =
                            shirabe_php_shim::ftell(self.stdout.as_ref().unwrap()).unwrap_or(0);
                    }

                    yields.push((Self::OUT.to_string(), out));
                }
            }

            if yield_err {
                let err = shirabe_php_shim::stream_get_contents3(
                    self.stderr.as_ref().unwrap(),
                    -1,
                    self.incremental_error_output_offset,
                )
                .unwrap_or_default();

                if !err.is_empty() {
                    got_err = true;
                    if clear_output {
                        self.clear_error_output();
                    } else {
                        self.incremental_error_output_offset =
                            shirabe_php_shim::ftell(self.stderr.as_ref().unwrap()).unwrap_or(0);
                    }

                    yields.push((Self::ERR.to_string(), err));
                }
            }

            if !blocking && !got_out && !got_err {
                yields.push((Self::OUT.to_string(), String::new()));
            }

            self.check_timeout()?;
            self.read_pipes_for_output("getIterator", blocking)?;
        }

        Ok(yields)
    }

    /// Clears the process output.
    pub fn clear_output(&mut self) -> &mut Self {
        shirabe_php_shim::ftruncate(self.stdout.as_ref().unwrap(), 0);
        shirabe_php_shim::fseek(self.stdout.as_ref().unwrap(), 0, shirabe_php_shim::SEEK_SET);
        self.incremental_output_offset = 0;

        self
    }

    /// Returns the current error output of the process (STDERR).
    pub fn get_error_output(&mut self) -> anyhow::Result<String> {
        self.read_pipes_for_output("getErrorOutput", false)?;

        Ok(
            shirabe_php_shim::stream_get_contents3(self.stderr.as_ref().unwrap(), -1, 0)
                .unwrap_or_default(),
        )
    }

    /// Returns the errorOutput incrementally.
    pub fn get_incremental_error_output(&mut self) -> anyhow::Result<String> {
        self.read_pipes_for_output("getIncrementalErrorOutput", false)?;

        let latest = shirabe_php_shim::stream_get_contents3(
            self.stderr.as_ref().unwrap(),
            -1,
            self.incremental_error_output_offset,
        );
        self.incremental_error_output_offset =
            shirabe_php_shim::ftell(self.stderr.as_ref().unwrap()).unwrap_or(0);

        Ok(latest.unwrap_or_default())
    }

    /// Clears the process error output.
    pub fn clear_error_output(&mut self) -> &mut Self {
        shirabe_php_shim::ftruncate(self.stderr.as_ref().unwrap(), 0);
        shirabe_php_shim::fseek(self.stderr.as_ref().unwrap(), 0, shirabe_php_shim::SEEK_SET);
        self.incremental_error_output_offset = 0;

        self
    }

    /// Returns the exit code returned by the process.
    pub fn get_exit_code(&mut self) -> Option<i64> {
        self.update_status(false);

        self.exitcode
    }

    /// Returns a string representation for the exit code returned by the process.
    pub fn get_exit_code_text(&mut self) -> Option<String> {
        let exitcode = self.get_exit_code()?;

        Some(
            Self::exit_code_text(exitcode)
                .unwrap_or("Unknown error")
                .to_string(),
        )
    }

    /// Checks if the process ended successfully.
    pub fn is_successful(&mut self) -> bool {
        self.get_exit_code() == Some(0)
    }

    /// Returns true if the child process has been terminated by an uncaught signal.
    pub fn has_been_signaled(&mut self) -> anyhow::Result<bool> {
        self.require_process_is_terminated("hasBeenSignaled")?;

        Ok(self
            .process_information
            .as_ref()
            .and_then(|i| i.get("signaled"))
            .map(shirabe_php_shim::php_truthy)
            .unwrap_or(false))
    }

    /// Returns the number of the signal that caused the child process to terminate.
    pub fn get_term_signal(&mut self) -> anyhow::Result<i64> {
        self.require_process_is_terminated("getTermSignal")?;

        let termsig = self
            .process_information
            .as_ref()
            .and_then(|i| i.get("termsig"))
            .and_then(|v| v.as_int());
        if self.is_sigchild_enabled() && termsig == Some(-1) {
            return Err(RuntimeException::new(
                "This PHP has been compiled with --enable-sigchild. Term signal cannot be retrieved.".to_string(),
            )
            .into());
        }

        Ok(termsig.unwrap_or(0))
    }

    /// Returns true if the child process has been stopped by a signal.
    pub fn has_been_stopped(&mut self) -> anyhow::Result<bool> {
        self.require_process_is_terminated("hasBeenStopped")?;

        Ok(self
            .process_information
            .as_ref()
            .and_then(|i| i.get("stopped"))
            .map(shirabe_php_shim::php_truthy)
            .unwrap_or(false))
    }

    /// Returns the number of the signal that caused the child process to stop.
    pub fn get_stop_signal(&mut self) -> anyhow::Result<i64> {
        self.require_process_is_terminated("getStopSignal")?;

        Ok(self
            .process_information
            .as_ref()
            .and_then(|i| i.get("stopsig"))
            .and_then(|v| v.as_int())
            .unwrap_or(0))
    }

    /// Checks if the process is currently running.
    pub fn is_running(&mut self) -> bool {
        if Self::STATUS_STARTED != self.status {
            return false;
        }

        self.update_status(false);

        self.process_information
            .as_ref()
            .and_then(|i| i.get("running"))
            .map(shirabe_php_shim::php_truthy)
            .unwrap_or(false)
    }

    /// Checks if the process has been started with no regard to the current state.
    pub fn is_started(&self) -> bool {
        Self::STATUS_READY != self.status
    }

    /// Checks if the process is terminated.
    pub fn is_terminated(&mut self) -> bool {
        self.update_status(false);

        Self::STATUS_TERMINATED == self.status
    }

    /// Gets the process status (one of: ready, started, terminated).
    pub fn get_status(&mut self) -> String {
        self.update_status(false);

        self.status.clone()
    }

    /// Stops the process.
    pub fn stop(&mut self, timeout: f64, signal: Option<i64>) -> Option<i64> {
        let timeout_micro = shirabe_php_shim::microtime() + timeout;
        if self.is_running() {
            // given SIGTERM may not be defined and that "proc_terminate" uses the constant value
            // and not the constant itself, we use the same here
            let _ = self.do_signal(15, false);
            loop {
                shirabe_php_shim::usleep(1000);
                if !(self.is_running() && shirabe_php_shim::microtime() < timeout_micro) {
                    break;
                }
            }

            if self.is_running() {
                // Avoid exception here: process is supposed to be running, but it might have
                // stopped just after this line. Silently discard the error.
                let _ = self.do_signal(signal.filter(|&s| s != 0).unwrap_or(9), false);
            }
        }

        if self.is_running() {
            if self.fallback_status.contains_key("pid") {
                self.fallback_status.shift_remove("pid");

                return self.stop(0.0, signal);
            }
            self.close();
        }

        self.exitcode
    }

    /// Adds a line to the STDOUT stream.
    pub fn add_output(&mut self, line: &str) {
        self.last_output_time = Some(shirabe_php_shim::microtime());

        let stdout = self.stdout.as_ref().unwrap();
        shirabe_php_shim::fseek(stdout, 0, shirabe_php_shim::SEEK_END);
        shirabe_php_shim::fwrite(stdout, line, Some(line.len() as i64));
        shirabe_php_shim::fseek(
            stdout,
            self.incremental_output_offset,
            shirabe_php_shim::SEEK_SET,
        );
    }

    /// Adds a line to the STDERR stream.
    pub fn add_error_output(&mut self, line: &str) {
        self.last_output_time = Some(shirabe_php_shim::microtime());

        let stderr = self.stderr.as_ref().unwrap();
        shirabe_php_shim::fseek(stderr, 0, shirabe_php_shim::SEEK_END);
        shirabe_php_shim::fwrite(stderr, line, Some(line.len() as i64));
        shirabe_php_shim::fseek(
            stderr,
            self.incremental_error_output_offset,
            shirabe_php_shim::SEEK_SET,
        );
    }

    /// Gets the last output time in seconds.
    pub fn get_last_output_time(&self) -> Option<f64> {
        self.last_output_time
    }

    /// Gets the command line to be executed.
    pub fn get_command_line(&self) -> String {
        match &self.commandline {
            CommandLine::Array(args) => args
                .iter()
                .map(|a| self.escape_argument(Some(a)))
                .collect::<Vec<_>>()
                .join(" "),
            CommandLine::String(s) => s.clone(),
        }
    }

    /// Gets the process timeout in seconds (max. runtime).
    pub fn get_timeout(&self) -> Option<f64> {
        self.timeout
    }

    /// Gets the process idle timeout in seconds (max. time since last output).
    pub fn get_idle_timeout(&self) -> Option<f64> {
        self.idle_timeout
    }

    /// Sets the process timeout (max. runtime) in seconds.
    pub fn set_timeout(&mut self, timeout: Option<f64>) -> anyhow::Result<&mut Self> {
        self.timeout = self.validate_timeout(timeout)?;

        Ok(self)
    }

    /// Sets the process idle timeout (max. time since last output) in seconds.
    pub fn set_idle_timeout(&mut self, timeout: Option<f64>) -> anyhow::Result<&mut Self> {
        if timeout.is_some() && self.output_disabled {
            return Err(LogicException::new(
                "Idle timeout cannot be set while the output is disabled.".to_string(),
            )
            .into());
        }

        self.idle_timeout = self.validate_timeout(timeout)?;

        Ok(self)
    }

    /// Enables or disables the TTY mode.
    pub fn set_tty(&mut self, tty: bool) -> anyhow::Result<&mut Self> {
        if shirabe_php_shim::DIRECTORY_SEPARATOR == "\\" && tty {
            return Err(RuntimeException::new(
                "TTY mode is not supported on Windows platform.".to_string(),
            )
            .into());
        }

        if tty && !Self::is_tty_supported() {
            return Err(RuntimeException::new(
                "TTY mode requires /dev/tty to be read/writable.".to_string(),
            )
            .into());
        }

        self.tty = tty;

        Ok(self)
    }

    /// Checks if the TTY mode is enabled.
    pub fn is_tty(&self) -> bool {
        self.tty
    }

    /// Sets PTY mode.
    pub fn set_pty(&mut self, bool: bool) -> &mut Self {
        self.pty = bool;

        self
    }

    /// Returns PTY state.
    pub fn is_pty(&self) -> bool {
        self.pty
    }

    /// Gets the working directory.
    pub fn get_working_directory(&self) -> Option<String> {
        if self.cwd.is_none() {
            // getcwd() will return false if any one of the parent directories does not have
            // the readable or search mode set, even if the current directory does
            return shirabe_php_shim::getcwd().filter(|s| !s.is_empty());
        }

        self.cwd.clone()
    }

    /// Sets the current working directory.
    pub fn set_working_directory(&mut self, cwd: &str) -> &mut Self {
        self.cwd = Some(cwd.to_string());

        self
    }

    /// Gets the environment variables.
    pub fn get_env(&self) -> &IndexMap<String, PhpMixed> {
        &self.env
    }

    /// Sets the environment variables.
    pub fn set_env(&mut self, env: IndexMap<String, PhpMixed>) -> &mut Self {
        self.env = env;

        self
    }

    /// Gets the Process input.
    pub fn get_input(&self) -> &PhpMixed {
        &self.input
    }

    /// Sets the input.
    pub fn set_input(&mut self, input: PhpMixed) -> anyhow::Result<&mut Self> {
        if self.is_running() {
            return Err(LogicException::new(
                "Input cannot be set while the process is running.".to_string(),
            )
            .into());
        }

        self.input =
            ProcessUtils::validate_input("Symfony\\Component\\Process\\Process::setInput", input)?;

        Ok(self)
    }

    /// Performs a check between the timeout definition and the time the process started.
    pub fn check_timeout(&mut self) -> anyhow::Result<()> {
        if Self::STATUS_STARTED != self.status {
            return Ok(());
        }

        if let Some(timeout) = self.timeout
            && timeout < shirabe_php_shim::microtime() - self.starttime.unwrap_or(0.0)
        {
            self.stop(0.0, None);

            return Err(ProcessTimedOutException::new(
                self,
                ProcessTimedOutException::TYPE_GENERAL,
            )
            .into());
        }

        if let Some(idle_timeout) = self.idle_timeout
            && idle_timeout < shirabe_php_shim::microtime() - self.last_output_time.unwrap_or(0.0)
        {
            self.stop(0.0, None);

            return Err(
                ProcessTimedOutException::new(self, ProcessTimedOutException::TYPE_IDLE).into(),
            );
        }

        Ok(())
    }

    pub fn get_start_time(&self) -> anyhow::Result<f64> {
        if !self.is_started() {
            return Err(LogicException::new(
                "Start time is only available after process start.".to_string(),
            )
            .into());
        }

        Ok(self.starttime.unwrap())
    }

    /// Defines options to pass to the underlying proc_open().
    pub fn set_options(&mut self, options: IndexMap<String, PhpMixed>) -> anyhow::Result<()> {
        if self.is_running() {
            return Err(RuntimeException::new(
                "Setting options while the process is running is not possible.".to_string(),
            )
            .into());
        }

        let default_options = self.options.clone();
        let existing_options = [
            "blocking_pipes",
            "create_process_group",
            "create_new_console",
        ];

        for (key, value) in options {
            if !existing_options.contains(&key.as_str()) {
                self.options = default_options;
                return Err(LogicException::new(format!(
                    "Invalid option \"{}\" passed to \"Symfony\\Component\\Process\\Process::setOptions()\". Supported options are \"{}\".",
                    key,
                    existing_options.join("\", \"")
                ))
                .into());
            }
            self.options.insert(key, value);
        }

        Ok(())
    }

    /// Returns whether TTY is supported on the current operating system.
    pub fn is_tty_supported() -> bool {
        static IS_TTY_SUPPORTED: OnceLock<bool> = OnceLock::new();

        *IS_TTY_SUPPORTED.get_or_init(|| {
            let mut pipes = IndexMap::new();
            shirabe_php_shim::proc_open(
                "echo 1 >/dev/null",
                &[
                    descriptor(&["file", "/dev/tty", "r"]),
                    descriptor(&["file", "/dev/tty", "w"]),
                    descriptor(&["file", "/dev/tty", "w"]),
                ],
                &mut pipes,
                None,
                None,
                None,
            )
            .is_ok()
        })
    }

    /// Returns whether PTY is supported on the current operating system.
    pub fn is_pty_supported() -> bool {
        static RESULT: OnceLock<bool> = OnceLock::new();

        *RESULT.get_or_init(|| {
            if shirabe_php_shim::DIRECTORY_SEPARATOR == "\\" {
                return false;
            }

            let mut pipes = IndexMap::new();
            shirabe_php_shim::proc_open(
                "echo 1 >/dev/null",
                &[
                    descriptor(&["pty"]),
                    descriptor(&["pty"]),
                    descriptor(&["pty"]),
                ],
                &mut pipes,
                None,
                None,
                None,
            )
            .is_ok()
        })
    }

    /// Creates the descriptors needed by the proc_open.
    fn get_descriptors(&mut self) -> Vec<Descriptor> {
        // TODO(plugin): $this->input instanceof \Iterator -> rewind() is not modeled.
        if shirabe_php_shim::DIRECTORY_SEPARATOR == "\\" {
            self.process_pipes = Some(Box::new(WindowsPipes::new(
                self.input.clone(),
                !self.output_disabled || self.has_callback,
            )));
        } else {
            self.process_pipes = Some(Box::new(UnixPipes::new(
                Some(self.is_tty()),
                self.is_pty(),
                self.input.clone(),
                !self.output_disabled || self.has_callback,
            )));
        }

        self.process_pipes.as_mut().unwrap().get_descriptors()
    }

    /// Builds up the callback used by wait().
    fn build_callback(&self, callback: Option<UserCallback>) -> ProcessCallback {
        let mut callback = callback;
        if self.output_disabled {
            return Box::new(
                move |_this: &mut Process, r#type: &str, data: &str| -> bool {
                    match callback.as_mut() {
                        Some(cb) => cb(r#type, data),
                        None => false,
                    }
                },
            );
        }

        let out = Self::OUT;

        Box::new(
            move |this: &mut Process, r#type: &str, data: &str| -> bool {
                if out == r#type {
                    this.add_output(data);
                } else {
                    this.add_error_output(data);
                }

                match callback.as_mut() {
                    Some(cb) => cb(r#type, data),
                    None => false,
                }
            },
        )
    }

    /// Updates the status of the process, reads pipes.
    fn update_status(&mut self, blocking: bool) {
        if Self::STATUS_STARTED != self.status {
            return;
        }

        self.process_information = Some(shirabe_php_shim::proc_get_status(
            self.process.as_ref().unwrap(),
        ));
        let running = self
            .process_information
            .as_ref()
            .unwrap()
            .get("running")
            .map(shirabe_php_shim::php_truthy)
            .unwrap_or(false);

        // In PHP < 8.3, "proc_get_status" only returns the correct exit status on the first call.
        if shirabe_php_shim::PHP_VERSION_ID < 80300 {
            let exitcode = self
                .process_information
                .as_ref()
                .unwrap()
                .get("exitcode")
                .and_then(|v| v.as_int());
            if self.cached_exit_code.is_none() && !running && exitcode != Some(-1) {
                self.cached_exit_code = exitcode;
            }

            if let Some(cached) = self.cached_exit_code
                && !running
                && exitcode == Some(-1)
            {
                self.process_information
                    .as_mut()
                    .unwrap()
                    .insert("exitcode".to_string(), PhpMixed::Int(cached));
            }
        }

        self.read_pipes(
            running && blocking,
            shirabe_php_shim::DIRECTORY_SEPARATOR != "\\" || !running,
        );

        if !self.fallback_status.is_empty() && self.is_sigchild_enabled() {
            // processInformation = fallbackStatus + processInformation (fallback keys win)
            let mut merged = self.fallback_status.clone();
            for (k, v) in self.process_information.take().unwrap() {
                merged.entry(k).or_insert(v);
            }
            self.process_information = Some(merged);
        }

        if !running {
            self.close();
        }
    }

    /// Returns whether PHP has been compiled with the '--enable-sigchild' option or not.
    fn is_sigchild_enabled(&self) -> bool {
        static SIGCHILD: OnceLock<bool> = OnceLock::new();

        if let Some(v) = SIGCHILD.get() {
            return *v;
        }

        if !shirabe_php_shim::function_exists("phpinfo") {
            return *SIGCHILD.get_or_init(|| false);
        }

        shirabe_php_shim::ob_start();
        shirabe_php_shim::phpinfo(shirabe_php_shim::INFO_GENERAL);

        *SIGCHILD.get_or_init(|| {
            shirabe_php_shim::str_contains(
                &shirabe_php_shim::ob_get_clean().unwrap_or_default(),
                "--enable-sigchild",
            )
        })
    }

    /// Reads pipes for the freshest output.
    fn read_pipes_for_output(&mut self, caller: &str, blocking: bool) -> anyhow::Result<()> {
        if self.output_disabled {
            return Err(LogicException::new("Output has been disabled.".to_string()).into());
        }

        self.require_process_is_started(caller)?;

        self.update_status(blocking);
        Ok(())
    }

    /// Validates and returns the filtered timeout.
    fn validate_timeout(&self, timeout: Option<f64>) -> anyhow::Result<Option<f64>> {
        let timeout = timeout.unwrap_or(0.0);

        if timeout == 0.0 {
            Ok(None)
        } else if timeout < 0.0 {
            Err(InvalidArgumentException::new(
                "The timeout value must be a valid positive integer or float number.".to_string(),
            )
            .into())
        } else {
            Ok(Some(timeout))
        }
    }

    /// Reads pipes, executes callback.
    fn read_pipes(&mut self, blocking: bool, close: bool) {
        let result = self
            .process_pipes
            .as_mut()
            .unwrap()
            .read_and_write(blocking, close);

        let mut callback = self.callback.take();
        for (r#type, data) in result {
            if r#type != 3 {
                if let Some(cb) = callback.as_mut() {
                    cb(
                        self,
                        if Self::STDOUT == r#type {
                            Self::OUT
                        } else {
                            Self::ERR
                        },
                        &data,
                    );
                }
            } else if !self.fallback_status.contains_key("signaled") {
                self.fallback_status.insert(
                    "exitcode".to_string(),
                    PhpMixed::Int(data.trim().parse().unwrap_or(0)),
                );
            }
        }
        self.callback = callback;
    }

    /// Closes process resource, closes file handles, sets the exitcode.
    fn close(&mut self) -> i64 {
        if let Some(p) = self.process_pipes.as_mut() {
            p.close();
        }
        if self.process.is_some() {
            shirabe_php_shim::proc_close(self.process.as_ref().unwrap());
            self.process = None;
        }
        self.exitcode = self
            .process_information
            .as_ref()
            .and_then(|i| i.get("exitcode"))
            .and_then(|v| v.as_int());
        self.status = Self::STATUS_TERMINATED.to_string();

        if self.exitcode == Some(-1) {
            let signaled = self
                .process_information
                .as_ref()
                .and_then(|i| i.get("signaled"))
                .map(shirabe_php_shim::php_truthy)
                .unwrap_or(false);
            let termsig = self
                .process_information
                .as_ref()
                .and_then(|i| i.get("termsig"))
                .and_then(|v| v.as_int())
                .unwrap_or(0);
            if signaled && termsig > 0 {
                // if process has been signaled, no exitcode but a valid termsig, apply Unix convention
                self.exitcode = Some(128 + termsig);
            } else if self.is_sigchild_enabled()
                && let Some(i) = self.process_information.as_mut()
            {
                i.insert("signaled".to_string(), PhpMixed::Bool(true));
                i.insert("termsig".to_string(), PhpMixed::Int(-1));
            }
        }

        // Free memory from self-reference callback created by buildCallback
        self.callback = None;

        self.exitcode.unwrap_or(-1)
    }

    /// Resets data related to the latest run of the process.
    fn reset_process_data(&mut self) {
        self.starttime = None;
        self.callback = None;
        self.exitcode = None;
        self.fallback_status = IndexMap::new();
        self.process_information = None;
        // php://temp is an in-memory stream; fopen never fails for it.
        self.stdout = Some(
            shirabe_php_shim::fopen(&format!("php://temp/maxmemory:{}", 1024 * 1024), "w+")
                .unwrap(),
        );
        self.stderr = Some(
            shirabe_php_shim::fopen(&format!("php://temp/maxmemory:{}", 1024 * 1024), "w+")
                .unwrap(),
        );
        self.process = None;
        self.latest_signal = None;
        self.status = Self::STATUS_READY.to_string();
        self.incremental_output_offset = 0;
        self.incremental_error_output_offset = 0;
    }

    /// Sends a POSIX signal to the process.
    fn do_signal(&mut self, signal: i64, throw_exception: bool) -> anyhow::Result<bool> {
        let pid = match self.get_pid() {
            None => {
                if throw_exception {
                    return Err(LogicException::new(
                        "Cannot send signal on a non running process.".to_string(),
                    )
                    .into());
                }

                return Ok(false);
            }
            Some(pid) => pid,
        };

        if shirabe_php_shim::DIRECTORY_SEPARATOR == "\\" {
            let mut output: Vec<String> = Vec::new();
            let mut exit_code: i64 = 0;
            shirabe_php_shim::exec(
                &format!("taskkill /F /T /PID {} 2>&1", pid),
                Some(&mut output),
                Some(&mut exit_code),
            );
            if exit_code != 0 && self.is_running() {
                if throw_exception {
                    return Err(RuntimeException::new(format!(
                        "Unable to kill the process ({}).",
                        output.join(" ")
                    ))
                    .into());
                }

                return Ok(false);
            }
        } else {
            let ok;
            if !self.is_sigchild_enabled() {
                ok = shirabe_php_shim::proc_terminate(self.process.as_ref().unwrap(), signal);
            } else if shirabe_php_shim::function_exists("posix_kill") {
                ok = shirabe_php_shim::posix_kill(pid, signal);
            } else {
                let mut pipes = IndexMap::new();
                let opened = shirabe_php_shim::proc_open(
                    &format!("kill -{} {}", signal, pid),
                    &[
                        Descriptor::Inherit,
                        Descriptor::Inherit,
                        descriptor(&["pipe", "w"]),
                    ],
                    &mut pipes,
                    None,
                    None,
                    None,
                );
                ok = match opened {
                    Ok(_) => pipes
                        .get(&2)
                        .and_then(|p| shirabe_php_shim::fgets(p, None))
                        .is_none(),
                    Err(_) => false,
                };
            }
            if !ok {
                if throw_exception {
                    return Err(RuntimeException::new(format!(
                        "Error while sending signal \"{}\".",
                        signal
                    ))
                    .into());
                }

                return Ok(false);
            }
        }

        self.latest_signal = Some(signal);
        self.fallback_status
            .insert("signaled".to_string(), PhpMixed::Bool(true));
        self.fallback_status
            .insert("exitcode".to_string(), PhpMixed::Int(-1));
        self.fallback_status.insert(
            "termsig".to_string(),
            PhpMixed::Int(self.latest_signal.unwrap()),
        );

        Ok(true)
    }

    fn prepare_windows_command_line(
        &mut self,
        cmd: &str,
        env: &mut IndexMap<String, PhpMixed>,
    ) -> anyhow::Result<String> {
        let uid = shirabe_php_shim::uniqid("", true);
        let mut var_count = 0;
        let mut var_cache: IndexMap<String, String> = IndexMap::new();
        let cmd = shirabe_php_shim::preg_replace_callback(
            r#"/"(?:(
                [^"%!^]*+
                (?:
                    (?: !LF! | "(?:\^[%!^])?+" )
                    [^"%!^]*+
                )++
            ) | [^"]*+ )"/x"#,
            |m: &[Option<String>]| -> anyhow::Result<String> {
                let m0 = m.first().cloned().flatten().unwrap_or_default();
                let m1 = m.get(1).cloned().flatten();
                if m1.is_none() {
                    return Ok(m0);
                }
                if let Some(cached) = var_cache.get(&m0) {
                    return Ok(cached.clone());
                }
                let mut value = m1.unwrap();
                if value.contains('\0') {
                    value = value.replace('\0', "?");
                }
                if shirabe_php_shim::strpbrk(&value, "\"%!\n").is_none() {
                    return Ok(format!("\"{}\"", value));
                }

                for (from, to) in [
                    ("!LF!", "\n"),
                    ("\"^!\"", "!"),
                    ("\"^%\"", "%"),
                    ("\"^^\"", "^"),
                    ("\"\"", "\""),
                ] {
                    value = value.replace(from, to);
                }
                value = format!(
                    "\"{}\"",
                    shirabe_php_shim::preg_replace(r#"/(\\*)"/"#, "$1$1\\\"", &value)
                );
                var_count += 1;
                let var = format!("{}{}", uid, var_count);

                env.insert(var.clone(), PhpMixed::String(value));

                let replacement = format!("!{}!", var);
                var_cache.insert(m0, replacement.clone());
                Ok(replacement)
            },
            cmd,
        )?;

        static COM_SPEC: OnceLock<Option<String>> = OnceLock::new();
        let com_spec = COM_SPEC
            .get_or_init(|| {
                ExecutableFinder::new()
                    .find("cmd.exe", None, &[])
                    .map(|spec| {
                        format!(
                            "\"{}\"",
                            shirabe_php_shim::preg_replace(r#"{(\\*+)"}"#, "$1$1\\\"", &spec)
                        )
                    })
            })
            .clone();

        let mut cmd = format!(
            "{} /V:ON /E:ON /D /C ({})",
            com_spec.unwrap_or_else(|| "cmd".to_string()),
            cmd.replace('\n', " ")
        );
        for (offset, filename) in self.process_pipes.as_ref().unwrap().get_files() {
            cmd.push_str(&format!(" {}>\"{}\"", offset, filename));
        }

        Ok(cmd)
    }

    /// Ensures the process is running or terminated.
    fn require_process_is_started(&self, function_name: &str) -> anyhow::Result<()> {
        if !self.is_started() {
            return Err(LogicException::new(format!(
                "Process must be started before calling \"{}()\".",
                function_name
            ))
            .into());
        }
        Ok(())
    }

    /// Ensures the process is terminated.
    fn require_process_is_terminated(&mut self, function_name: &str) -> anyhow::Result<()> {
        if !self.is_terminated() {
            return Err(LogicException::new(format!(
                "Process must be terminated before calling \"{}()\".",
                function_name
            ))
            .into());
        }
        Ok(())
    }

    /// Escapes a string to be used as a shell argument.
    fn escape_argument(&self, argument: Option<&str>) -> String {
        let argument = match argument {
            None | Some("") => return "\"\"".to_string(),
            Some(a) => a,
        };
        if shirabe_php_shim::DIRECTORY_SEPARATOR != "\\" {
            return format!("'{}'", argument.replace('\'', "'\\''"));
        }
        let mut argument = argument.to_string();
        if argument.contains('\0') {
            argument = argument.replace('\0', "?");
        }
        if !shirabe_php_shim::preg_match(
            r#"/[()%!^"<>&|\s\[\]=;*?'$]/"#,
            &argument,
            &mut Vec::new(),
        ) {
            return argument;
        }
        argument = shirabe_php_shim::preg_replace(r"/(\\+)$/", "$1$1", &argument);

        let mut result = argument;
        for (from, to) in [
            ("\"", "\"\""),
            ("^", "\"^^\""),
            ("%", "\"^%\""),
            ("!", "\"^!\""),
            ("\n", "!LF!"),
        ] {
            result = result.replace(from, to);
        }
        format!("\"{}\"", result)
    }

    fn replace_placeholders(
        &self,
        commandline: &str,
        env: &IndexMap<String, PhpMixed>,
    ) -> anyhow::Result<String> {
        shirabe_php_shim::preg_replace_callback(
            r#"/"\$\{:([_a-zA-Z]+[_a-zA-Z0-9]*)\}"/"#,
            |matches: &[Option<String>]| -> anyhow::Result<String> {
                let key = matches.get(1).cloned().flatten().unwrap_or_default();
                match env.get(&key) {
                    None => Err(InvalidArgumentException::new(format!(
                        "Command line is missing a value for parameter \"{}\": {}",
                        key, commandline
                    ))
                    .into()),
                    Some(PhpMixed::Bool(false)) => Err(InvalidArgumentException::new(format!(
                        "Command line is missing a value for parameter \"{}\": {}",
                        key, commandline
                    ))
                    .into()),
                    Some(v) => Ok(self.escape_argument(Some(&to_php_string(v)))),
                }
            },
            commandline,
        )
    }

    fn get_default_env(&self) -> IndexMap<String, PhpMixed> {
        let env: IndexMap<String, String> = shirabe_php_shim::getenv_all()
            .map(|(k, v)| {
                (
                    k.to_string_lossy().into_owned(),
                    v.to_string_lossy().into_owned(),
                )
            })
            .collect();
        let server = shirabe_php_shim::PHP_SERVER.lock().unwrap();

        // non-Windows: array_intersect_key($env, $_SERVER) ?: $env
        let mut intersect: IndexMap<String, PhpMixed> = IndexMap::new();
        for (k, v) in &env {
            if server.get(k).is_some() {
                intersect.insert(k.clone(), PhpMixed::String(v.clone()));
            }
        }
        let env_map: IndexMap<String, PhpMixed> = if intersect.is_empty() {
            env.into_iter()
                .map(|(k, v)| (k, PhpMixed::String(v)))
                .collect()
        } else {
            intersect
        };

        // $_ENV + env_map
        let mut result: IndexMap<String, PhpMixed> = shirabe_php_shim::PHP_ENV
            .lock()
            .unwrap()
            .get_all()
            .map(|(k, v)| {
                (
                    k.to_string_lossy().into_owned(),
                    PhpMixed::String(v.to_string_lossy().into_owned()),
                )
            })
            .collect();
        for (k, v) in env_map {
            result.entry(k).or_insert(v);
        }
        result
    }

    /// Clone the process configuration, mirroring PHP `clone $this` followed by `__clone`
    /// (which calls resetProcessData). Runtime state is reset, not copied.
    fn clone_process(&self) -> Process {
        let mut process = Self::empty();
        process.has_callback = self.has_callback;
        process.commandline = self.commandline.clone();
        process.cwd = self.cwd.clone();
        process.env = self.env.clone();
        process.input = self.input.clone();
        process.timeout = self.timeout;
        process.idle_timeout = self.idle_timeout;
        process.output_disabled = self.output_disabled;
        process.tty = self.tty;
        process.pty = self.pty;
        process.options = self.options.clone();
        process.use_file_handles = self.use_file_handles;
        process
    }
}

impl Drop for Process {
    fn drop(&mut self) {
        if self
            .options
            .get("create_new_console")
            .map(shirabe_php_shim::php_truthy)
            .unwrap_or(false)
        {
            if let Some(p) = self.process_pipes.as_mut() {
                p.close();
            }
        } else {
            self.stop(0.0, None);
        }
    }
}
