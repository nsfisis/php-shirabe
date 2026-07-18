//! ref: composer/src/Composer/Util/ProcessExecutor.php

use crate::io::IOInterface;
use crate::io::IOInterfaceImmutable;
use crate::io::io_interface;
use crate::util::GitHub;
use crate::util::Platform;
use indexmap::IndexMap;
use shirabe_external_packages::composer::pcre::{CaptureKey, Preg};
use shirabe_external_packages::seld::signal::SignalHandler;
use shirabe_external_packages::symfony::process::ExecutableFinder;
use shirabe_external_packages::symfony::process::Process;
use shirabe_external_packages::symfony::process::exception::ProcessSignaledException;
use shirabe_external_packages::symfony::process::exception::RuntimeException as SymfonyProcessRuntimeException;
use shirabe_php_shim::{
    LogicException, PHP_EOL, PhpMixed, array_intersect, array_map, call_user_func, escapeshellarg,
    explode, implode, in_array, is_array, is_dir, is_numeric, is_string, php_regex, rtrim, sprintf,
    str_replace, strcspn, strlen, strpbrk, strtolower, strtr_array, substr_replace, trim, usleep,
};
use std::sync::{LazyLock, Mutex};

static EXECUTABLES: LazyLock<Mutex<IndexMap<String, String>>> =
    LazyLock::new(|| Mutex::new(IndexMap::new()));

static TIMEOUT: LazyLock<Mutex<i64>> = LazyLock::new(|| Mutex::new(300));

#[derive(Debug)]
pub struct ProcessExecutor {
    /// @var bool
    pub(crate) capture_output: bool,
    /// @var string
    pub(crate) error_output: String,
    /// @var ?IOInterface
    pub(crate) io: Option<std::rc::Rc<std::cell::RefCell<dyn IOInterface>>>,
    /// @phpstan-var array<int, array<string, mixed>>
    jobs: IndexMap<i64, Job>,
    /// @var int
    running_jobs: i64,
    /// @var int
    max_jobs: i64,
    /// @var int
    id_gen: i64,
    /// @var bool
    allow_async: bool,
    /// Test-only mock state. `None` in production; set via [`ProcessExecutor::__expects`] in tests.
    /// Mirrors `composer/tests/Composer/Test/Mock/ProcessExecutorMock.php`.
    mock: Option<ProcessExecutorMockState>,
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
    /// `string|list<string>`: a `PhpMixed::String` or `PhpMixed::List`.
    pub cmd: PhpMixed,
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
    pub fn from_cmd(cmd: PhpMixed) -> Self {
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

struct Job {
    id: i64,
    status: i64,
    command: PhpMixed,
    cwd: Option<String>,
    process: Option<Process>,
    exception: Option<anyhow::Error>,
}

impl std::fmt::Debug for Job {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Job")
            .field("id", &self.id)
            .field("status", &self.status)
            .field("command", &self.command)
            .field("cwd", &self.cwd)
            .field("process", &self.process)
            .finish()
    }
}

/// Output target marking the "no `$output` argument" case of `ProcessExecutor::execute`, where the
/// child's output is forwarded to STDOUT/STDERR (or the IO) instead of captured. Use the
/// [`ProcessExecutor::FORWARD_OUTPUT`] constant rather than constructing this directly.
pub struct ProcessForwardOutput;

impl ProcessExecutor {
    pub const FORWARD_OUTPUT: ProcessForwardOutput = ProcessForwardOutput;

    const STATUS_QUEUED: i64 = 1;
    const STATUS_STARTED: i64 = 2;
    const STATUS_COMPLETED: i64 = 3;
    const STATUS_FAILED: i64 = 4;
    const STATUS_ABORTED: i64 = 5;

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
        let mut this = Self {
            capture_output: false,
            error_output: String::new(),
            io,
            jobs: IndexMap::new(),
            running_jobs: 0,
            max_jobs: 10,
            id_gen: 0,
            allow_async: false,
            mock: None,
        };
        this.reset_max_jobs();
        this
    }

    /// runs a process on the commandline
    pub fn execute<'o, C, O>(
        &mut self,
        command: C,
        output: O,
        cwd: Option<&str>,
    ) -> anyhow::Result<i64>
    where
        C: IntoExecCommand,
        O: IntoExecOutput<'o>,
    {
        let command = command.into_exec_command();
        self.do_execute(command, cwd, false, output)
    }

    /// Convenience wrapper used by phase-A code that calls
    /// `process.execute(&[String], &mut String, Option<&str>) == 0`.
    /// Forwards to `execute`, returning the status code (0 on Err for compatibility).
    pub fn execute_args(
        &mut self,
        command: &[String],
        output: &mut String,
        cwd: Option<&str>,
    ) -> i64 {
        let cmd = PhpMixed::List(
            command
                .iter()
                .map(|s| PhpMixed::String(s.clone()))
                .collect(),
        );
        let mut buf = PhpMixed::String(String::new());
        let rc = self.execute(cmd, &mut buf, cwd).unwrap_or(1);
        *output = buf.as_string().unwrap_or("").to_string();
        rc
    }

    /// runs a process on the commandline in TTY mode
    pub fn execute_tty<C>(&mut self, command: C, cwd: Option<&str>) -> anyhow::Result<i64>
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
        &mut self,
        command: PhpMixed,
        cwd: Option<&str>,
        env: Option<IndexMap<String, String>>,
        tty: bool,
        output: O,
    ) -> anyhow::Result<Option<i64>>
    where
        O: IntoExecOutput<'o>,
    {
        // On Windows, we don't rely on the OS to find the executable if possible to avoid lookups
        // in the current directory which could be untrusted. Instead we use the ExecutableFinder.

        let mut process: Process;
        if is_string(&command) {
            let mut command_str = command.as_string().unwrap_or("").to_string();
            if Platform::is_windows() {
                let mut m: IndexMap<CaptureKey, String> = IndexMap::new();
                if Preg::is_match3(php_regex!(r"{^([^:/\\]++) }"), &command_str, Some(&mut m)) {
                    let m1 = m.get(&CaptureKey::ByIndex(1)).cloned().unwrap_or_default();
                    command_str = substr_replace(
                        &command_str,
                        &Self::escape(&Self::get_executable(&m1)),
                        0,
                        strlen(&m1) as usize,
                    );
                }
            }

            process = Process::from_shell_commandline(
                &command_str,
                cwd,
                env.clone(),
                PhpMixed::Null,
                Some(Self::get_timeout() as f64),
            )?;
        } else if let PhpMixed::List(ref list) = command {
            let mut cmd_vec: Vec<String> = list
                .iter()
                .map(|v| v.as_string().unwrap_or("").to_string())
                .collect();
            if Platform::is_windows() && strlen(&cmd_vec[0]) == strcspn(&cmd_vec[0], ":/\\") as i64
            {
                cmd_vec[0] = Self::get_executable(&cmd_vec[0]);
            }

            process = Process::new(
                cmd_vec,
                cwd.map(String::from),
                env,
                PhpMixed::Null,
                Some(Self::get_timeout() as f64),
            )?;
        } else {
            return Err(LogicException {
                message: "Invalid command type".to_string(),
                code: 0,
            }
            .into());
        }

        if !Platform::is_windows() && tty {
            // PHP: try { $process->setTty(true); } catch (RuntimeException $e) { /* ignore */ }
            if let Err(e) = process.set_tty(true)
                && e.downcast_ref::<SymfonyProcessRuntimeException>().is_none()
            {
                return Err(e);
            }
            // ignore TTY enabling errors
        }

        let io_for_signal = self.io.clone();
        let signal_handler = SignalHandler::create(
            vec![
                SignalHandler::SIGINT.to_string(),
                SignalHandler::SIGTERM.to_string(),
                SignalHandler::SIGHUP.to_string(),
            ],
            Box::new(move |signal: String, _h: &SignalHandler| {
                if let Some(io) = &io_for_signal {
                    io.write_error(&format!(
                        "Received {}, aborting when child process is done",
                        signal
                    ));
                }
            }),
        );

        let result: anyhow::Result<()> = (|| -> anyhow::Result<()> {
            match output.to_callback() {
                Ok(callback) => {
                    process.run(Some(callback), IndexMap::new())?;
                }
                Err(mut output) => {
                    let capture_output = self.capture_output;
                    let mut io = self.io.clone();
                    let callback = move |r#type: &str, buffer: &str| {
                        Self::output_handler(capture_output, &mut io, r#type, buffer);
                        false
                    };
                    process.run(Some(Box::new(callback)), IndexMap::new())?;
                    if self.capture_output {
                        output.write_back(process.get_output()?);
                    }
                }
            }

            self.error_output = process.get_error_output()?;
            Ok(())
        })();
        let final_result: anyhow::Result<()> = match result {
            Ok(()) => Ok(()),
            Err(e) => {
                if let Some(pse) = e.downcast_ref::<ProcessSignaledException>() {
                    if signal_handler.is_triggered() {
                        // exiting as we were signaled and the child process exited too due to the signal
                        signal_handler.exit_with_last_signal();
                    }
                    let _ = pse;
                    Ok(())
                } else {
                    signal_handler.unregister();
                    return Err(e);
                }
            }
        };
        signal_handler.unregister();
        final_result?;

        Ok(process.get_exit_code())
    }

    fn do_execute<'o, O>(
        &mut self,
        command: PhpMixed,
        cwd: Option<&str>,
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

        self.capture_output = output.capture_output();
        self.error_output = String::new();

        let mut env: Option<IndexMap<String, String>> = None;

        let requires_git_dir_env = self.requires_git_dir_env(&command);
        if let Some(cwd) = cwd
            && requires_git_dir_env
        {
            let is_bare_repository = !is_dir(format!("{}/.git", rtrim(cwd, Some("/"))));
            if is_bare_repository {
                let mut config_value = PhpMixed::String(String::new());
                let mut git_env: IndexMap<String, String> = IndexMap::new();
                git_env.insert("GIT_DIR".to_string(), cwd.to_string());
                self.run_process(
                    PhpMixed::List(vec![
                        PhpMixed::String("git".to_string()),
                        PhpMixed::String("config".to_string()),
                        PhpMixed::String("safe.bareRepository".to_string()),
                    ]),
                    Some(cwd),
                    Some(git_env.clone()),
                    tty,
                    &mut config_value,
                )?;
                let trimmed = trim(config_value.as_string().unwrap_or(""), None);
                if trimmed == "explicit" {
                    env = Some(git_env);
                }
            }
        }

        Ok(self
            .run_process(command, cwd, env, tty, output)?
            .unwrap_or(0))
    }

    /// Mock replacement for `do_execute` when [`Self::mock`] is set (cf.
    /// `ProcessExecutorMock::doExecute`). Logs the command, matches it against the head of the
    /// expectation queue (exact `===`), pops on match (firing the optional callback), falls back to
    /// the default handler in non-strict mode, or panics in strict mode. Emits stdout/stderr through
    /// the output target and records `error_output`.
    fn mock_do_execute<'o, O>(
        &mut self,
        command: PhpMixed,
        cwd: Option<&str>,
        output: O,
    ) -> anyhow::Result<i64>
    where
        O: IntoExecOutput<'o>,
    {
        let capture_output = output.capture_output();
        self.capture_output = capture_output;
        self.error_output = String::new();

        let command_string = if is_array(&command) {
            match &command {
                PhpMixed::List(l) => implode(
                    " ",
                    &l.iter()
                        .map(|v| v.as_string().unwrap_or("").to_string())
                        .collect::<Vec<_>>(),
                ),
                PhpMixed::Array(m) => implode(
                    " ",
                    &m.values()
                        .map(|v| v.as_string().unwrap_or("").to_string())
                        .collect::<Vec<_>>(),
                ),
                _ => String::new(),
            }
        } else {
            command.as_string().unwrap_or("").to_string()
        };

        let mock = self.mock.as_mut().unwrap();
        mock.log.push(command_string.clone());

        let matched = mock
            .expectations
            .as_ref()
            .map(|exps| !exps.is_empty() && exps[0].cmd == command)
            .unwrap_or(false);

        let (stdout, stderr, r#return);
        if matched {
            let mut expect = mock.expectations.as_mut().unwrap().remove(0);
            stdout = expect.stdout.clone();
            stderr = expect.stderr.clone();
            r#return = expect.r#return;
            if let Some(callback) = expect.callback.as_mut() {
                callback();
            }
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
            panic!(
                "Received unexpected command {:?} in \"{}\"{}{}{}Received calls:{}{}",
                command,
                cwd.unwrap_or(""),
                PHP_EOL,
                expected,
                PHP_EOL,
                PHP_EOL,
                received
            );
        }

        // Feed stdout/stderr through the output target, mirroring the PHP `$callback(...)` calls.
        match output.to_callback() {
            Ok(mut callback) => {
                if !stdout.is_empty() {
                    callback(Process::OUT, &stdout);
                }
                if !stderr.is_empty() {
                    callback(Process::ERR, &stderr);
                }
            }
            Err(mut out) => {
                let mut io = self.io.clone();
                if !stdout.is_empty() {
                    Self::output_handler(capture_output, &mut io, Process::OUT, &stdout);
                }
                if !stderr.is_empty() {
                    Self::output_handler(capture_output, &mut io, Process::ERR, &stderr);
                }
                if capture_output {
                    out.write_back(stdout.clone());
                }
            }
        }

        self.error_output = stderr;

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
        self.mock = Some(ProcessExecutorMockState {
            expectations: Some(expectations),
            strict,
            default_handler,
            log: Vec::new(),
        });
    }

    /// For testing only. Asserts all configured expectations were consumed (cf.
    /// `ProcessExecutorMock::assertComplete`). Panics with the remaining/received commands otherwise.
    pub fn __assert_complete(&self) {
        let Some(mock) = self.mock.as_ref() else {
            return;
        };
        // Not configured to expect anything, so no need to react here.
        let Some(expectations) = mock.expectations.as_ref() else {
            return;
        };

        if !expectations.is_empty() {
            let remaining: Vec<String> = expectations
                .iter()
                .map(|expect| {
                    if is_array(&expect.cmd) {
                        match &expect.cmd {
                            PhpMixed::List(l) => implode(
                                " ",
                                &l.iter()
                                    .map(|v| v.as_string().unwrap_or("").to_string())
                                    .collect::<Vec<_>>(),
                            ),
                            PhpMixed::Array(m) => implode(
                                " ",
                                &m.values()
                                    .map(|v| v.as_string().unwrap_or("").to_string())
                                    .collect::<Vec<_>>(),
                            ),
                            _ => String::new(),
                        }
                    } else {
                        expect.cmd.as_string().unwrap_or("").to_string()
                    }
                })
                .collect();
            panic!(
                "There are still {} expected process calls which have not been consumed:{}{}{}{}Received calls:{}{}",
                expectations.len(),
                PHP_EOL,
                remaining.join(PHP_EOL),
                PHP_EOL,
                PHP_EOL,
                PHP_EOL,
                mock.log.join(PHP_EOL)
            );
        }
    }

    /// starts a process on the commandline in async mode
    pub async fn execute_async<C>(
        &mut self,
        command: C,
        cwd: Option<&str>,
    ) -> anyhow::Result<Process>
    where
        C: IntoExecCommand,
    {
        let command = command.into_exec_command();
        if self.mock.is_some() {
            // PHP resolves the promise with a Process mock whose getOutput/isSuccessful/getExitCode
            // reflect the doExecute result. We consume the expectation/log via mock_do_execute, but
            // cannot fabricate a Process mock here: Process has no test seam in the external-packages
            // crate (out of scope to modify). No portable test currently exercises the async mock
            // path, so leave it unimplemented rather than returning a misleading Process.
            let mut captured = PhpMixed::String(String::new());
            let _result = self.mock_do_execute(command, cwd, &mut captured)?;
            todo!("ProcessExecutorMock async path needs a Process mock seam in external-packages");
        }
        if !self.allow_async {
            return Err(LogicException {
                message: "You must use the ProcessExecutor instance which is part of a Composer\\Loop instance to be able to run async processes".to_string(),
                code: 0,
            }
            .into());
        }

        let id = self.id_gen;
        self.id_gen += 1;
        let job = Job {
            id,
            status: Self::STATUS_QUEUED,
            command,
            cwd: cwd.map(ToOwned::to_owned),
            process: None,
            exception: None,
        };

        self.jobs.insert(id, job);

        if self.running_jobs < self.max_jobs {
            self.start_job(id);
        }

        // Drive the job to completion (serial pump). PHP resolves the promise with the Process
        // once it stops running; here we await by pumping count_active_jobs and then hand back the
        // Process (or the rejection captured during start_job).
        self.wait_id(Some(id))?;

        let mut job = self.jobs.shift_remove(&id).unwrap();
        if let Some(process) = job.process.take() {
            Ok(process)
        } else if let Some(e) = job.exception.take() {
            Err(e)
        } else {
            Err(anyhow::anyhow!(
                "ProcessExecutor async job completed without a process"
            ))
        }
    }

    fn output_handler(
        capture_output: bool,
        io: &mut Option<std::rc::Rc<std::cell::RefCell<dyn IOInterface>>>,
        r#type: &str,
        buffer: &str,
    ) {
        if capture_output {
            return;
        }

        if io.is_none() {
            print!("{}", buffer);

            return;
        }

        if Process::ERR == r#type {
            io.as_mut()
                .unwrap()
                .write_error_raw3(buffer, false, io_interface::NORMAL);
        } else {
            io.as_mut()
                .unwrap()
                .write_raw3(buffer, false, io_interface::NORMAL);
        }
    }

    fn start_job(&mut self, id: i64) {
        let job_status = self.jobs.get(&id).map(|j| j.status);
        if job_status != Some(Self::STATUS_QUEUED) {
            return;
        }

        // start job
        if let Some(job) = self.jobs.get_mut(&id) {
            job.status = Self::STATUS_STARTED;
        }
        self.running_jobs += 1;

        let (command, cwd) = {
            let j = self.jobs.get(&id).unwrap();
            (j.command.clone(), j.cwd.clone())
        };

        self.output_command_run(&command, cwd.as_deref(), true);

        let process_result: anyhow::Result<Process> = if is_string(&command) {
            Process::from_shell_commandline(
                command.as_string().unwrap_or(""),
                cwd.as_deref(),
                None,
                PhpMixed::Null,
                Some(Self::get_timeout() as f64),
            )
        } else if let PhpMixed::List(ref list) = command {
            Process::new(
                list.iter()
                    .map(|v| v.as_string().unwrap_or("").to_string())
                    .collect(),
                cwd.clone(),
                None,
                PhpMixed::Null,
                Some(Self::get_timeout() as f64),
            )
        } else {
            Err(LogicException {
                message: "Invalid command type".to_string(),
                code: 0,
            }
            .into())
        };
        let process = match process_result {
            Ok(p) => p,
            Err(e) => {
                // PHP: $job['reject']($e) — record the rejection and settle the job as failed.
                if let Some(job) = self.jobs.get_mut(&id) {
                    job.status = Self::STATUS_FAILED;
                    job.exception = Some(e);
                }
                self.mark_job_done();
                return;
            }
        };

        if let Some(job) = self.jobs.get_mut(&id) {
            job.process = Some(process);
        }

        // PHP: $process->start($callback); — we operate on the stored job.process directly
        let start_result = if let Some(job) = self.jobs.get_mut(&id)
            && let Some(p) = job.process.as_mut()
        {
            p.start(None, IndexMap::new())
        } else {
            Ok(())
        };
        if let Err(e) = start_result {
            // PHP: $job['reject']($e) — record the rejection and settle the job as failed.
            if let Some(job) = self.jobs.get_mut(&id) {
                job.status = Self::STATUS_FAILED;
                job.exception = Some(e);
            }
            self.mark_job_done();
        }
    }

    pub fn set_max_jobs(&mut self, max_jobs: i64) {
        self.max_jobs = max_jobs;
    }

    pub fn reset_max_jobs(&mut self) {
        let max_jobs_env = Platform::get_env("COMPOSER_MAX_PARALLEL_PROCESSES");
        let max_jobs_env_mixed = match &max_jobs_env {
            Some(s) => PhpMixed::String(s.clone()),
            None => PhpMixed::Null,
        };
        if is_numeric(&max_jobs_env_mixed) {
            self.max_jobs = max_jobs_env
                .as_deref()
                .unwrap_or("0")
                .parse()
                .unwrap_or(0)
                .clamp(1, 50);
        } else {
            self.max_jobs = 10;
        }
    }

    /// @param  ?int $index job id
    pub fn wait(&mut self) -> anyhow::Result<()> {
        self.wait_id(None)
    }

    pub fn wait_id(&mut self, index: Option<i64>) -> anyhow::Result<()> {
        loop {
            if 0 == self.count_active_jobs(index)? {
                return Ok(());
            }

            usleep(1000);
        }
    }

    /// @internal
    pub fn enable_async(&mut self) {
        self.allow_async = true;
    }

    /// @internal
    pub fn count_active_jobs(&mut self, index: Option<i64>) -> anyhow::Result<i64> {
        // tick
        let ids: Vec<i64> = self.jobs.keys().copied().collect();
        for id in &ids {
            let (status, has_process) = {
                let j = self.jobs.get(id).unwrap();
                (j.status, j.process.is_some())
            };
            if status == Self::STATUS_STARTED && has_process {
                let is_running = self
                    .jobs
                    .get_mut(id)
                    .and_then(|j| j.process.as_mut())
                    .map(|p| p.is_running())
                    .unwrap_or(false);
                if !is_running {
                    // PHP: call_user_func($job['resolve'], $job['process']) — the .then handler
                    // marks the job completed/failed based on the process exit status.
                    let successful = self
                        .jobs
                        .get_mut(id)
                        .and_then(|j| j.process.as_mut())
                        .map(|p| p.is_successful())
                        .unwrap_or(false);
                    if let Some(job) = self.jobs.get_mut(id) {
                        job.status = if successful {
                            Self::STATUS_COMPLETED
                        } else {
                            Self::STATUS_FAILED
                        };
                    }
                    self.mark_job_done();
                }

                if let Some(p) = self.jobs.get_mut(id).and_then(|j| j.process.as_mut()) {
                    p.check_timeout()?;
                }
            }

            if self.running_jobs < self.max_jobs {
                let status_now = self.jobs.get(id).map(|j| j.status).unwrap_or(0);
                if status_now == Self::STATUS_QUEUED {
                    self.start_job(*id);
                }
            }
        }

        if let Some(index) = index {
            return Ok(
                if self.jobs.get(&index).map(|j| j.status).unwrap_or(0) < Self::STATUS_COMPLETED {
                    1
                } else {
                    0
                },
            );
        }

        let mut active: i64 = 0;
        let ids2: Vec<i64> = self.jobs.keys().copied().collect();
        for id in ids2 {
            let status = self.jobs.get(&id).map(|j| j.status).unwrap_or(0);
            if status < Self::STATUS_COMPLETED {
                active += 1;
            } else {
                self.jobs.shift_remove(&id);
            }
        }

        Ok(active)
    }

    fn mark_job_done(&mut self) {
        self.running_jobs -= 1;
    }

    /// @return string[]
    pub fn split_lines(&self, output: &str) -> Vec<String> {
        let output = trim(output, None);

        if output.is_empty() {
            vec![]
        } else {
            Preg::split(php_regex!(r"{\r?\n}"), &output)
        }
    }

    /// Get any error output from the last command
    pub fn get_error_output(&self) -> &str {
        &self.error_output
    }

    /// @return int the timeout in seconds
    pub fn get_timeout() -> i64 {
        *TIMEOUT.lock().unwrap()
    }

    /// @param  int  $timeout the timeout in seconds
    pub fn set_timeout<T: ToTimeoutSeconds>(timeout: T) {
        *TIMEOUT.lock().unwrap() = timeout.to_timeout_seconds();
    }

    /// Escapes a string to be used as a shell argument.
    pub fn escape(argument: &str) -> String {
        Self::escape_argument(argument)
    }

    /// @param string|list<string> $command
    fn output_command_run(&self, command: &PhpMixed, cwd: Option<&str>, r#async: bool) {
        if self.io.is_none() || !self.io.as_ref().unwrap().is_debug() {
            return;
        }

        let command_string = if is_string(command) {
            command.as_string().unwrap_or("").to_string()
        } else if let PhpMixed::List(list) = command {
            let parts: Vec<String> = array_map(
                |v| Self::escape(v.as_string().unwrap_or("")),
                &list.to_vec(),
            );
            implode(" ", &parts)
        } else {
            String::new()
        };
        let safe_command = Preg::replace_callback(
            php_regex!(r"{://(?P<user>[^:/\s]+):(?P<password>[^@\s/]+)@}i"),
            |m: &IndexMap<CaptureKey, String>| -> String {
                let user_key = CaptureKey::ByName("user".to_string());
                // if the username looks like a long (12char+) hex string, or a modern github token (e.g. ghp_xxx, github_pat_xxx) we obfuscate that
                if Preg::is_match(
                    GitHub::GITHUB_TOKEN_REGEX,
                    m.get(&user_key).cloned().unwrap_or_default().as_str(),
                ) {
                    return "://***:***@".to_string();
                }
                if Preg::is_match(
                    r"{^[a-f0-9]{12,}$}",
                    m.get(&user_key).cloned().unwrap_or_default().as_str(),
                ) {
                    return "://***:***@".to_string();
                }

                format!("://{}:***@", m.get(&user_key).cloned().unwrap_or_default())
            },
            &command_string,
        );
        let safe_command = Preg::replace(
            php_regex!(r"{--password (.*[^\\]') }"),
            "--password '***' ",
            &safe_command,
        );
        self.io.as_ref().unwrap().write_error(&format!(
            "Executing{} command ({}): {}",
            if r#async { " async" } else { "" },
            cwd.unwrap_or("CWD"),
            safe_command
        ));
    }

    /// Escapes a string to be used as a shell argument for Symfony Process.
    fn escape_argument(argument: &str) -> String {
        let mut argument = argument.to_string();
        if argument.is_empty() {
            return escapeshellarg(&argument);
        }

        if !Platform::is_windows() {
            return format!("'{}'", str_replace("'", "'\\''", &argument));
        }

        // New lines break cmd.exe command parsing
        // and special chars like the fullwidth quote can be used to break out
        // of parameter encoding via "Best Fit" encoding conversion
        let mut translation: IndexMap<String, String> = IndexMap::new();
        translation.insert("\n".to_string(), " ".to_string());
        translation.insert("\u{ff02}".to_string(), "\"".to_string());
        translation.insert("\u{02ba}".to_string(), "\"".to_string());
        translation.insert("\u{301d}".to_string(), "\"".to_string());
        translation.insert("\u{301e}".to_string(), "\"".to_string());
        translation.insert("\u{030e}".to_string(), "\"".to_string());
        translation.insert("\u{ff1a}".to_string(), ":".to_string());
        translation.insert("\u{0589}".to_string(), ":".to_string());
        translation.insert("\u{2236}".to_string(), ":".to_string());
        translation.insert("\u{ff0f}".to_string(), "/".to_string());
        translation.insert("\u{2044}".to_string(), "/".to_string());
        translation.insert("\u{2215}".to_string(), "/".to_string());
        translation.insert("\u{00b4}".to_string(), "/".to_string());
        argument = strtr_array(&argument, &translation);

        // In addition to whitespace, commas need quoting to preserve paths
        let mut quote = strpbrk(&argument, " \t,").is_some();
        let mut dquotes: usize = 0;
        // PHP: Preg::replace('/(\\\\*)"/', '$1$1\\"', $argument, -1, $dquotes)
        argument = Preg::replace5(
            php_regex!(r#"/(\\*)"/"#),
            r#"$1$1\""#,
            &argument,
            -1,
            &mut dquotes,
        );
        let meta = dquotes > 0 || Preg::is_match(php_regex!(r"/%[^%]+%|![^!]+!/"), &argument);

        if !meta && !quote {
            quote = strpbrk(&argument, "^&|<>()").is_some();
        }

        if quote {
            argument = format!("\"{}\"", Preg::replace(r"/(\\*)$/", "$1$1", &argument));
        }

        if meta {
            argument = Preg::replace(php_regex!(r#"/(["^&|<>()%])/"#), "^$1", &argument);
            argument = Preg::replace(php_regex!(r"/(!)/"), "^^$1", &argument);
        }

        argument
    }

    /// @param string[]|string $command
    pub fn requires_git_dir_env(&self, command: &PhpMixed) -> bool {
        let cmd: Vec<String> = if !is_array(command) {
            explode(" ", command.as_string().unwrap_or(""))
        } else {
            match command {
                PhpMixed::List(l) => l
                    .iter()
                    .map(|v| v.as_string().unwrap_or("").to_string())
                    .collect(),
                PhpMixed::Array(m) => m
                    .values()
                    .map(|v| v.as_string().unwrap_or("").to_string())
                    .collect(),
                _ => vec![],
            }
        };
        if cmd.first().map(|s| s.as_str()) != Some("git") {
            return false;
        }

        for git_cmd in Self::GIT_CMDS_NEED_GIT_DIR.iter() {
            let cmd_strs: Vec<String> = cmd.clone();
            let git_cmd_strs: Vec<String> = git_cmd.iter().map(|s| s.to_string()).collect();
            if array_intersect(&cmd_strs, &git_cmd_strs) == git_cmd_strs {
                return true;
            }
        }

        false
    }

    /// Resolves executable paths on Windows
    fn get_executable(name: &str) -> String {
        if in_array(
            PhpMixed::String(strtolower(name)),
            &PhpMixed::List(
                Self::BUILTIN_CMD_COMMANDS
                    .iter()
                    .map(|s| PhpMixed::String(s.to_string()))
                    .collect(),
            ),
            true,
        ) {
            return name.to_string();
        }

        let mut executables = EXECUTABLES.lock().unwrap();
        if !executables.contains_key(name) {
            let path = ExecutableFinder::new().find(name, Some(name), &[]);
            if let Some(p) = path {
                executables.insert(name.to_string(), p);
            }
        }

        executables
            .get(name)
            .cloned()
            .unwrap_or_else(|| name.to_string())
    }
}

/// Phase B helper trait: convert various command argument forms into `PhpMixed`.
pub trait IntoExecCommand {
    fn into_exec_command(self) -> PhpMixed;
}

impl IntoExecCommand for PhpMixed {
    fn into_exec_command(self) -> PhpMixed {
        self
    }
}

impl IntoExecCommand for &PhpMixed {
    fn into_exec_command(self) -> PhpMixed {
        self.clone()
    }
}

impl IntoExecCommand for &str {
    fn into_exec_command(self) -> PhpMixed {
        PhpMixed::String(self.to_string())
    }
}

impl IntoExecCommand for String {
    fn into_exec_command(self) -> PhpMixed {
        PhpMixed::String(self)
    }
}

impl IntoExecCommand for &String {
    fn into_exec_command(self) -> PhpMixed {
        PhpMixed::String(self.clone())
    }
}

impl IntoExecCommand for Vec<String> {
    fn into_exec_command(self) -> PhpMixed {
        PhpMixed::List(self.into_iter().map(PhpMixed::String).collect())
    }
}

impl IntoExecCommand for &Vec<String> {
    fn into_exec_command(self) -> PhpMixed {
        PhpMixed::List(self.iter().map(|s| PhpMixed::String(s.clone())).collect())
    }
}

impl<const N: usize> IntoExecCommand for &[&str; N] {
    fn into_exec_command(self) -> PhpMixed {
        PhpMixed::List(
            self.iter()
                .map(|s| PhpMixed::String(s.to_string()))
                .collect(),
        )
    }
}

impl IntoExecCommand for &[&str] {
    fn into_exec_command(self) -> PhpMixed {
        PhpMixed::List(
            self.iter()
                .map(|s| PhpMixed::String(s.to_string()))
                .collect(),
        )
    }
}

impl IntoExecCommand for &[String] {
    fn into_exec_command(self) -> PhpMixed {
        PhpMixed::List(self.iter().map(|s| PhpMixed::String(s.clone())).collect())
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
/// | `execute($cmd, $cb)` | drive the child through the callback | `Box<dyn FnMut(&str, &str) -> bool>` | `false` |
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
    fn to_callback(self) -> anyhow::Result<Box<dyn FnMut(&str, &str) -> bool>, Self>;

    /// Assigns the captured output back to the by-reference target, mirroring PHP's
    /// `$output = $process->getOutput()`. Only meaningful when [`capture_output`](Self::capture_output)
    /// is `true`; a no-op for the forwarding and discarding variants.
    fn write_back(&mut self, value: String);
}

/// `execute($cmd, $out)` where the caller never reads `$out` (e.g. `HomeCommand::openBrowser`):
/// output is captured (so the terminal stays quiet) but thrown away.
impl<'a> IntoExecOutput<'a> for () {
    fn capture_output(&self) -> bool {
        true
    }

    fn to_callback(self) -> anyhow::Result<Box<dyn FnMut(&str, &str) -> bool>, Self> {
        Err(self)
    }

    fn write_back(&mut self, _value: String) {}
}

/// `execute($cmd)` with no output argument: the child's output is forwarded to STDOUT/STDERR
/// (or the IO). Obtained via [`ProcessExecutor::FORWARD_OUTPUT`].
impl<'a> IntoExecOutput<'a> for ProcessForwardOutput {
    fn capture_output(&self) -> bool {
        false
    }

    fn to_callback(self) -> anyhow::Result<Box<dyn FnMut(&str, &str) -> bool>, Self> {
        Err(self)
    }

    fn write_back(&mut self, _value: String) {}
}

/// `execute($cmd, $out)` where `$out` holds a `mixed`/string value: output is captured and assigned
/// back as a [`PhpMixed::String`].
impl<'a> IntoExecOutput<'a> for &'a mut PhpMixed {
    fn capture_output(&self) -> bool {
        true
    }

    fn to_callback(self) -> anyhow::Result<Box<dyn FnMut(&str, &str) -> bool>, Self> {
        Err(self)
    }

    fn write_back(&mut self, value: String) {
        **self = PhpMixed::String(value);
    }
}

/// `execute($cmd, $out)` where `$out` is consumed as a string: output is captured and assigned back
/// directly.
impl<'a> IntoExecOutput<'a> for &'a mut String {
    fn capture_output(&self) -> bool {
        true
    }

    fn to_callback(self) -> anyhow::Result<Box<dyn FnMut(&str, &str) -> bool>, Self> {
        Err(self)
    }

    fn write_back(&mut self, value: String) {
        **self = value;
    }
}

/// `execute($cmd, $cb)` where `$cb` is callable: the callback is passed straight to `Process::run`
/// as the output handler, so the caller drives the child's output itself (e.g. `Svn`'s streaming
/// handler). The `bool` return mirrors Symfony's ignored callback return value.
impl<'a> IntoExecOutput<'a> for Box<dyn FnMut(&str, &str) -> bool> {
    fn capture_output(&self) -> bool {
        false
    }

    fn to_callback(self) -> anyhow::Result<Box<dyn FnMut(&str, &str) -> bool>, Self> {
        Ok(self)
    }

    fn write_back(&mut self, _value: String) {}
}

/// Phase B helper: accept either `i64` or `PhpMixed` for `set_timeout`.
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

// Suppress unused-import warnings.
#[allow(dead_code)]
const _USE_PARITY: () = {
    let _ = call_user_func::<PhpMixed>;
    let _ = sprintf;
};
