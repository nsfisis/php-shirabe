//! ref: composer/src/Composer/Util/ProcessExecutor.php

use crate::io::io_interface;
use anyhow::Result;
use indexmap::IndexMap;
use std::sync::{LazyLock, Mutex};

use shirabe_external_packages::composer::pcre::preg::Preg;
use shirabe_external_packages::react::promise::promise::Promise;
use shirabe_external_packages::react::promise::promise_interface::PromiseInterface;
use shirabe_external_packages::seld::signal::signal_handler::SignalHandler;
use shirabe_external_packages::symfony::component::process::exception::process_signaled_exception::ProcessSignaledException;
use shirabe_external_packages::symfony::component::process::exception::runtime_exception::RuntimeException as SymfonyProcessRuntimeException;
use shirabe_external_packages::symfony::component::process::executable_finder::ExecutableFinder;
use shirabe_external_packages::symfony::component::process::process::Process;
use shirabe_php_shim::{
    LogicException, PhpMixed, RuntimeException, array_intersect, array_map, call_user_func,
    defined, escapeshellarg, explode, implode, in_array, is_array, is_callable, is_dir, is_numeric,
    is_string, max, min, rtrim, sprintf, str_replace, strcspn, strlen, strpbrk, strtolower, strtr,
    substr_replace, trim, usleep,
};

use crate::io::io_interface::IOInterface;
use crate::util::github::GitHub;
use crate::util::platform::Platform;

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
    pub(crate) io: Option<Box<dyn IOInterface>>,
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

#[derive(Debug)]
struct Job {
    id: i64,
    status: i64,
    command: PhpMixed,
    cwd: Option<String>,
    process: Option<Process>,
    resolve: Option<Box<dyn Fn(PhpMixed) + Send + Sync>>,
    reject: Option<Box<dyn Fn(PhpMixed) + Send + Sync>>,
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

    pub fn new(io: Option<Box<dyn IOInterface>>, _: Option<()>) -> Self {
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
    pub fn execute(
        &mut self,
        command: PhpMixed,
        output: Option<&mut PhpMixed>,
        cwd: Option<&str>,
    ) -> Result<i64> {
        // PHP: func_num_args() > 1
        let has_output_arg = output.is_some();
        if has_output_arg {
            return self.do_execute(command, cwd, false, output);
        }

        self.do_execute(command, cwd, false, None)
    }

    /// runs a process on the commandline in TTY mode
    pub fn execute_tty(&mut self, command: PhpMixed, cwd: Option<&str>) -> Result<i64> {
        if Platform::is_tty() {
            return self.do_execute(command, cwd, true, None);
        }

        self.do_execute(command, cwd, false, None)
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
                if let Some(m) = Preg::is_match_strict_groups(r"{^([^:/\\]++) }", &command_str) {
                    command_str = substr_replace(
                        &command_str,
                        &Self::escape(PhpMixed::String(Self::get_executable(
                            m.get(1).cloned().unwrap_or_default().as_str(),
                        ))),
                        0,
                        strlen(m.get(1).cloned().unwrap_or_default().as_str()) as usize,
                    );
                }
            }

            process = Process::from_shell_commandline(
                &command_str,
                cwd,
                env.clone(),
                None,
                Self::get_timeout(),
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

            process = Process::new(cmd_vec, cwd, env, None, Self::get_timeout());
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

        let _callback: Box<dyn Fn(&str, &str)> = if is_callable(output.as_deref().cloned()) {
            // TODO(phase-b): adapt output PhpMixed callable to closure
            Box::new(|_t: &str, _b: &str| {})
        } else {
            Box::new(|_t: &str, _b: &str| {
                // TODO(phase-b): self.output_handler(t, b) — self is borrowed mutably elsewhere
            })
        };

        let io_for_signal = self.io.as_ref().map(|b| &**b as *const dyn IOInterface);
        let signal_handler = SignalHandler::create(
            vec![
                SignalHandler::SIGINT,
                SignalHandler::SIGTERM,
                SignalHandler::SIGHUP,
            ],
            Box::new(move |signal: String, _h: &SignalHandler| {
                if let Some(io_ptr) = io_for_signal {
                    let io = unsafe { &*io_ptr };
                    io.write_error(&format!(
                        "Received {}, aborting when child process is done",
                        signal
                    ));
                }
            }),
        );

        let result: Result<()> = (|| -> Result<()> {
            process.run(/* callback */ Box::new(|_t: &str, _b: &str| {}))?;

            if self.capture_output && !is_callable(output.as_deref().cloned()) {
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
                // TODO(phase-b): catch ProcessSignaledException
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
    pub fn execute_async(
        &mut self,
        command: PhpMixed,
        cwd: Option<&str>,
    ) -> Result<Box<dyn PromiseInterface>> {
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
            cwd: cwd.map(String::from),
            process: None,
            resolve: None,
            reject: None,
        };

        // TODO(phase-b): build resolver/canceler closures bound to &mut self.jobs
        let resolver: Box<dyn Fn(_, _)> = Box::new(|_resolve, _reject| {});
        let canceler: Box<dyn Fn()> = Box::new(|| {
            if defined("SIGINT") {
                // job.process.signal(SIGINT)
            }
            // job.process.stop(1)
        });
        let _ = (resolver, canceler);

        let promise = Promise::new(Box::new(|_resolve, _reject| {}), Box::new(|| {}));
        // TODO(phase-b): wire promise.then() side-effects: mark job done & update status
        let promise: Box<dyn PromiseInterface> = Box::new(promise);

        self.jobs.insert(id, job);

        if self.running_jobs < self.max_jobs {
            self.start_job(id);
        }

        Ok(promise)
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
            self.io.as_ref().unwrap().write_error_raw(
                PhpMixed::String(buffer.to_string()),
                false,
                io_interface::NORMAL,
            );
        } else {
            self.io.as_ref().unwrap().write_raw(
                PhpMixed::String(buffer.to_string()),
                false,
                io_interface::NORMAL,
            );
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
                    Self::get_timeout(),
                ))
            } else if let PhpMixed::List(ref list) = command {
                Ok(Process::new(
                    list.iter()
                        .map(|v| v.as_string().unwrap_or("").to_string())
                        .collect(),
                    cwd.as_deref(),
                    None,
                    None,
                    Self::get_timeout(),
                ))
            } else {
                Err(LogicException {
                    message: "Invalid command type".to_string(),
                    code: 0,
                }
                .into())
            }
        })();
        let mut process = match process_result {
            Ok(p) => p,
            Err(_e) => {
                // job.reject(e) — TODO(phase-b)
                return;
            }
        };

        if let Some(job) = self.jobs.get_mut(&id) {
            job.process = Some(process.clone());
        }

        if let Err(_e) = process.start() {
            // job.reject(e) — TODO(phase-b)
            return;
        }
    }

    pub fn set_max_jobs(&mut self, max_jobs: i64) {
        self.max_jobs = max_jobs;
    }

    pub fn reset_max_jobs(&mut self) {
        let max_jobs_env = Platform::get_env("COMPOSER_MAX_PARALLEL_PROCESSES");
        if is_numeric(&max_jobs_env) {
            self.max_jobs = max(
                1,
                min(
                    50,
                    max_jobs_env.as_string().unwrap_or("0").parse().unwrap_or(0),
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
            if 0 == self.count_active_jobs(index) {
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
    pub fn count_active_jobs(&mut self, index: Option<i64>) -> i64 {
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
                        if let Some(job) = self.jobs.get(id) {
                            if let Some(resolve) = job.resolve.as_ref() {
                                let process_mixed = PhpMixed::Null; // TODO(phase-b): wrap Process as PhpMixed
                                resolve(process_mixed);
                            }
                        }
                    }

                    if let Some(job) = self.jobs.get_mut(id) {
                        if let Some(p) = job.process.as_mut() {
                            p.check_timeout();
                        }
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
            return if self.jobs.get(&index).map(|j| j.status).unwrap_or(0) < Self::STATUS_COMPLETED
            {
                1
            } else {
                0
            };
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

        active
    }

    fn mark_job_done(&mut self) {
        self.running_jobs -= 1;
    }

    /// @return string[]
    pub fn split_lines(&self, output: Option<&str>) -> Vec<String> {
        let output = trim(output.unwrap_or(""), None);

        if output.is_empty() {
            vec![]
        } else {
            Preg::split(r"{\r?\n}", &output)
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
    pub fn set_timeout(timeout: i64) {
        *TIMEOUT.lock().unwrap() = timeout;
    }

    /// Escapes a string to be used as a shell argument.
    pub fn escape(argument: PhpMixed) -> String {
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
                |v| Self::escape(v.clone()),
                &list.iter().map(|b| (**b).clone()).collect::<Vec<_>>(),
            );
            implode(" ", &parts)
        } else {
            String::new()
        };
        let safe_command = Preg::replace_callback(
            r"{://(?P<user>[^:/\s]+):(?P<password>[^@\s/]+)@}i",
            |m: &IndexMap<String, String>| -> String {
                // if the username looks like a long (12char+) hex string, or a modern github token (e.g. ghp_xxx, github_pat_xxx) we obfuscate that
                if Preg::is_match(
                    GitHub::GITHUB_TOKEN_REGEX,
                    m.get("user").cloned().unwrap_or_default().as_str(),
                ) {
                    return "://***:***@".to_string();
                }
                if Preg::is_match(
                    r"{^[a-f0-9]{12,}$}",
                    m.get("user").cloned().unwrap_or_default().as_str(),
                ) {
                    return "://***:***@".to_string();
                }

                format!("://{}:***@", m.get("user").cloned().unwrap_or_default())
            },
            &command_string,
        );
        let safe_command = Preg::replace(
            r"{--password (.*[^\\]') }",
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
    fn escape_argument(argument: PhpMixed) -> String {
        let mut argument = argument.as_string().unwrap_or("").to_string();
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
        // PHP: strtr($argument, $translation) — variadic translation map
        // TODO(phase-b): implement multi-target strtr; for now we apply replacements iteratively
        for (from, to) in &translation {
            argument = str_replace(from, to, &argument);
        }
        let _ = strtr;

        // In addition to whitespace, commas need quoting to preserve paths
        let mut quote = strpbrk(&argument, " \t,").is_some();
        let mut dquotes: i64 = 0;
        // PHP: Preg::replace('/(\\\\*)"/', '$1$1\\"', $argument, -1, $dquotes)
        argument =
            Preg::replace_with_count(r#"/(\\*)"/"#, r#"$1$1\""#, &argument, -1, &mut dquotes);
        let meta = dquotes > 0 || Preg::is_match(r"/%[^%]+%|![^!]+!/", &argument);

        if !meta && !quote {
            quote = strpbrk(&argument, "^&|<>()").is_some();
        }

        if quote {
            argument = format!("\"{}\"", Preg::replace(r"/(\\*)$/", "$1$1", &argument));
        }

        if meta {
            argument = Preg::replace(r#"/(["^&|<>()%])/"#, "^$1", &argument);
            argument = Preg::replace(r"/(!)/", "^^$1", &argument);
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
            let path = ExecutableFinder::new().find(name, Some(name));
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

impl Clone for ProcessExecutor {
    fn clone(&self) -> Self {
        // TODO(phase-b): cloning ProcessExecutor is incidental to Phase A — share state
        // properly in a Phase B refactor
        Self {
            capture_output: self.capture_output,
            error_output: self.error_output.clone(),
            io: None,
            jobs: IndexMap::new(),
            running_jobs: 0,
            max_jobs: self.max_jobs,
            id_gen: 0,
            allow_async: self.allow_async,
        }
    }
}

// Suppress unused-import warnings.
#[allow(dead_code)]
const _USE_PARITY: () = {
    let _ = call_user_func;
    let _ = sprintf;
};
