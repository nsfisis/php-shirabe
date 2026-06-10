//! ref: composer/src/Composer/Util/ProcessExecutor.php

use crate::io::io_interface;
use anyhow::Result;
use indexmap::IndexMap;
use std::sync::{LazyLock, Mutex};

use shirabe_external_packages::composer::pcre::{CaptureKey, Preg};
use shirabe_external_packages::seld::signal::SignalHandler;
use shirabe_external_packages::symfony::process::ExecutableFinder;
use shirabe_external_packages::symfony::process::Process;
use shirabe_external_packages::symfony::process::exception::ProcessSignaledException;
use shirabe_external_packages::symfony::process::exception::RuntimeException as SymfonyProcessRuntimeException;
use shirabe_php_shim::{
    LogicException, PhpMixed, RuntimeException, array_intersect, array_map, call_user_func,
    defined, escapeshellarg, explode, implode, in_array, is_array, is_callable, is_dir, is_numeric,
    is_string, max, min, rtrim, sprintf, str_replace, strcspn, strlen, strpbrk, strtolower,
    strtr_array, substr_replace, trim, usleep,
};

use crate::io::IOInterface;
use crate::io::IOInterfaceImmutable;
use crate::util::GitHub;
use crate::util::Platform;

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

impl ProcessExecutor {
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
        };
        this.reset_max_jobs();
        this
    }

    /// runs a process on the commandline
    ///
    /// @param  string|non-empty-list<string> $command the command to execute
    /// @param  mixed   $output  the output will be written into this var if passed by ref
    ///                          if a callable is passed it will be used as output handler
    /// @param  null|string $cwd     the working directory
    /// @return int     statuscode
    pub fn execute<'o, C, O, W>(&mut self, command: C, output: O, cwd: W) -> Result<i64>
    where
        C: IntoExecCommand,
        O: IntoExecOutput<'o>,
        W: IntoExecCwd,
    {
        let command = command.into_exec_command();
        let mut output = output.into_exec_output();
        let cwd_storage;
        let cwd_ref: Option<&str> = match cwd.into_exec_cwd() {
            Some(s) => {
                cwd_storage = s;
                Some(cwd_storage.as_str())
            }
            None => None,
        };
        // PHP: func_num_args() > 1
        let has_output_arg = output.has_output();
        let rc = if has_output_arg {
            let mut buf = PhpMixed::Null;
            let result = self.do_execute(command, cwd_ref, false, Some(&mut buf))?;
            output.write_back(buf);
            result
        } else {
            self.do_execute(command, cwd_ref, false, None)?
        };
        Ok(rc)
    }

    /// Convenience wrapper used by phase-A code that calls
    /// `process.execute(&[String], &mut String, Option<&str>) == 0`.
    /// Forwards to `execute`, returning the status code (0 on Err for compatibility).
    pub fn execute_args<W>(&mut self, command: &[String], output: &mut String, cwd: W) -> i64
    where
        W: IntoExecCwd,
    {
        let cmd = PhpMixed::List(
            command
                .iter()
                .map(|s| Box::new(PhpMixed::String(s.clone())))
                .collect(),
        );
        let mut buf = PhpMixed::String(String::new());
        let cwd_storage;
        let cwd_ref: Option<&str> = match cwd.into_exec_cwd() {
            Some(s) => {
                cwd_storage = s;
                Some(cwd_storage.as_str())
            }
            None => None,
        };
        let rc = self.execute(cmd, Some(&mut buf), cwd_ref).unwrap_or(1);
        *output = buf.as_string().unwrap_or("").to_string();
        rc
    }

    /// runs a process on the commandline in TTY mode
    pub fn execute_tty<C, W>(&mut self, command: C, cwd: W) -> Result<i64>
    where
        C: IntoExecCommand,
        W: IntoExecCwd,
    {
        let command = command.into_exec_command();
        let cwd_storage;
        let cwd_ref: Option<&str> = match cwd.into_exec_cwd() {
            Some(s) => {
                cwd_storage = s;
                Some(cwd_storage.as_str())
            }
            None => None,
        };
        if Platform::is_tty(None) {
            return self.do_execute(command, cwd_ref, true, None);
        }

        self.do_execute(command, cwd_ref, false, None)
    }

    /// @param  string|non-empty-list<string> $command
    /// @param  array<string, string>|null $env
    fn run_process(
        &mut self,
        command: PhpMixed,
        cwd: Option<&str>,
        env: Option<IndexMap<String, String>>,
        tty: bool,
        mut output: Option<&mut PhpMixed>,
    ) -> Result<Option<i64>> {
        // On Windows, we don't rely on the OS to find the executable if possible to avoid lookups
        // in the current directory which could be untrusted. Instead we use the ExecutableFinder.

        let mut process: Process;
        if is_string(&command) {
            let mut command_str = command.as_string().unwrap_or("").to_string();
            if Platform::is_windows() {
                let mut m: IndexMap<CaptureKey, String> = IndexMap::new();
                if Preg::is_match_strict_groups3(r"{^([^:/\\]++) }", &command_str, Some(&mut m))
                    .unwrap_or(false)
                {
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
                None,
                Some(Self::get_timeout() as f64),
            );
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
                None,
                Some(Self::get_timeout() as f64),
            );
        } else {
            return Err(LogicException {
                message: "Invalid command type".to_string(),
                code: 0,
            }
            .into());
        }

        if !Platform::is_windows() && tty {
            // PHP: try { $process->setTty(true); } catch (RuntimeException $e) { /* ignore */ }
            if let Err(e) = process.set_tty(true) {
                if e.downcast_ref::<SymfonyProcessRuntimeException>().is_none() {
                    return Err(e);
                }
                // ignore TTY enabling errors
            }
        }

        // PHP: $callback = is_callable($output) ? $output : fn($type, $buffer) => $this->outputHandler($type, $buffer);
        let output_is_callable = output.as_deref().map(|o| is_callable(o)).unwrap_or(false);
        let _callback: Box<dyn Fn(&str, &str)> = if output_is_callable {
            // TODO(phase-c): the user-supplied $output is a PhpMixed callable that cannot be
            // invoked without a typed callable model (Rc<dyn Fn>); deferred with the callable model.
            Box::new(|_t: &str, _b: &str| {})
        } else {
            // TODO(phase-c): the fallback must call self.output_handler(type, buffer) (which is
            // &mut self and writes to io / updates last_message), but this is a 'static
            // `Box<dyn Fn>` that cannot borrow &mut self. Wiring it needs the handler state shared
            // (Rc<RefCell<...>>). The callback is also not yet passed to process.run, whose Symfony
            // Process backing stays todo!().
            Box::new(|_t: &str, _b: &str| {})
        };

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

        let result: Result<()> = (|| -> Result<()> {
            let _ = process.run(/* callback */ Some(Box::new(|_t: &str, _b: &str| {})));

            let output_is_callable_inner =
                output.as_deref().map(|o| is_callable(o)).unwrap_or(false);
            if self.capture_output && !output_is_callable_inner {
                if let Some(out) = output.as_mut() {
                    **out = PhpMixed::String(process.get_output());
                }
            }

            self.error_output = process.get_error_output();
            Ok(())
        })();
        let final_result: Result<()> = match result {
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

    /// @param  string|non-empty-list<string> $command
    /// @param  mixed   $output
    fn do_execute(
        &mut self,
        command: PhpMixed,
        cwd: Option<&str>,
        tty: bool,
        mut output: Option<&mut PhpMixed>,
    ) -> Result<i64> {
        self.output_command_run(&command, cwd, false);

        self.capture_output = output.is_some();
        self.error_output = String::new();

        let mut env: Option<IndexMap<String, String>> = None;

        let requires_git_dir_env = self.requires_git_dir_env(&command);
        if cwd.is_some() && requires_git_dir_env {
            let is_bare_repository = !is_dir(&format!("{}/.git", rtrim(cwd.unwrap(), Some("/"))));
            if is_bare_repository {
                let mut config_value = PhpMixed::String(String::new());
                let mut git_env: IndexMap<String, String> = IndexMap::new();
                git_env.insert("GIT_DIR".to_string(), cwd.unwrap().to_string());
                self.run_process(
                    PhpMixed::List(vec![
                        Box::new(PhpMixed::String("git".to_string())),
                        Box::new(PhpMixed::String("config".to_string())),
                        Box::new(PhpMixed::String("safe.bareRepository".to_string())),
                    ]),
                    cwd,
                    Some(git_env.clone()),
                    tty,
                    Some(&mut config_value),
                )?;
                let trimmed = trim(config_value.as_string().unwrap_or(""), None);
                if trimmed == "explicit" {
                    env = Some(git_env);
                }
            }
        }

        Ok(self
            .run_process(command, cwd, env, tty, output.as_deref_mut())?
            .unwrap_or(0))
    }

    /// starts a process on the commandline in async mode
    pub async fn execute_async<C, W>(&mut self, command: C, cwd: W) -> Result<Process>
    where
        C: IntoExecCommand,
        W: IntoExecCwd,
    {
        let command = command.into_exec_command();
        let cwd_opt = cwd.into_exec_cwd();
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
            cwd: cwd_opt,
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

    fn output_handler(&mut self, r#type: &str, buffer: &str) {
        if self.capture_output {
            return;
        }

        if self.io.is_none() {
            print!("{}", buffer);

            return;
        }

        if Process::ERR == r#type {
            self.io
                .as_mut()
                .unwrap()
                .write_error_raw3(buffer, false, io_interface::NORMAL);
        } else {
            self.io
                .as_mut()
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

        let process_result: Result<Process> = (|| -> Result<Process> {
            if is_string(&command) {
                Ok(Process::from_shell_commandline(
                    command.as_string().unwrap_or(""),
                    cwd.as_deref(),
                    None,
                    None,
                    Some(Self::get_timeout() as f64),
                ))
            } else if let PhpMixed::List(ref list) = command {
                Ok(Process::new(
                    list.iter()
                        .map(|v| v.as_string().unwrap_or("").to_string())
                        .collect(),
                    cwd.clone(),
                    None,
                    None,
                    Some(Self::get_timeout() as f64),
                ))
            } else {
                Err(LogicException {
                    message: "Invalid command type".to_string(),
                    code: 0,
                }
                .into())
            }
        })();
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
        if let Some(job) = self.jobs.get_mut(&id) {
            if let Some(p) = job.process.as_mut() {
                p.start(None);
            }
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
            self.max_jobs = max(
                1,
                min(
                    50,
                    max_jobs_env.as_deref().unwrap_or("0").parse().unwrap_or(0),
                ),
            );
        } else {
            self.max_jobs = 10;
        }
    }

    /// @param  ?int $index job id
    pub fn wait(&mut self) -> Result<()> {
        self.wait_id(None)
    }

    pub fn wait_id(&mut self, index: Option<i64>) -> Result<()> {
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
    pub fn count_active_jobs(&mut self, index: Option<i64>) -> Result<i64> {
        // tick
        let ids: Vec<i64> = self.jobs.keys().copied().collect();
        for id in &ids {
            let (status, has_process) = {
                let j = self.jobs.get(id).unwrap();
                (j.status, j.process.is_some())
            };
            if status == Self::STATUS_STARTED {
                if has_process {
                    let is_running = self
                        .jobs
                        .get(id)
                        .and_then(|j| j.process.as_ref())
                        .map(|p| p.is_running())
                        .unwrap_or(false);
                    if !is_running {
                        // PHP: call_user_func($job['resolve'], $job['process']) — the .then handler
                        // marks the job completed/failed based on the process exit status.
                        let successful = self
                            .jobs
                            .get(id)
                            .and_then(|j| j.process.as_ref())
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

                    if let Some(p) = self.jobs.get(id).and_then(|j| j.process.as_ref()) {
                        p.check_timeout()?;
                    }
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
            Preg::split(r"{\r?\n}", &output).unwrap_or_default()
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
                &list.iter().map(|b| (**b).clone()).collect::<Vec<_>>(),
            );
            implode(" ", &parts)
        } else {
            String::new()
        };
        let safe_command = Preg::replace_callback(
            r"{://(?P<user>[^:/\s]+):(?P<password>[^@\s/]+)@}i",
            |m: &IndexMap<CaptureKey, String>| -> String {
                let user_key = CaptureKey::ByName("user".to_string());
                // if the username looks like a long (12char+) hex string, or a modern github token (e.g. ghp_xxx, github_pat_xxx) we obfuscate that
                if Preg::is_match(
                    GitHub::GITHUB_TOKEN_REGEX,
                    m.get(&user_key).cloned().unwrap_or_default().as_str(),
                )
                .unwrap_or(false)
                {
                    return "://***:***@".to_string();
                }
                if Preg::is_match(
                    r"{^[a-f0-9]{12,}$}",
                    m.get(&user_key).cloned().unwrap_or_default().as_str(),
                )
                .unwrap_or(false)
                {
                    return "://***:***@".to_string();
                }

                format!("://{}:***@", m.get(&user_key).cloned().unwrap_or_default())
            },
            &command_string,
        )
        .unwrap_or_default();
        let safe_command = Preg::replace(
            r"{--password (.*[^\\]') }",
            "--password '***' ",
            &safe_command,
        )
        .unwrap_or_default();
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
        if "" == argument {
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
        argument = Preg::replace5(r#"/(\\*)"/"#, r#"$1$1\""#, &argument, -1, &mut dquotes)
            .unwrap_or_default();
        let meta = dquotes > 0 || Preg::is_match(r"/%[^%]+%|![^!]+!/", &argument).unwrap_or(false);

        if !meta && !quote {
            quote = strpbrk(&argument, "^&|<>()").is_some();
        }

        if quote {
            argument = format!(
                "\"{}\"",
                Preg::replace(r"/(\\*)$/", "$1$1", &argument).unwrap_or_default()
            );
        }

        if meta {
            argument = Preg::replace(r#"/(["^&|<>()%])/"#, "^$1", &argument).unwrap_or_default();
            argument = Preg::replace(r"/(!)/", "^^$1", &argument).unwrap_or_default();
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
        if cmd.get(0).map(|s| s.as_str()) != Some("git") {
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
                    .map(|s| Box::new(PhpMixed::String(s.to_string())))
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
        PhpMixed::List(
            self.into_iter()
                .map(|s| Box::new(PhpMixed::String(s)))
                .collect(),
        )
    }
}

impl IntoExecCommand for &Vec<String> {
    fn into_exec_command(self) -> PhpMixed {
        PhpMixed::List(
            self.iter()
                .map(|s| Box::new(PhpMixed::String(s.clone())))
                .collect(),
        )
    }
}

impl<const N: usize> IntoExecCommand for &[&str; N] {
    fn into_exec_command(self) -> PhpMixed {
        PhpMixed::List(
            self.iter()
                .map(|s| Box::new(PhpMixed::String(s.to_string())))
                .collect(),
        )
    }
}

impl IntoExecCommand for &[&str] {
    fn into_exec_command(self) -> PhpMixed {
        PhpMixed::List(
            self.iter()
                .map(|s| Box::new(PhpMixed::String(s.to_string())))
                .collect(),
        )
    }
}

impl IntoExecCommand for &[String] {
    fn into_exec_command(self) -> PhpMixed {
        PhpMixed::List(
            self.iter()
                .map(|s| Box::new(PhpMixed::String(s.clone())))
                .collect(),
        )
    }
}

/// Phase B helper trait: write captured output back to the caller's buffer.
pub trait IntoExecOutput<'a> {
    type Sink: ExecOutputSink + 'a;
    fn into_exec_output(self) -> Self::Sink;
}

pub trait ExecOutputSink {
    fn has_output(&self) -> bool;
    fn write_back(&mut self, value: PhpMixed);
}

pub struct NoOutput;
impl ExecOutputSink for NoOutput {
    fn has_output(&self) -> bool {
        false
    }
    fn write_back(&mut self, _value: PhpMixed) {}
}

pub struct PhpMixedOutput<'a>(Option<&'a mut PhpMixed>);
impl<'a> ExecOutputSink for PhpMixedOutput<'a> {
    fn has_output(&self) -> bool {
        self.0.is_some()
    }
    fn write_back(&mut self, value: PhpMixed) {
        if let Some(out) = self.0.as_deref_mut() {
            *out = value;
        }
    }
}

pub struct StringOutput<'a>(&'a mut String);
impl<'a> ExecOutputSink for StringOutput<'a> {
    fn has_output(&self) -> bool {
        true
    }
    fn write_back(&mut self, value: PhpMixed) {
        *self.0 = value.as_string().unwrap_or("").to_string();
    }
}

impl<'a> IntoExecOutput<'a> for () {
    type Sink = NoOutput;
    fn into_exec_output(self) -> NoOutput {
        NoOutput
    }
}

impl<'a> IntoExecOutput<'a> for Option<&'a mut PhpMixed> {
    type Sink = PhpMixedOutput<'a>;
    fn into_exec_output(self) -> PhpMixedOutput<'a> {
        PhpMixedOutput(self)
    }
}

impl<'a> IntoExecOutput<'a> for &'a mut PhpMixed {
    type Sink = PhpMixedOutput<'a>;
    fn into_exec_output(self) -> PhpMixedOutput<'a> {
        PhpMixedOutput(Some(self))
    }
}

impl<'a> IntoExecOutput<'a> for &'a mut String {
    type Sink = StringOutput<'a>;
    fn into_exec_output(self) -> StringOutput<'a> {
        StringOutput(self)
    }
}

/// Phase B helper trait: convert various cwd argument forms into `Option<String>`.
pub trait IntoExecCwd {
    fn into_exec_cwd(self) -> Option<String>;
}

impl IntoExecCwd for () {
    fn into_exec_cwd(self) -> Option<String> {
        None
    }
}

impl IntoExecCwd for Option<&str> {
    fn into_exec_cwd(self) -> Option<String> {
        self.map(|s| s.to_string())
    }
}

impl IntoExecCwd for Option<String> {
    fn into_exec_cwd(self) -> Option<String> {
        self
    }
}

impl IntoExecCwd for Option<&String> {
    fn into_exec_cwd(self) -> Option<String> {
        self.cloned()
    }
}

impl IntoExecCwd for &str {
    fn into_exec_cwd(self) -> Option<String> {
        Some(self.to_string())
    }
}

impl IntoExecCwd for String {
    fn into_exec_cwd(self) -> Option<String> {
        Some(self)
    }
}

impl IntoExecCwd for &String {
    fn into_exec_cwd(self) -> Option<String> {
        Some(self.clone())
    }
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
