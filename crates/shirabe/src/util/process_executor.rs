//! ref: composer/src/Composer/Util/ProcessExecutor.php

use crate::io::{IOInterface, IOInterfaceImmutable, io_interface};
use crate::signal::SignalSubscription;
use crate::util::{GitHub, Platform};
use bstr::{BStr, BString, ByteSlice};
use indexmap::IndexMap;
use shirabe_php_shim::{
    Catch as _, LogicException, PHP_EOL, PhpMixed, PregMatches, RuntimeException, array_intersect,
    array_map, bconcat, escapeshellarg, explode, implode, in_array_strict, is_dir, is_numeric,
    legacy_lossy, path_from_bytes, path_to_bytes, php_regex, preg_is_match, preg_match,
    preg_replace, preg_replace_callback, preg_replace2, preg_split, rtrim2, str_replace, strcspn,
    strlen, strpbrk, strtolower, strtr_array, substr_replace, trim,
};
use shirabe_symfony_process::exception::{
    ProcessSignaledException, RuntimeException as SymfonyProcessRuntimeException,
};
use shirabe_symfony_process::{ExecutableFinder, Process, ProcessMock};
use std::path::Path;
use std::sync::{LazyLock, Mutex};

static EXECUTABLES: LazyLock<Mutex<IndexMap<String, String>>> =
    LazyLock::new(|| Mutex::new(IndexMap::new()));

static TIMEOUT: LazyLock<Mutex<i64>> = LazyLock::new(|| Mutex::new(300));

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CommandLine {
    Shell(BString),
    Args(Vec<BString>),
}

#[derive(Debug)]
pub struct ProcessExecutor {
    capture_output: std::cell::Cell<bool>,
    error_output: std::cell::RefCell<BString>,
    io: Option<std::rc::Rc<std::cell::RefCell<dyn IOInterface>>>,
    max_jobs: std::cell::Cell<i64>,
    /// PHP throttles async jobs through the $jobs queue and $maxJobs; here concurrent
    /// `execute_async`/`execute_async_php` calls hold a permit for the duration of the
    /// child process instead (same design as HttpDownloader).
    semaphore: std::cell::RefCell<std::rc::Rc<tokio::sync::Semaphore>>,
    allow_async: std::cell::Cell<bool>,
    /// Test-only mock state. `None` in production; set via [`ProcessExecutor::__expects`] in tests.
    /// Mirrors `composer/tests/Composer/Test/Mock/ProcessExecutorMock.php`.
    mock: Option<std::cell::RefCell<ProcessExecutorMockState>>,
}

/// Test-only state for the ProcessExecutorMock behaviour (cf.
/// `composer/tests/Composer/Test/Mock/ProcessExecutorMock.php`). Held in
/// [`ProcessExecutor::mock`]; always `None` in production builds.
#[derive(Debug)]
pub struct ProcessExecutorMockState {
    /// `null` until configured via `expects`; once set, an empty list means "no more calls".
    pub expectations: Option<Vec<MockExpectation>>,
    pub strict: bool,
    pub default_handler: MockHandler,
    pub log: Vec<String>,
}

/// A single expected command (`array{cmd, return, stdout, stderr, callback}` in PHP).
pub struct MockExpectation {
    pub cmd: CommandLine,
    pub r#return: i64,
    pub stdout: String,
    pub stderr: String,
    /// Optional `callable` fired when the expectation is consumed. Rare in Composer's suite.
    pub callback: Option<Box<dyn FnMut()>>,
}

impl std::fmt::Debug for MockExpectation {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("MockExpectation")
            .field("cmd", &self.cmd)
            .field("return", &self.r#return)
            .field("stdout", &self.stdout)
            .field("stderr", &self.stderr)
            .field("callback", &self.callback.is_some())
            .finish()
    }
}

impl MockExpectation {
    /// Builds an expectation from just a command (string or list), defaulting the rest, matching
    /// PHP's handling of bare `string`/`list` entries in `expects`.
    pub fn from_cmd(cmd: CommandLine) -> Self {
        Self {
            cmd,
            r#return: 0,
            stdout: String::new(),
            stderr: String::new(),
            callback: None,
        }
    }
}

/// The `{return, stdout, stderr}` default-handler triple used for unmatched commands in non-strict
/// mode (`$defaultHandler` in PHP).
#[derive(Debug, Clone, Default)]
pub struct MockHandler {
    pub r#return: i64,
    pub stdout: String,
    pub stderr: String,
}

/// Output target marking the "no `$output` argument" case of `ProcessExecutor::execute`, where the
/// child's output is forwarded to STDOUT/STDERR (or the IO) instead of captured. Use the
/// [`ProcessExecutor::FORWARD_OUTPUT`] constant rather than constructing this directly.
pub struct ProcessForwardOutput;

impl ProcessExecutor {
    pub const FORWARD_OUTPUT: ProcessForwardOutput = ProcessForwardOutput;

    const BUILTIN_CMD_COMMANDS: [&'static str; 47] = [
        "assoc", "break", "call", "cd", "chdir", "cls", "color", "copy", "date", "del", "dir",
        "echo", "endlocal", "erase", "exit", "for", "ftype", "goto", "help", "if", "label", "md",
        "mkdir", "mklink", "move", "path", "pause", "popd", "prompt", "pushd", "rd", "rem", "ren",
        "rename", "rmdir", "set", "setlocal", "shift", "start", "time", "title", "type", "ver",
        "vol", // unused slots to make 47 above explicit
        "", "", "",
    ];

    const GIT_CMDS_NEED_GIT_DIR: &'static [&'static [&'static str]] =
        &[&["show"], &["log"], &["branch"], &["remote", "set-url"]];

    pub fn new(io: Option<std::rc::Rc<std::cell::RefCell<dyn IOInterface>>>) -> Self {
        let this = Self {
            capture_output: std::cell::Cell::new(false),
            error_output: std::cell::RefCell::new(BString::default()),
            io,
            max_jobs: std::cell::Cell::new(10),
            semaphore: std::cell::RefCell::new(std::rc::Rc::new(tokio::sync::Semaphore::new(10))),
            allow_async: std::cell::Cell::new(false),
            mock: None,
        };
        this.reset_max_jobs();
        this
    }

    /// runs a process on the commandline
    pub fn execute<'o, C, O>(
        &self,
        command: C,
        output: O,
        cwd: Option<&Path>,
    ) -> anyhow::Result<i64>
    where
        C: IntoExecCommand,
        O: IntoExecOutput<'o>,
    {
        let command = command.into_exec_command();
        self.do_execute(command, cwd, false, output)
    }

    /// runs a process on the commandline in TTY mode
    pub fn execute_tty<C>(&self, command: C, cwd: Option<&Path>) -> anyhow::Result<i64>
    where
        C: IntoExecCommand,
    {
        let command = command.into_exec_command();
        if Platform::is_tty(None) {
            return self.do_execute(command, cwd, true, Self::FORWARD_OUTPUT);
        }

        self.do_execute(command, cwd, false, Self::FORWARD_OUTPUT)
    }

    fn run_process<'o, O>(
        &self,
        command: CommandLine,
        cwd: Option<&Path>,
        env: Option<IndexMap<String, String>>,
        tty: bool,
        output: O,
    ) -> anyhow::Result<Option<i64>>
    where
        O: IntoExecOutput<'o>,
    {
        // On Windows, we don't rely on the OS to find the executable if possible to avoid lookups
        // in the current directory which could be untrusted. Instead we use the ExecutableFinder.

        let mut process: Process = match command {
            CommandLine::Shell(mut command) => {
                if Platform::is_windows()
                    && let Some(m) = preg_match(php_regex!(r"{^([^:/\\]++) }"), &command)
                {
                    let m1 = BString::from(m.get(1).unwrap_or_default());
                    command = substr_replace(
                        &command,
                        Self::escape(Self::get_executable(&m1)),
                        0,
                        Some(strlen(&m1)),
                    );
                }

                Process::from_shell_commandline(
                    &command,
                    cwd,
                    env,
                    PhpMixed::Null,
                    Some(Self::get_timeout() as f64),
                )?
            }
            CommandLine::Args(mut command) => {
                if Platform::is_windows()
                    && strlen(&command[0]) == strcspn(&command[0], ":/\\") as i64
                {
                    command[0] = Self::get_executable(&command[0]);
                }

                Process::new(
                    command,
                    cwd.map(Path::to_path_buf),
                    env,
                    PhpMixed::Null,
                    Some(Self::get_timeout() as f64),
                )?
            }
        };

        if !Platform::is_windows() && tty {
            // PHP: try { $process->setTty(true); } catch (RuntimeException $e) { /* ignore */ }
            if let Err(e) = process.set_tty(true)
                && !e.is_instanceof::<SymfonyProcessRuntimeException>()
            {
                return Err(e);
            }
            // ignore TTY enabling errors
        }

        let signals = SignalSubscription::new();

        let result: anyhow::Result<()> = (|| -> anyhow::Result<()> {
            match output.to_callback() {
                Ok(callback) => {
                    process.run(Some(callback), IndexMap::new())?;
                }
                Err(mut output) => {
                    let capture_output = self.capture_output.get();
                    let mut io = self.io.clone();
                    let callback = move |r#type: &str, buffer: &BStr| {
                        Self::output_handler(capture_output, &mut io, r#type, buffer);
                        Ok(false)
                    };
                    process.run(Some(Box::new(callback)), IndexMap::new())?;
                    if self.capture_output.get() {
                        output.write_back(process.get_output()?);
                    }
                }
            }

            *self.error_output.borrow_mut() = process.get_error_output()?;
            Ok(())
        })();
        if signals.is_triggered()
            && let Some(io) = &self.io
        {
            io.write_error3(
                &format!(
                    "Received {}, aborting when child process is done",
                    signals.last_signal().as_str()
                ),
                true,
                io_interface::DEBUG,
            );
        }
        let final_result: anyhow::Result<()> = match result {
            Ok(()) => Ok(()),
            Err(e) => {
                if let Some(pse) = e.catch::<ProcessSignaledException>() {
                    if signals.is_triggered() {
                        // exiting as we were signaled and the child process exited too due to the signal
                        signals.exit_with_last_signal();
                    }
                    let _ = pse;
                    Ok(())
                } else {
                    return Err(e);
                }
            }
        };
        final_result?;

        Ok(process.get_exit_code())
    }

    fn do_execute<'o, O>(
        &self,
        command: CommandLine,
        cwd: Option<&Path>,
        tty: bool,
        output: O,
    ) -> anyhow::Result<i64>
    where
        O: IntoExecOutput<'o>,
    {
        if self.mock.is_some() {
            return self.mock_do_execute(command, cwd, output);
        }

        self.output_command_run(&command, cwd, false);

        self.capture_output.set(output.capture_output());
        *self.error_output.borrow_mut() = BString::default();

        let mut env: Option<IndexMap<String, String>> = None;

        let requires_git_dir_env = self.requires_git_dir_env(&command);
        if let Some(cwd) = cwd
            && requires_git_dir_env
        {
            let is_bare_repository = !is_dir(path_from_bytes(&bconcat!(
                rtrim2(path_to_bytes(cwd), "/"),
                "/.git"
            )));
            if is_bare_repository {
                let mut config_value = BString::default();
                let mut git_env: IndexMap<String, String> = IndexMap::new();
                // TODO(bytes): Process takes the environment as Strings.
                git_env.insert(
                    "GIT_DIR".to_string(),
                    legacy_lossy(path_to_bytes(cwd)).into_owned(),
                );
                self.run_process(
                    CommandLine::Args(vec![
                        "git".into(),
                        "config".into(),
                        "safe.bareRepository".into(),
                    ]),
                    Some(cwd),
                    Some(git_env.clone()),
                    tty,
                    &mut config_value,
                )?;
                let trimmed = trim(&config_value);
                if trimmed == "explicit" {
                    env = Some(git_env);
                }
            }
        }

        Ok(self
            .run_process(command, cwd, env, tty, output)?
            .unwrap_or(0))
    }

    /// Shared expectation-matching logic behind the mock branches of `do_execute` and
    /// `execute_async` (cf. `ProcessExecutorMock::doExecute`). Logs the command, matches it
    /// against the head of the expectation queue (exact `===`), pops on match (firing the
    /// optional callback), falls back to the default handler in non-strict mode, or returns an
    /// error in strict mode (cf. PHPUnit's `AssertionFailedError`, itself a catchable
    /// `\RuntimeException` — callers such as `Git::get_mirror_default_branch` rely on being able
    /// to catch a strict-mode mismatch rather than have it abort the process). Returns `(stdout,
    /// stderr, return)`.
    fn mock_match(
        &self,
        command: &CommandLine,
        cwd: Option<&Path>,
    ) -> anyhow::Result<(String, String, i64)> {
        let command_string = legacy_lossy(&match command {
            CommandLine::Args(args) => implode(" ", args),
            CommandLine::Shell(command) => command.clone(),
        })
        .into_owned();

        let mut mock = self.mock.as_ref().unwrap().borrow_mut();
        mock.log.push(command_string);

        let matched = mock
            .expectations
            .as_ref()
            .map(|exps| !exps.is_empty() && exps[0].cmd == *command)
            .unwrap_or(false);

        let (stdout, stderr, r#return);
        let mut callback = None;
        if matched {
            let mut expect = mock.expectations.as_mut().unwrap().remove(0);
            stdout = expect.stdout.clone();
            stderr = expect.stderr.clone();
            r#return = expect.r#return;
            callback = expect.callback.take();
        } else if !mock.strict {
            stdout = mock.default_handler.stdout.clone();
            stderr = mock.default_handler.stderr.clone();
            r#return = mock.default_handler.r#return;
        } else {
            let expected = mock
                .expectations
                .as_ref()
                .filter(|exps| !exps.is_empty())
                .map(|exps| format!("Expected {:?} at this point.", exps[0].cmd))
                .unwrap_or_else(|| "Expected no more calls at this point.".to_string());
            let received = mock.log[..mock.log.len().saturating_sub(1)].join(PHP_EOL);
            // PHPUnit's `AssertionFailedError` (thrown by `ProcessExecutorMock::doExecute` on a
            // strict-mode mismatch) extends `\RuntimeException`, so PHP call sites that
            // `catch (\RuntimeException $e)` around a mock-driven git/hg/svn call (e.g.
            // `GitDriver::supports`) treat a mismatch as an ordinary recoverable failure. Using
            // the same `RuntimeException` type here keeps `catch::<RuntimeException>()` checks
            // working the same way against a mismatch.
            return Err(RuntimeException::new(format!(
                "Received unexpected command {:?} in \"{}\"{}{}{}Received calls:{}{}",
                command,
                cwd.unwrap_or(Path::new("")).display(),
                PHP_EOL,
                expected,
                PHP_EOL,
                PHP_EOL,
                received
            ))
            .into());
        }

        drop(mock);
        if let Some(mut callback) = callback {
            callback();
        }

        Ok((stdout, stderr, r#return))
    }

    /// Mock replacement for `do_execute` when [`Self::mock`] is set (cf.
    /// `ProcessExecutorMock::doExecute`). Delegates the matching to [`Self::mock_match`], then
    /// emits stdout/stderr through the output target and records `error_output`.
    fn mock_do_execute<'o, O>(
        &self,
        command: CommandLine,
        cwd: Option<&Path>,
        output: O,
    ) -> anyhow::Result<i64>
    where
        O: IntoExecOutput<'o>,
    {
        let capture_output = output.capture_output();
        self.capture_output.set(capture_output);
        *self.error_output.borrow_mut() = BString::default();

        let (stdout, stderr, r#return) = self.mock_match(&command, cwd)?;

        // Feed stdout/stderr through the output target, mirroring the PHP `$callback(...)` calls.
        match output.to_callback() {
            Ok(mut callback) => {
                if !stdout.is_empty() {
                    callback(Process::OUT, stdout.as_bytes().as_bstr())?;
                }
                if !stderr.is_empty() {
                    callback(Process::ERR, stderr.as_bytes().as_bstr())?;
                }
            }
            Err(mut out) => {
                let mut io = self.io.clone();
                if !stdout.is_empty() {
                    Self::output_handler(
                        capture_output,
                        &mut io,
                        Process::OUT,
                        stdout.as_bytes().as_bstr(),
                    );
                }
                if !stderr.is_empty() {
                    Self::output_handler(
                        capture_output,
                        &mut io,
                        Process::ERR,
                        stderr.as_bytes().as_bstr(),
                    );
                }
                if capture_output {
                    out.write_back(stdout.clone().into());
                }
            }
        }

        *self.error_output.borrow_mut() = stderr.into();

        Ok(r#return)
    }

    /// For testing only. Configures the mock expectation queue (cf. `ProcessExecutorMock::expects`).
    /// Activates the mock branch in `do_execute`/`execute_async`.
    pub fn __expects(
        &mut self,
        expectations: Vec<MockExpectation>,
        strict: bool,
        default_handler: MockHandler,
    ) {
        self.mock = Some(std::cell::RefCell::new(ProcessExecutorMockState {
            expectations: Some(expectations),
            strict,
            default_handler,
            log: Vec::new(),
        }));
    }

    /// For testing only. Asserts all configured expectations were consumed (cf.
    /// `ProcessExecutorMock::assertComplete`). Panics with the remaining/received commands otherwise.
    pub fn __assert_complete(&self) {
        let Some(mock) = self.mock.as_ref() else {
            return;
        };
        let mock = mock.borrow();
        // Not configured to expect anything, so no need to react here.
        let Some(expectations) = mock.expectations.as_ref() else {
            return;
        };

        if !expectations.is_empty() {
            let remaining: Vec<BString> = expectations
                .iter()
                .map(|expect| match &expect.cmd {
                    CommandLine::Args(args) => implode(" ", args),
                    CommandLine::Shell(command) => command.clone(),
                })
                .collect();
            panic!(
                "There are still {} expected process calls which have not been consumed:{}{}{}{}Received calls:{}{}",
                expectations.len(),
                PHP_EOL,
                implode(PHP_EOL, &remaining),
                PHP_EOL,
                PHP_EOL,
                PHP_EOL,
                mock.log.join(PHP_EOL)
            );
        }
    }

    /// starts a process on the commandline in async mode, for callers within Rust-ported Composer
    /// code (i.e. everything except a plugin holding a `ProcessExecutor` RPC handle — see
    /// `execute_async_php` and docs/dev/plugin-class-classification.md, "Process: dual
    /// instantiation split by caller"). This is the direct 1:1 port of PHP's `executeAsync()`:
    /// almost all calls go through here, spawning in Rust with no PHP child involved.
    ///
    /// The max_jobs throttle is enforced by the semaphore, shared with `execute_async_php`.
    ///
    /// The mock branch updates `error_output`/`capture_output` before returning, mirroring
    /// `ProcessExecutorMock::executeAsync` — which resolves through the same `doExecute` the sync
    /// path uses, so it updates the executor's cached error output too (cf. `mock_do_execute`).
    pub fn execute_async<C>(
        &self,
        command: C,
        cwd: Option<&Path>,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = anyhow::Result<Process>>>>
    where
        C: IntoExecCommand,
    {
        self.execute_async_job(command, cwd, None)
    }

    /// `execute_async` for a caller that registers the future as a job (see
    /// `docs/dev/plugin-async.md`): `started` is set once the job budget lets the job through,
    /// which is when PHP's `startJob()` moves it from `STATUS_QUEUED` to `STATUS_STARTED`.
    pub(crate) fn execute_async_job<C>(
        &self,
        command: C,
        cwd: Option<&Path>,
        started: Option<std::rc::Rc<std::cell::Cell<bool>>>,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = anyhow::Result<Process>>>>
    where
        C: IntoExecCommand,
    {
        let command = command.into_exec_command();
        if self.mock.is_some() {
            // PHP resolves the promise with a Process mock whose getOutput/isSuccessful/getExitCode
            // reflect the doExecute result; reuse the same expectation matching as the sync mock
            // branch and fabricate the resolved Process via `Process::__mock`, a test seam mirroring
            // `ZipArchive::__mock`.
            let matched = self.mock_match(&command, cwd);
            if let Ok((_, ref stderr, _)) = matched {
                self.capture_output.set(true);
                *self.error_output.borrow_mut() = stderr.clone().into();
            }
            return Box::pin(async move {
                let (stdout, stderr, r#return) = matched?;
                Ok(Process::__mock(ProcessMock {
                    exit_code: r#return,
                    stdout,
                    stderr,
                }))
            });
        }
        let allow_async = self.allow_async.get();
        let semaphore = self.semaphore.borrow().clone();
        let io = self.io.clone();
        let cwd = cwd.map(Path::to_path_buf);

        Box::pin(async move {
            if !allow_async {
                return Err(LogicException::new("You must use the ProcessExecutor instance which is part of a Composer\\Loop instance to be able to run async processes".to_string())
                .into());
            }

            // PHP queues the job and only startJob()s it once runningJobs < maxJobs; the permit is
            // the equivalent gate, so everything below (including the "Executing async command"
            // debug line PHP prints from startJob) happens only once a slot is free.
            let _permit = semaphore
                .acquire()
                .await
                .expect("the semaphore is never closed");
            if let Some(started) = started {
                started.set(true);
            }

            Self::output_command_run_with(&io, &command, cwd.as_deref(), true);

            // PHP: $job['reject']($e) on process construction/start failure — surfaced as Err here.
            let mut process = match command {
                CommandLine::Shell(command) => Process::from_shell_commandline(
                    &command,
                    cwd.as_deref(),
                    None,
                    PhpMixed::Null,
                    Some(Self::get_timeout() as f64),
                )?,
                CommandLine::Args(command) => Process::new(
                    command,
                    cwd.clone(),
                    None,
                    PhpMixed::Null,
                    Some(Self::get_timeout() as f64),
                )?,
            };

            process.start(None, IndexMap::new())?;

            // PHP's countActiveJobs tick: pump the process until it exits, checking the timeout
            // each round. The async sleep yields to the reactor so sibling jobs genuinely overlap.
            while process.is_running() {
                process.check_timeout()?;
                tokio::time::sleep(std::time::Duration::from_millis(1)).await;
            }

            // PHP resolves the promise with the Process regardless of its exit status; callers
            // inspect is_successful() themselves.
            Ok(process)
        })
    }

    fn output_handler(
        capture_output: bool,
        io: &mut Option<std::rc::Rc<std::cell::RefCell<dyn IOInterface>>>,
        r#type: &str,
        buffer: &BStr,
    ) {
        if capture_output {
            return;
        }

        if io.is_none() {
            use std::io::Write as _;
            let _ = std::io::stdout().write_all(buffer);

            return;
        }

        if Process::ERR == r#type {
            io.as_mut().unwrap().write_error_raw3(
                &legacy_lossy(buffer),
                false,
                io_interface::NORMAL,
            );
        } else {
            io.as_mut()
                .unwrap()
                .write_raw3(&legacy_lossy(buffer), false, io_interface::NORMAL);
        }
    }

    pub fn set_max_jobs(&self, max_jobs: i64) {
        self.max_jobs.set(max_jobs);
        *self.semaphore.borrow_mut() =
            std::rc::Rc::new(tokio::sync::Semaphore::new(max_jobs as usize));
    }

    pub fn reset_max_jobs(&self) {
        let max_jobs_env = Platform::get_env("COMPOSER_MAX_PARALLEL_PROCESSES");
        let max_jobs_env_mixed = match &max_jobs_env {
            Some(s) => PhpMixed::string(s.clone()),
            None => PhpMixed::Null,
        };
        if is_numeric(&max_jobs_env_mixed) {
            self.max_jobs.set(
                max_jobs_env
                    .as_deref()
                    .unwrap_or("0")
                    .parse()
                    .unwrap_or(0)
                    .clamp(1, 50),
            );
        } else {
            self.max_jobs.set(10);
        }
        *self.semaphore.borrow_mut() =
            std::rc::Rc::new(tokio::sync::Semaphore::new(self.max_jobs.get() as usize));
    }

    pub fn enable_async(&self) {
        self.allow_async.set(true);
    }

    pub fn split_lines(&self, output: impl AsRef<[u8]>) -> Vec<BString> {
        let output = trim(output);

        if output.is_empty() {
            vec![]
        } else {
            preg_split(php_regex!(r"{\r?\n}"), &output)
        }
    }

    /// Get any error output from the last command
    pub fn get_error_output(&self) -> BString {
        self.error_output.borrow().clone()
    }

    pub fn get_timeout() -> i64 {
        *TIMEOUT.lock().unwrap()
    }

    pub fn set_timeout<T: ToTimeoutSeconds>(timeout: T) {
        *TIMEOUT.lock().unwrap() = timeout.to_timeout_seconds();
    }

    /// Escapes a string to be used as a shell argument.
    pub fn escape(argument: impl AsRef<[u8]>) -> BString {
        Self::escape_argument(argument.as_ref())
    }

    fn output_command_run(&self, command: &CommandLine, cwd: Option<&Path>, r#async: bool) {
        Self::output_command_run_with(&self.io, command, cwd, r#async);
    }

    fn output_command_run_with(
        io: &Option<std::rc::Rc<std::cell::RefCell<dyn IOInterface>>>,
        command: &CommandLine,
        cwd: Option<&Path>,
        r#async: bool,
    ) {
        if io.is_none() || !io.as_ref().unwrap().is_debug() {
            return;
        }

        let command_string = match command {
            CommandLine::Shell(command) => command.clone(),
            CommandLine::Args(args) => implode(" ", &array_map(|arg| Self::escape(arg), args)),
        };
        let safe_command = preg_replace_callback(
            php_regex!(r"{://(?P<user>[^:/\s]+):(?P<password>[^@\s/]+)@}i"),
            |m: &PregMatches| -> anyhow::Result<BString> {
                // if the username looks like a long (12char+) hex string, or a modern github token (e.g. ghp_xxx, github_pat_xxx) we obfuscate that
                if preg_is_match(
                    GitHub::GITHUB_TOKEN_REGEX,
                    m.name("user").unwrap_or_default(),
                ) {
                    return Ok("://***:***@".into());
                }
                if preg_is_match(r"{^[a-f0-9]{12,}$}", m.name("user").unwrap_or_default()) {
                    return Ok("://***:***@".into());
                }

                Ok(bconcat!("://", m.name("user").unwrap_or_default(), ":***@"))
            },
            &command_string,
        )
        .expect("the replacement callback cannot fail");
        let safe_command = preg_replace(
            php_regex!(r"{--password (.*[^\\]') }"),
            "--password '***' ",
            &safe_command,
        );
        io.as_ref().unwrap().write_error(&format!(
            "Executing{} command ({}): {}",
            if r#async { " async" } else { "" },
            cwd.map_or("CWD".into(), |cwd| cwd.display().to_string()),
            safe_command
        ));
    }

    /// Escapes a string to be used as a shell argument for Symfony Process.
    fn escape_argument(argument: &[u8]) -> BString {
        let mut argument = BString::from(argument);
        if argument.is_empty() {
            return escapeshellarg(&argument);
        }

        if !Platform::is_windows() {
            return bconcat!("'", str_replace("'", "'\\''", &argument), "'");
        }

        // New lines break cmd.exe command parsing
        // and special chars like the fullwidth quote can be used to break out
        // of parameter encoding via "Best Fit" encoding conversion
        let mut translation: IndexMap<&str, &str> = IndexMap::new();
        translation.insert("\n", " ");
        translation.insert("\u{ff02}", "\"");
        translation.insert("\u{02ba}", "\"");
        translation.insert("\u{301d}", "\"");
        translation.insert("\u{301e}", "\"");
        translation.insert("\u{030e}", "\"");
        translation.insert("\u{ff1a}", ":");
        translation.insert("\u{0589}", ":");
        translation.insert("\u{2236}", ":");
        translation.insert("\u{ff0f}", "/");
        translation.insert("\u{2044}", "/");
        translation.insert("\u{2215}", "/");
        translation.insert("\u{00b4}", "/");
        argument = strtr_array(&argument, &translation);

        // In addition to whitespace, commas need quoting to preserve paths
        let mut quote = strpbrk(&argument, " \t,").is_some();
        let mut dquotes: usize = 0;
        // PHP: Preg::replace('/(\\\\*)"/', '$1$1\\"', $argument, -1, $dquotes)
        argument = preg_replace2(
            php_regex!(r#"/(\\*)"/"#),
            r#"$1$1\""#,
            &argument,
            -1,
            Some(&mut dquotes),
        );
        let meta = dquotes > 0 || preg_is_match(php_regex!(r"/%[^%]+%|![^!]+!/"), &argument);

        if !meta && !quote {
            quote = strpbrk(&argument, "^&|<>()").is_some();
        }

        if quote {
            argument = bconcat!("\"", preg_replace(r"/(\\*)$/", "$1$1", &argument), "\"");
        }

        if meta {
            argument = preg_replace(php_regex!(r#"/(["^&|<>()%])/"#), "^$1", &argument);
            argument = preg_replace(php_regex!(r"/(!)/"), "^^$1", &argument);
        }

        argument
    }

    pub fn requires_git_dir_env(&self, command: &CommandLine) -> bool {
        let cmd: Vec<BString> = match command {
            CommandLine::Shell(command) => explode(" ", command),
            CommandLine::Args(args) => args.clone(),
        };
        if cmd.first().is_none_or(|s| s != "git") {
            return false;
        }

        // TODO(php-semantics): the shim's array_intersect returns a Vec and drops the keys PHP
        // keeps from its first argument, so this compares values where PHP's `===` compares keys
        // as well. PHP answers false for every pattern below, the early return above leaving `git`
        // at index 0 so the intersection can never start where the pattern does; this answers
        // true, which is what the patterns were written for.
        for git_cmd in Self::GIT_CMDS_NEED_GIT_DIR.iter() {
            let cmd_strs: Vec<BString> = cmd.clone();
            let git_cmd_strs: Vec<BString> = git_cmd.iter().map(|&s| s.into()).collect();
            if array_intersect(&cmd_strs, &git_cmd_strs) == git_cmd_strs {
                return true;
            }
        }

        false
    }

    /// Resolves executable paths on Windows
    fn get_executable(name: &[u8]) -> BString {
        if in_array_strict(
            strtolower(name),
            &Self::BUILTIN_CMD_COMMANDS
                .iter()
                .map(|s| PhpMixed::string(*s))
                .collect::<Vec<_>>(),
        ) {
            return name.into();
        }

        // TODO(bytes,windows): ExecutableFinder looks the name up as a &str.
        let name = legacy_lossy(name).into_owned();
        let mut executables = EXECUTABLES.lock().unwrap();
        if !executables.contains_key(&name) {
            let path = ExecutableFinder::new().find(&name, Some(&name), &[]);
            if let Some(p) = path {
                executables.insert(name.clone(), p);
            }
        }

        executables.get(&name).cloned().unwrap_or(name).into()
    }
}

/// Helper trait: convert various command argument forms into a [`CommandLine`].
pub trait IntoExecCommand {
    fn into_exec_command(self) -> CommandLine;
}

impl IntoExecCommand for CommandLine {
    fn into_exec_command(self) -> CommandLine {
        self
    }
}

impl IntoExecCommand for &CommandLine {
    fn into_exec_command(self) -> CommandLine {
        self.clone()
    }
}

impl IntoExecCommand for &str {
    fn into_exec_command(self) -> CommandLine {
        CommandLine::Shell(self.into())
    }
}

impl IntoExecCommand for String {
    fn into_exec_command(self) -> CommandLine {
        CommandLine::Shell(self.into())
    }
}

impl IntoExecCommand for &String {
    fn into_exec_command(self) -> CommandLine {
        CommandLine::Shell(self.as_str().into())
    }
}

impl IntoExecCommand for BString {
    fn into_exec_command(self) -> CommandLine {
        CommandLine::Shell(self)
    }
}

impl IntoExecCommand for &BString {
    fn into_exec_command(self) -> CommandLine {
        CommandLine::Shell(self.clone())
    }
}

impl IntoExecCommand for Vec<String> {
    fn into_exec_command(self) -> CommandLine {
        CommandLine::Args(self.into_iter().map(BString::from).collect())
    }
}

impl IntoExecCommand for &Vec<String> {
    fn into_exec_command(self) -> CommandLine {
        CommandLine::Args(self.iter().map(|s| s.as_str().into()).collect())
    }
}

impl IntoExecCommand for Vec<BString> {
    fn into_exec_command(self) -> CommandLine {
        CommandLine::Args(self)
    }
}

impl IntoExecCommand for &Vec<BString> {
    fn into_exec_command(self) -> CommandLine {
        CommandLine::Args(self.clone())
    }
}

impl<const N: usize> IntoExecCommand for &[&str; N] {
    fn into_exec_command(self) -> CommandLine {
        CommandLine::Args(self.iter().map(|&s| s.into()).collect())
    }
}

impl IntoExecCommand for &[&str] {
    fn into_exec_command(self) -> CommandLine {
        CommandLine::Args(self.iter().map(|&s| s.into()).collect())
    }
}

impl IntoExecCommand for &[String] {
    fn into_exec_command(self) -> CommandLine {
        CommandLine::Args(self.iter().map(|s| s.as_str().into()).collect())
    }
}

impl<const N: usize> IntoExecCommand for &[String; N] {
    fn into_exec_command(self) -> CommandLine {
        CommandLine::Args(self.iter().map(|s| s.as_str().into()).collect())
    }
}

impl IntoExecCommand for &[BString] {
    fn into_exec_command(self) -> CommandLine {
        CommandLine::Args(self.to_vec())
    }
}

/// Models the `mixed &$output` parameter of `ProcessExecutor::execute` (cf.
/// `composer/src/Composer/Util/ProcessExecutor.php`). In PHP the behaviour is selected by
/// `func_num_args()` and `is_callable($output)`; here each behaviour is a distinct implementing type:
///
/// | PHP call site | meaning | implementor | `capture_output` |
/// |---|---|---|---|
/// | `execute($cmd)` | forward child output to STDOUT/STDERR (or the IO) | [`ProcessForwardOutput`] | `false` |
/// | `execute($cmd, $out)` | assign captured output back to `$out` | `&mut String` / `&mut PhpMixed` | `true` |
/// | `execute($cmd, $out)` where `$out` is unused | capture (suppress output) but discard it | `()` | `true` |
/// | `execute($cmd, $out)` from a plugin | capture, recording whether it was assigned | `&mut Option<BString>` | `true` |
/// | `execute($cmd, $cb)` | drive the child through the callback | `Box<dyn FnMut(&str, &BStr) -> anyhow::Result<bool>>` | `false` |
///
/// `capture_output` maps to PHP's `$this->captureOutput` (`func_num_args() > 3` in `doExecute`): when
/// `true`, `outputHandler` swallows the child output instead of echoing it, and the full output is
/// read back via `Process::getOutput()` afterwards.
pub trait IntoExecOutput<'a>: Sized {
    /// Whether the child's output is captured rather than forwarded to the terminal/IO.
    ///
    /// Mirrors `$this->captureOutput`: `true` makes `output_handler` swallow the stream so the buffer
    /// can be retrieved via `Process::get_output` (and handed to [`write_back`](Self::write_back)),
    /// `false` lets it pass through to STDOUT/STDERR or the IO.
    fn capture_output(&self) -> bool;

    /// Splits the output target into the two PHP cases handled by `is_callable($output)`:
    /// `Ok(callback)` for a user-supplied output handler (passed straight to `Process::run`),
    /// `Err(self)` for the capture/forward cases (a default handler is used and `self` is returned
    /// so the captured output can still be written back).
    fn to_callback(
        self,
    ) -> anyhow::Result<Box<dyn FnMut(&str, &BStr) -> anyhow::Result<bool>>, Self>;

    /// Assigns the captured output back to the by-reference target, mirroring PHP's
    /// `$output = $process->getOutput()`. Only meaningful when [`capture_output`](Self::capture_output)
    /// is `true`; a no-op for the forwarding and discarding variants.
    fn write_back(&mut self, value: BString);
}

/// `execute($cmd, $out)` where the caller never reads `$out` (e.g. `HomeCommand::openBrowser`):
/// output is captured (so the terminal stays quiet) but thrown away.
impl<'a> IntoExecOutput<'a> for () {
    fn capture_output(&self) -> bool {
        true
    }

    fn to_callback(
        self,
    ) -> anyhow::Result<Box<dyn FnMut(&str, &BStr) -> anyhow::Result<bool>>, Self> {
        Err(self)
    }

    fn write_back(&mut self, _value: BString) {}
}

/// `execute($cmd)` with no output argument: the child's output is forwarded to STDOUT/STDERR
/// (or the IO). Obtained via [`ProcessExecutor::FORWARD_OUTPUT`].
impl<'a> IntoExecOutput<'a> for ProcessForwardOutput {
    fn capture_output(&self) -> bool {
        false
    }

    fn to_callback(
        self,
    ) -> anyhow::Result<Box<dyn FnMut(&str, &BStr) -> anyhow::Result<bool>>, Self> {
        Err(self)
    }

    fn write_back(&mut self, _value: BString) {}
}

/// `execute($cmd, $out)` where `$out` holds a `mixed`/string value: output is captured and assigned
/// back as a [`PhpMixed::String`].
impl<'a> IntoExecOutput<'a> for &'a mut PhpMixed {
    fn capture_output(&self) -> bool {
        true
    }

    fn to_callback(
        self,
    ) -> anyhow::Result<Box<dyn FnMut(&str, &BStr) -> anyhow::Result<bool>>, Self> {
        Err(self)
    }

    fn write_back(&mut self, value: BString) {
        **self = PhpMixed::string(value);
    }
}

/// `execute($cmd, $out)` where `$out` is consumed as a string: output is captured and assigned back
/// directly.
impl<'a> IntoExecOutput<'a> for &'a mut String {
    fn capture_output(&self) -> bool {
        true
    }

    fn to_callback(
        self,
    ) -> anyhow::Result<Box<dyn FnMut(&str, &BStr) -> anyhow::Result<bool>>, Self> {
        Err(self)
    }

    fn write_back(&mut self, value: BString) {
        **self = legacy_lossy(&value).into_owned();
    }
}

/// `execute($cmd, $out)` where `$out` is consumed as a string of bytes.
impl<'a> IntoExecOutput<'a> for &'a mut BString {
    fn capture_output(&self) -> bool {
        true
    }

    fn to_callback(
        self,
    ) -> anyhow::Result<Box<dyn FnMut(&str, &BStr) -> anyhow::Result<bool>>, Self> {
        Err(self)
    }

    fn write_back(&mut self, value: BString) {
        **self = value;
    }
}

/// `execute($cmd, $out)` reached over the plugin RPC boundary, where the caller has to know whether
/// the output was assigned at all rather than just what it is: PHP leaves `$output` untouched when
/// the child is signaled, and `None` reproduces that by sending no value back across the boundary.
impl<'a> IntoExecOutput<'a> for &'a mut Option<BString> {
    fn capture_output(&self) -> bool {
        true
    }

    fn to_callback(
        self,
    ) -> anyhow::Result<Box<dyn FnMut(&str, &BStr) -> anyhow::Result<bool>>, Self> {
        Err(self)
    }

    fn write_back(&mut self, value: BString) {
        **self = Some(value);
    }
}

/// `execute($cmd, $cb)` where `$cb` is callable: the callback is passed straight to `Process::run`
/// as the output handler, so the caller drives the child's output itself (e.g. `Svn`'s streaming
/// handler). The `bool` return mirrors Symfony's ignored callback return value.
impl<'a> IntoExecOutput<'a> for Box<dyn FnMut(&str, &BStr) -> anyhow::Result<bool>> {
    fn capture_output(&self) -> bool {
        false
    }

    fn to_callback(
        self,
    ) -> anyhow::Result<Box<dyn FnMut(&str, &BStr) -> anyhow::Result<bool>>, Self> {
        Ok(self)
    }

    fn write_back(&mut self, _value: BString) {}
}

/// Helper: accept either `i64` or `PhpMixed` for `set_timeout`.
pub trait ToTimeoutSeconds {
    fn to_timeout_seconds(self) -> i64;
}

impl ToTimeoutSeconds for i64 {
    fn to_timeout_seconds(self) -> i64 {
        self
    }
}

impl ToTimeoutSeconds for PhpMixed {
    fn to_timeout_seconds(self) -> i64 {
        self.as_int().unwrap_or(0)
    }
}
