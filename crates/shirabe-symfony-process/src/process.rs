//! ref: composer/vendor/symfony/process/Process.php

use crate::exception::{
    InvalidArgumentException, LogicException, ProcessSignaledException, ProcessTimedOutException,
    RuntimeException,
};
use crate::executable_finder::ExecutableFinder;
use crate::pipes::pipes_interface::PipesInterface;
use crate::pipes::unix_pipes::UnixPipes;
use crate::pipes::windows_pipes::WindowsPipes;
use crate::process_utils::ProcessUtils;
use bstr::{B, BString, ByteSlice};
use indexmap::IndexMap;
use shirabe_php_shim::{
    Descriptor, PHP_ENV, PHP_SERVER, PhpMixed, PhpResource, PregMatches, ProcStatus, SEEK_END,
    SEEK_SET, exec, fopen, fseek, fwrite, getcwd, getenv_all, is_dir, legacy_lossy, microtime,
    php_regex, preg_is_match, preg_replace, preg_replace_callback, proc_close, proc_get_status,
    proc_open, proc_terminate, stream_get_contents3, strpbrk, uniqid, usleep,
};
use std::sync::OnceLock;

/// A user-supplied callback invoked with the output type ("out"/"err") and a chunk of output. An
/// error it returns is the exception PHP lets unwind out of `readPipes()`.
pub type UserCallback = Box<dyn FnMut(&str, &str) -> anyhow::Result<bool>>;

/// The callback built by `build_callback`. It receives the owning Process so it can append output
/// to the internal buffers, mirroring the `$this`-capturing closure produced in PHP.
type ProcessCallback = Box<dyn FnMut(&mut Process, &str, &str) -> anyhow::Result<bool>>;

/// PHP `$this->commandline` is `array|string`.
#[derive(Debug, Clone)]
enum CommandLine {
    Array(Vec<String>),
    String(String),
}

/// Test-only behaviour for a Process fabricated via [`Process::__mock`]: `getOutput`/
/// `getErrorOutput`/`getExitCode`/`isSuccessful` return these fixed values instead of reading a
/// real subprocess. Mirrors PHPUnit's `getMockBuilder(Process::class)->disableOriginalConstructor()`
/// mocks used by the Composer test suite (e.g. `ZipDownloaderTest`). Held in [`Process::mock`];
/// always `None` in production.
#[derive(Debug, Clone)]
pub struct ProcessMock {
    pub exit_code: i64,
    pub stdout: String,
    pub stderr: String,
}

/// Process is a thin wrapper around proc_* functions to easily
/// start independent PHP processes.
pub struct Process {
    callback: Option<ProcessCallback>,
    /// The error the callback returned while the pipes were read, held until a fallible method
    /// can return it. The callback is not called again while one is held.
    ///
    /// TODO(error-model): PHP throws it out of whichever method read the pipes, including
    /// `isRunning()`, `isTerminated()`, `getStatus()` and `getExitCode()`. Those return no
    /// `Result` here, so an error raised under them surfaces at the next `start()`, `wait()` or
    /// output read instead.
    callback_error: Option<anyhow::Error>,
    commandline: CommandLine,
    cwd: Option<String>,
    env: IndexMap<BString, PhpMixed>,
    input: PhpMixed,
    starttime: Option<f64>,
    timeout: Option<f64>,
    exitcode: Option<i64>,
    process_information: Option<ProcStatus>,
    stdout: Option<PhpResource>,
    stderr: Option<PhpResource>,
    process: Option<PhpResource>,
    status: String,
    incremental_output_offset: i64,
    incremental_error_output_offset: i64,
    tty: bool,
    options: IndexMap<BString, PhpMixed>,
    process_pipes: Option<Box<dyn PipesInterface>>,
    latest_signal: Option<i64>,
    /// Test-only mock state. `None` in production; set via [`Process::__mock`] in tests.
    mock: Option<ProcessMock>,
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
        _ => panic!("unsupported descriptor spec: {:?}", items),
    }
}

/// PHP `(string)` cast for an environment value or stream payload.
fn to_php_string(value: &PhpMixed) -> String {
    match value {
        PhpMixed::String(s) => s.to_str_lossy().into_owned(),
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

    pub const STDOUT: i64 = 1;
    pub const STDERR: i64 = 2;

    // Timeout Precision in seconds.
    pub const TIMEOUT_PRECISION: f64 = 0.2;

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
        options.insert("suppress_errors".into(), PhpMixed::Bool(true));
        options.insert("bypass_shell".into(), PhpMixed::Bool(true));

        Self {
            callback: None,
            callback_error: None,
            commandline: CommandLine::Array(Vec::new()),
            cwd: None,
            env: IndexMap::new(),
            input: PhpMixed::Null,
            starttime: None,
            timeout: None,
            exitcode: None,
            process_information: None,
            stdout: None,
            stderr: None,
            process: None,
            status: Self::STATUS_READY.to_string(),
            incremental_output_offset: 0,
            incremental_error_output_offset: 0,
            tty: false,
            options,
            process_pipes: None,
            latest_signal: None,
            mock: None,
        }
    }

    /// For testing only. Builds an already-terminated mock Process whose getOutput/
    /// getErrorOutput/getExitCode/isSuccessful return the configured values, without spawning a
    /// real subprocess.
    pub fn __mock(mock: ProcessMock) -> Self {
        let mut this = Self::empty();
        this.status = Self::STATUS_TERMINATED.to_string();
        this.exitcode = Some(mock.exit_code);
        this.mock = Some(mock);
        this
    }

    pub fn new(
        command: Vec<String>,
        cwd: Option<String>,
        env: Option<IndexMap<String, String>>,
        input: PhpMixed,
        timeout: Option<f64>,
    ) -> anyhow::Result<Self> {
        let mut this = Self::empty();
        this.commandline = CommandLine::Array(command);
        this.cwd = cwd;

        // on Windows, if the cwd changed via chdir(), proc_open defaults to the dir where PHP was started
        // PHP: null === $this->cwd && (\defined('ZEND_THREAD_SAFE') || '\\' === \DIRECTORY_SEPARATOR)
        // `\defined('ZEND_THREAD_SAFE')` is unconditionally true in modern PHP (the constant always
        // exists; its value merely reflects the NTS/ZTS build), so the disjunction is always true and
        // the cwd is defaulted to getcwd() whenever it was null.
        if this.cwd.is_none() {
            // TODO(bytes): Process keeps the working directory as a String.
            this.cwd = getcwd().map(|cwd| {
                cwd.into_os_string()
                    .into_string()
                    .unwrap_or_else(|cwd| panic!("cwd is not valid UTF-8: {cwd:?}"))
            });
        }
        if let Some(env) = env {
            this.set_env(
                env.into_iter()
                    .map(|(k, v)| (k.into(), PhpMixed::string(v)))
                    .collect(),
            );
        }

        this.set_input(input)?;
        this.set_timeout(timeout)?;

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

    /// Runs the process.
    pub fn run(
        &mut self,
        callback: Option<UserCallback>,
        env: IndexMap<BString, PhpMixed>,
    ) -> anyhow::Result<i64> {
        self.start(callback, env)?;

        self.wait(None)
    }

    /// Starts the process and returns after writing the input to STDIN.
    pub fn start(
        &mut self,
        callback: Option<UserCallback>,
        mut env: IndexMap<BString, PhpMixed>,
    ) -> anyhow::Result<()> {
        if self.is_running() {
            return Err(RuntimeException::new("Process is already running.".to_string()).into());
        }

        self.reset_process_data();
        self.starttime = Some(microtime());
        self.callback = Some(self.build_callback(callback));
        let descriptors = self.get_descriptors();

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

                if !cfg!(windows) {
                    // exec is mandatory to deal with sending a signal to the process
                    cmd = format!("exec {}", cmd);
                }
                cmd
            }
            CommandLine::String(s) => self.replace_placeholders(s, &env)?,
        };

        if cfg!(windows) {
            commandline = self.prepare_windows_command_line(&commandline, &mut env)?;
        }

        let mut env_pairs: Vec<String> = Vec::new();
        for (k, v) in &env {
            let is_false = matches!(v, PhpMixed::Bool(false));
            if !is_false
                && !["argc", "argv", "ARGC", "ARGV"]
                    .iter()
                    .any(|name| k == name)
            {
                env_pairs.push(format!("{}={}", k, to_php_string(v)));
            }
        }

        if !self.cwd.as_deref().map(is_dir).unwrap_or(false) {
            return Err(RuntimeException::new(format!(
                "The provided cwd \"{}\" does not exist.",
                self.cwd.as_deref().unwrap_or_default()
            ))
            .into());
        }

        let cwd = self.cwd.clone();
        let options = self.options.clone();
        let process = {
            let pipes = self.process_pipes.as_mut().unwrap().pipes_mut();
            proc_open(
                &commandline,
                &descriptors,
                pipes,
                cwd.as_deref().map(std::path::Path::new),
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

        if self.tty {
            return Ok(());
        }

        self.update_status(false);
        self.take_callback_error()?;
        self.check_timeout()?;
        Ok(())
    }

    /// Waits for the process to terminate.
    pub fn wait(&mut self, callback: Option<UserCallback>) -> anyhow::Result<i64> {
        self.require_process_is_started("wait")?;

        self.update_status(false);
        self.take_callback_error()?;

        if let Some(callback) = callback {
            self.callback = Some(self.build_callback(Some(callback)));
        }

        loop {
            self.check_timeout()?;
            let running = self.is_running()
                && (cfg!(windows) || self.process_pipes.as_ref().unwrap().are_open());
            self.take_callback_error()?;
            self.read_pipes(running, !cfg!(windows) || !running);
            self.take_callback_error()?;
            if !running {
                break;
            }
        }

        while self.is_running() {
            self.take_callback_error()?;
            self.check_timeout()?;
            usleep(1000);
        }
        self.take_callback_error()?;

        let signaled = self
            .process_information
            .as_ref()
            .map(|i| i.signaled)
            .unwrap_or(false);
        let termsig = self.process_information.as_ref().map(|i| i.termsig);
        if signaled && termsig != self.latest_signal {
            return Err(ProcessSignaledException::new(self)?.into());
        }

        Ok(self.exitcode.unwrap_or(0))
    }

    /// Returns the Pid (process identifier), if applicable.
    pub fn get_pid(&mut self) -> Option<i64> {
        if self.is_running() {
            self.process_information.as_ref().map(|i| i.pid)
        } else {
            None
        }
    }

    /// Returns the current output of the process (STDOUT).
    pub fn get_output(&mut self) -> anyhow::Result<String> {
        if let Some(mock) = &self.mock {
            return Ok(mock.stdout.clone());
        }

        self.read_pipes_for_output("getOutput", false)?;

        // TODO(bytes): the output is returned as a String.
        Ok(String::from_utf8_lossy(
            &stream_get_contents3(self.stdout.as_ref().unwrap(), -1, 0).unwrap_or_default(),
        )
        .into_owned())
    }

    /// Returns the current error output of the process (STDERR).
    pub fn get_error_output(&mut self) -> anyhow::Result<String> {
        if let Some(mock) = &self.mock {
            return Ok(mock.stderr.clone());
        }

        self.read_pipes_for_output("getErrorOutput", false)?;

        // TODO(bytes): the output is returned as a String.
        Ok(String::from_utf8_lossy(
            &stream_get_contents3(self.stderr.as_ref().unwrap(), -1, 0).unwrap_or_default(),
        )
        .into_owned())
    }

    /// Returns the exit code returned by the process.
    pub fn get_exit_code(&mut self) -> Option<i64> {
        if self.mock.is_some() {
            return self.exitcode;
        }

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

    /// Returns the number of the signal that caused the child process to terminate.
    pub fn get_term_signal(&mut self) -> anyhow::Result<i64> {
        self.require_process_is_terminated("getTermSignal")?;

        Ok(self
            .process_information
            .as_ref()
            .map(|i| i.termsig)
            .unwrap_or(0))
    }

    /// Returns true if the child process has been terminated by an uncaught signal.
    pub fn has_been_signaled(&mut self) -> anyhow::Result<bool> {
        self.require_process_is_terminated("hasBeenSignaled")?;

        Ok(self
            .process_information
            .as_ref()
            .map(|i| i.signaled)
            .unwrap_or(false))
    }

    /// Returns true if the child process has been stopped by a signal.
    pub fn has_been_stopped(&mut self) -> anyhow::Result<bool> {
        self.require_process_is_terminated("hasBeenStopped")?;

        Ok(self
            .process_information
            .as_ref()
            .map(|i| i.stopped)
            .unwrap_or(false))
    }

    /// Returns the number of the signal that caused the child process to stop its execution.
    pub fn get_stop_signal(&mut self) -> anyhow::Result<i64> {
        self.require_process_is_terminated("getStopSignal")?;

        Ok(self
            .process_information
            .as_ref()
            .map(|i| i.stopsig)
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
            .map(|i| i.running)
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

    /// Gets the process status.
    pub fn get_status(&mut self) -> &str {
        self.update_status(false);

        &self.status
    }

    /// Stops the process.
    pub fn stop(&mut self, timeout: f64, signal: Option<i64>) -> Option<i64> {
        let timeout_micro = microtime() + timeout;
        if self.is_running() {
            // given SIGTERM may not be defined and that "proc_terminate" uses the constant value
            // and not the constant itself, we use the same here
            let _ = self.do_signal(15, false);
            loop {
                usleep(1000);
                if !(self.is_running() && microtime() < timeout_micro) {
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
            self.close();
        }

        self.exitcode
    }

    /// Adds a line to the STDOUT stream.
    pub fn add_output(&mut self, line: &str) {
        let stdout = self.stdout.as_ref().unwrap();
        fseek(stdout, 0, SEEK_END);
        fwrite(stdout, line, Some(line.len() as i64));
        fseek(stdout, self.incremental_output_offset, SEEK_SET);
    }

    /// Adds a line to the STDERR stream.
    pub fn add_error_output(&mut self, line: &str) {
        let stderr = self.stderr.as_ref().unwrap();
        fseek(stderr, 0, SEEK_END);
        fwrite(stderr, line, Some(line.len() as i64));
        fseek(stderr, self.incremental_error_output_offset, SEEK_SET);
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

    /// Sets the process timeout (max. runtime) in seconds.
    pub fn set_timeout(&mut self, timeout: Option<f64>) -> anyhow::Result<&mut Self> {
        self.timeout = self.validate_timeout(timeout)?;

        Ok(self)
    }

    /// Enables or disables the TTY mode.
    pub fn set_tty(&mut self, tty: bool) -> anyhow::Result<&mut Self> {
        if cfg!(windows) && tty {
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

    /// Gets the working directory.
    pub fn get_working_directory(&self) -> Option<String> {
        if self.cwd.is_none() {
            // getcwd() will return false if any one of the parent directories does not have
            // the readable or search mode set, even if the current directory does
            // TODO(bytes): Process keeps the working directory as a String.
            return getcwd()
                .map(|cwd| {
                    cwd.into_os_string()
                        .into_string()
                        .unwrap_or_else(|cwd| panic!("cwd is not valid UTF-8: {cwd:?}"))
                })
                .filter(|s| !s.is_empty());
        }

        self.cwd.clone()
    }

    /// Gets the environment variables.
    pub fn get_env(&self) -> &IndexMap<BString, PhpMixed> {
        &self.env
    }

    /// Sets the environment variables.
    pub fn set_env(&mut self, env: IndexMap<BString, PhpMixed>) -> &mut Self {
        self.env = env;

        self
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
            && timeout < microtime() - self.starttime.unwrap_or(0.0)
        {
            self.stop(0.0, None);

            return Err(ProcessTimedOutException::new(self).into());
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

        Ok(self.starttime.unwrap_or_default())
    }

    /// Returns whether TTY is supported on the current operating system.
    pub fn is_tty_supported() -> bool {
        static IS_TTY_SUPPORTED: OnceLock<bool> = OnceLock::new();

        *IS_TTY_SUPPORTED.get_or_init(|| {
            let mut pipes = IndexMap::new();
            proc_open(
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

    /// Creates the descriptors needed by the proc_open.
    fn get_descriptors(&mut self) -> Vec<Descriptor> {
        // TODO(plugin): $this->input instanceof \Iterator -> rewind() is not modeled.
        if cfg!(windows) {
            self.process_pipes = Some(Box::new(WindowsPipes::new(self.input.clone())));
        } else {
            self.process_pipes = Some(Box::new(UnixPipes::new(
                Some(self.is_tty()),
                self.input.clone(),
            )));
        }

        self.process_pipes.as_mut().unwrap().get_descriptors()
    }

    /// Builds up the callback used by wait().
    fn build_callback(&self, callback: Option<UserCallback>) -> ProcessCallback {
        let mut callback = callback;
        let out = Self::OUT;

        Box::new(
            move |this: &mut Process, r#type: &str, data: &str| -> anyhow::Result<bool> {
                if out == r#type {
                    this.add_output(data);
                } else {
                    this.add_error_output(data);
                }

                match callback.as_mut() {
                    Some(cb) => cb(r#type, data),
                    None => Ok(false),
                }
            },
        )
    }

    /// Updates the status of the process, reads pipes.
    fn update_status(&mut self, blocking: bool) {
        if Self::STATUS_STARTED != self.status {
            return;
        }

        self.process_information = Some(proc_get_status(self.process.as_ref().unwrap()));
        let running = self.process_information.as_ref().unwrap().running;

        self.read_pipes(running && blocking, !cfg!(windows) || !running);

        if !running {
            self.close();
        }
    }

    /// Reads pipes for the freshest output.
    fn read_pipes_for_output(&mut self, caller: &str, blocking: bool) -> anyhow::Result<()> {
        self.require_process_is_started(caller)?;

        self.update_status(blocking);
        self.take_callback_error()
    }

    /// Returns the error the callback raised under a method that could not return it.
    fn take_callback_error(&mut self) -> anyhow::Result<()> {
        match self.callback_error.take() {
            Some(error) => Err(error),
            None => Ok(()),
        }
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
            if self.callback_error.is_some() {
                break;
            }
            if let Some(cb) = callback.as_mut()
                && let Err(error) = cb(
                    self,
                    if Self::STDOUT == r#type {
                        Self::OUT
                    } else {
                        Self::ERR
                    },
                    &data,
                )
            {
                self.callback_error = Some(error);
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
            proc_close(self.process.as_ref().unwrap());
            self.process = None;
        }
        self.exitcode = self.process_information.as_ref().map(|i| i.exitcode);
        self.status = Self::STATUS_TERMINATED.to_string();

        if self.exitcode == Some(-1) {
            let signaled = self
                .process_information
                .as_ref()
                .map(|i| i.signaled)
                .unwrap_or(false);
            let termsig = self
                .process_information
                .as_ref()
                .map(|i| i.termsig)
                .unwrap_or(0);
            if signaled && termsig > 0 {
                // if process has been signaled, no exitcode but a valid termsig, apply Unix convention
                self.exitcode = Some(128 + termsig);
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
        self.process_information = None;
        // php://temp is an in-memory stream; fopen never fails for it.
        self.stdout = Some(fopen(&format!("php://temp/maxmemory:{}", 1024 * 1024), "w+").unwrap());
        self.stderr = Some(fopen(&format!("php://temp/maxmemory:{}", 1024 * 1024), "w+").unwrap());
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

        if cfg!(windows) {
            let mut output: Vec<String> = Vec::new();
            let mut exit_code: i64 = 0;
            exec(
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
            let ok = proc_terminate(self.process.as_ref().unwrap(), signal);
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

        Ok(true)
    }

    fn prepare_windows_command_line(
        &mut self,
        cmd: &str,
        env: &mut IndexMap<BString, PhpMixed>,
    ) -> anyhow::Result<String> {
        let uid = uniqid("", true);
        let mut var_count = 0;
        let mut var_cache: IndexMap<String, String> = IndexMap::new();
        let cmd = preg_replace_callback(
            php_regex!(
                r#"/"(?:(
                [^"%!^]*+
                (?:
                    (?: !LF! | "(?:\^[%!^])?+" )
                    [^"%!^]*+
                )++
            ) | [^"]*+ )"/x"#
            ),
            |m: &PregMatches| -> anyhow::Result<String> {
                let m0 = m.get(0).unwrap_or_default().to_string();
                let m1 = m.get(1).map(|s| legacy_lossy(s).into_owned());
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
                if strpbrk(&value, "\"%!\n").is_none() {
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
                    preg_replace(php_regex!(r#"/(\\*)"/"#), "$1$1\\\"", &value)
                );
                var_count += 1;
                let var = format!("{}{}", uid, var_count);

                env.insert(var.as_str().into(), PhpMixed::string(value));

                let replacement = format!("!{}!", var);
                var_cache.insert(m0, replacement.clone());
                Ok(replacement)
            },
            cmd,
        )?;
        let cmd = legacy_lossy(&cmd).into_owned();

        static COM_SPEC: OnceLock<Option<String>> = OnceLock::new();
        let com_spec = COM_SPEC
            .get_or_init(|| {
                ExecutableFinder::new()
                    .find("cmd.exe", None, &[])
                    .map(|spec| {
                        format!(
                            "\"{}\"",
                            preg_replace(php_regex!(r#"{(\\*+)"}"#), "$1$1\\\"", &spec)
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
        if !cfg!(windows) {
            return format!("'{}'", argument.replace('\'', "'\\''"));
        }
        let mut argument = argument.to_string();
        if argument.contains('\0') {
            argument = argument.replace('\0', "?");
        }
        if !preg_is_match(php_regex!(r#"/[()%!^"<>&|\s\[\]=;*?'$]/"#), &argument) {
            return argument;
        }
        argument =
            legacy_lossy(&preg_replace(php_regex!(r"/(\\+)$/"), "$1$1", &argument)).into_owned();

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
        env: &IndexMap<BString, PhpMixed>,
    ) -> anyhow::Result<String> {
        let replaced = preg_replace_callback(
            php_regex!(r#"/"\$\{:([_a-zA-Z]+[_a-zA-Z0-9]*)\}"/"#),
            |matches: &PregMatches| -> anyhow::Result<String> {
                let key = matches.get(1).unwrap_or_default().to_string();
                match env.get(B(&key)) {
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
        )?;
        Ok(legacy_lossy(&replaced).into_owned())
    }

    fn get_default_env(&self) -> IndexMap<BString, PhpMixed> {
        // TODO(bytes): the environment is kept as Strings.
        let env: IndexMap<String, String> = getenv_all()
            .map(|(k, v)| {
                (
                    k.into_string()
                        .unwrap_or_else(|k| panic!("env name is not valid UTF-8: {k:?}")),
                    v.into_string()
                        .unwrap_or_else(|v| panic!("env value is not valid UTF-8: {v:?}")),
                )
            })
            .collect();
        let server = PHP_SERVER.lock().unwrap();

        // non-Windows: array_intersect_key($env, $_SERVER) ?: $env
        let mut intersect: IndexMap<BString, PhpMixed> = IndexMap::new();
        for (k, v) in &env {
            if server.get(k).is_some() {
                intersect.insert(k.as_str().into(), PhpMixed::string(v.clone()));
            }
        }
        let env_map: IndexMap<BString, PhpMixed> = if intersect.is_empty() {
            env.into_iter()
                .map(|(k, v)| (k.into(), PhpMixed::string(v)))
                .collect()
        } else {
            intersect
        };

        // $_ENV + env_map
        let mut result: IndexMap<BString, PhpMixed> = PHP_ENV
            .lock()
            .unwrap()
            .get_all()
            // TODO(bytes): the environment is kept as Strings.
            .map(|(k, v)| {
                (
                    k.into_string()
                        .unwrap_or_else(|k| panic!("env name is not valid UTF-8: {k:?}"))
                        .into(),
                    PhpMixed::string(
                        v.into_string()
                            .unwrap_or_else(|v| panic!("env value is not valid UTF-8: {v:?}")),
                    ),
                )
            })
            .collect();
        for (k, v) in env_map {
            result.entry(k).or_insert(v);
        }
        result
    }
}

impl Drop for Process {
    fn drop(&mut self) {
        self.stop(0.0, None);
    }
}
