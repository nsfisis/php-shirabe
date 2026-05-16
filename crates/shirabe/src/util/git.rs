//! ref: composer/src/Composer/Util/Git.php

use anyhow::Result;
use indexmap::IndexMap;
use std::sync::Mutex;

use shirabe_external_packages::composer::pcre::preg::Preg;
use shirabe_php_shim::{
    InvalidArgumentException, PHP_EOL, PhpMixed, RuntimeException, array_map,
    array_merge_recursive, clearstatcache, count, explode, implode, in_array, is_array,
    is_callable, is_dir, preg_quote, rawurldecode, rawurlencode, str_contains, str_ends_with,
    str_replace, str_replace_array, strlen, strpos, substr, trim, version_compare,
};

use crate::config::Config;
use crate::io::io_interface::IOInterface;
use crate::util::auth_helper::AuthHelper;
use crate::util::bitbucket::Bitbucket;
use crate::util::filesystem::Filesystem;
use crate::util::github::GitHub;
use crate::util::gitlab::GitLab;
use crate::util::http_downloader::HttpDownloader;
use crate::util::platform::Platform;
use crate::util::process_executor::ProcessExecutor;
use crate::util::url::Url;

#[derive(Debug)]
pub struct Git {
    pub(crate) io: Box<dyn IOInterface>,
    pub(crate) config: Config,
    pub(crate) process: ProcessExecutor,
    pub(crate) filesystem: Filesystem,
    pub(crate) http_downloader: Option<HttpDownloader>,
}

/// @var string|false|null
static VERSION: Mutex<Option<Option<String>>> = Mutex::new(None);

impl Git {
    pub fn new(
        io: Box<dyn IOInterface>,
        config: Config,
        process: ProcessExecutor,
        fs: Filesystem,
    ) -> Self {
        Self {
            io,
            config,
            process,
            filesystem: fs,
            http_downloader: None,
        }
    }

    /// @param IOInterface|null $io If present, a warning is output there instead of throwing, so pass this in only for cases where this is a soft failure
    pub fn check_for_repo_ownership_error(
        output: &str,
        path: &str,
        io: Option<&dyn IOInterface>,
    ) -> Result<()> {
        if str_contains(output, "fatal: detected dubious ownership") {
            let msg = format!(
                "The repository at \"{}\" does not have the correct ownership and git refuses to use it:{}{}{}",
                path, PHP_EOL, PHP_EOL, output
            );
            match io {
                None => {
                    return Err(RuntimeException {
                        message: msg,
                        code: 0,
                    }
                    .into());
                }
                Some(io) => {
                    io.write_error(
                        PhpMixed::String(format!("<warning>{}</warning>", msg)),
                        true,
                        IOInterface::NORMAL,
                    );
                }
            }
        }
        Ok(())
    }

    pub fn set_http_downloader(&mut self, http_downloader: HttpDownloader) {
        self.http_downloader = Some(http_downloader);
    }

    /// Runs a set of commands using the $url or a variation of it (with auth, ssh, ..)
    ///
    /// Commands should use %url% placeholders for the URL instead of inlining it to allow this function to do its job
    /// %sanitizedUrl% is also automatically replaced by the url without user/pass
    ///
    /// As soon as a single command fails it will halt, so assume the commands are run as && in bash
    ///
    /// @param non-empty-array<non-empty-list<string>> $commands
    /// @param mixed $commandOutput  the output will be written into this var if passed by ref
    ///                              if a callable is passed it will be used as output handler
    pub fn run_commands(
        &mut self,
        commands: Vec<Vec<String>>,
        url: &str,
        cwd: Option<&str>,
        initial_clone: bool,
        command_output: Option<&mut PhpMixed>,
    ) -> Result<()> {
        let mut callables: Vec<Box<dyn Fn(&str) -> Vec<String>>> = vec![];
        for cmd in commands {
            let cmd_clone = cmd.clone();
            callables.push(Box::new(move |url: &str| -> Vec<String> {
                let mut map: IndexMap<String, String> = IndexMap::new();
                map.insert("%url%".to_string(), url.to_string());
                map.insert(
                    "%sanitizedUrl%".to_string(),
                    Preg::replace(r"{://([^@]+?):(.+?)@}", "://", url.to_string()),
                );

                array_map(
                    |value: &String| map.get(value).cloned().unwrap_or_else(|| value.clone()),
                    &cmd_clone,
                )
            }));
        }

        // @phpstan-ignore method.deprecated
        self.run_command(callables, url, cwd, initial_clone, command_output)
    }

    /// @param callable|array<callable> $commandCallable
    /// @param mixed       $commandOutput  the output will be written into this var if passed by ref
    ///                                    if a callable is passed it will be used as output handler
    /// @deprecated Use runCommands with placeholders instead of callbacks for simplicity
    pub fn run_command(
        &mut self,
        command_callable: Vec<Box<dyn Fn(&str) -> Vec<String>>>,
        url: &str,
        cwd: Option<&str>,
        initial_clone: bool,
        mut command_output: Option<&mut PhpMixed>,
    ) -> Result<()> {
        let command_callables = command_callable;
        let mut last_command: PhpMixed = PhpMixed::String(String::new());

        // Ensure we are allowed to use this URL by config
        self.config
            .prohibit_url_by_config(url, Some(self.io.as_ref()), &IndexMap::new())?;

        let orig_cwd: Option<String> = if initial_clone {
            cwd.map(|s| s.to_string())
        } else {
            None
        };

        // TODO(phase-b): closure captures &mut self.process, &mut last_command, etc.
        // Inlined as a helper that returns (status, last_command, output)
        let cwd_string = cwd.map(|s| s.to_string());

        // PHP closure: $runCommands = function ($url) use (...) { ... };
        let mut run_commands_inline = |url_arg: &str,
                                       this_process: &mut ProcessExecutor,
                                       last_cmd: &mut PhpMixed,
                                       command_output: Option<&mut PhpMixed>|
         -> i64 {
            let collect_outputs = !command_output
                .as_ref()
                .map(|v| is_callable(v))
                .unwrap_or(false);
            let mut outputs: Vec<String> = vec![];

            let mut status: i64 = 0;
            let mut counter: i64 = 0;
            for callable in &command_callables {
                let cmd = callable(url_arg);
                *last_cmd = PhpMixed::List(
                    cmd.iter()
                        .map(|s| Box::new(PhpMixed::String(s.clone())))
                        .collect(),
                );
                let mut local_output = String::new();
                let exec_cwd = if initial_clone && counter == 0 {
                    None
                } else {
                    cwd_string.clone()
                };
                status = this_process.execute(&cmd, &mut local_output, exec_cwd);
                if collect_outputs {
                    outputs.push(local_output);
                }
                if status != 0 {
                    break;
                }
                counter += 1;
            }

            if collect_outputs {
                if let Some(out) = command_output {
                    *out = PhpMixed::String(implode("", &outputs));
                }
            }

            status
        };

        if Preg::is_match(r"{^ssh://[^@]+@[^:]+:[^0-9]+}", url).unwrap_or(false) {
            return Err(InvalidArgumentException {
                message: format!(
                    "The source URL {} is invalid, ssh URLs should have a port number after \":\".\nUse ssh://git@example.com:22/path or just git@example.com:path if you do not want to provide a password or custom port.",
                    url
                ),
                code: 0,
            }
            .into());
        }

        if !initial_clone {
            // capture username/password from URL if there is one and we have no auth configured yet
            let mut output = String::new();
            self.process.execute(
                &vec!["git".to_string(), "remote".to_string(), "-v".to_string()],
                &mut output,
                cwd.map(|s| s.to_string()),
            );
            if let Some(m) = Preg::is_match_strict_groups(
                r"{^(?:composer|origin)\s+https?://(.+):(.+)@([^/]+)}im",
                &output,
            ) {
                let m3 = m.get(3).cloned().unwrap_or_default();
                if !self.io.has_authentication(&m3) {
                    self.io.set_authentication(
                        m3.clone(),
                        rawurldecode(&m.get(1).cloned().unwrap_or_default()),
                        Some(rawurldecode(&m.get(2).cloned().unwrap_or_default())),
                    );
                }
            }
        }

        let protocols = self.config.get("github-protocols");
        // public github, autoswitch protocols
        // @phpstan-ignore composerPcre.maybeUnsafeStrictGroups
        if let Some(m) = Preg::is_match_strict_groups(
            &format!(
                "{{^(?:https?|git)://{}/(.*)}}",
                Self::get_github_domains_regex(&self.config)
            ),
            url,
        ) {
            let mut messages: Vec<String> = vec![];
            let protocols_list: Vec<String> = match &protocols {
                PhpMixed::List(l) => l
                    .iter()
                    .filter_map(|v| v.as_string().map(|s| s.to_string()))
                    .collect(),
                _ => vec![],
            };
            for protocol in &protocols_list {
                let m1 = m.get(1).cloned().unwrap_or_default();
                let m2 = m.get(2).cloned().unwrap_or_default();
                let proto_url = if protocol == "ssh" {
                    format!("git@{}:{}", m1, m2)
                } else {
                    format!("{}://{}/{}", protocol, m1, m2)
                };

                if run_commands_inline(
                    &proto_url,
                    &mut self.process,
                    &mut last_command,
                    command_output.as_deref_mut(),
                ) == 0
                {
                    return Ok(());
                }
                messages.push(format!(
                    "- {}\n{}",
                    proto_url,
                    Preg::replace(r"#^#m", "  ", self.process.get_error_output().to_string())
                ));

                if initial_clone {
                    if let Some(ref orig) = orig_cwd {
                        self.filesystem.remove_directory(orig);
                    }
                }
            }

            // failed to checkout, first check git accessibility
            let m1 = m.get(1).cloned().unwrap_or_default();
            if !self.io.has_authentication(&m1) && !self.io.is_interactive() {
                self.throw_exception(
                    &format!(
                        "Failed to clone {} via {} protocols, aborting.\n\n{}",
                        url,
                        implode(", ", &protocols_list),
                        implode("\n", &messages)
                    ),
                    url,
                )?;
            }
        }

        // if we have a private github url and the ssh protocol is disabled then we skip it and directly fallback to https
        let protocols_list: Vec<String> = match self.config.get("github-protocols") {
            PhpMixed::List(l) => l
                .iter()
                .filter_map(|v| v.as_string().map(|s| s.to_string()))
                .collect(),
            _ => vec![],
        };
        let bypass_ssh_for_github = Preg::is_match(
            &format!(
                "{{^git@{}:(.+?)\\.git$}}i",
                Self::get_github_domains_regex(&self.config)
            ),
            url,
        )
        .unwrap_or(false)
            && !in_array(
                PhpMixed::String("ssh".to_string()),
                &PhpMixed::List(
                    protocols_list
                        .iter()
                        .map(|s| Box::new(PhpMixed::String(s.clone())))
                        .collect(),
                ),
                true,
            );

        let mut auth: Option<IndexMap<String, Option<String>>> = None;
        let mut credentials: Vec<String> = vec![];
        if bypass_ssh_for_github
            || 0 != run_commands_inline(
                url,
                &mut self.process,
                &mut last_command,
                command_output.as_deref_mut(),
            )
        {
            let mut error_msg = self.process.get_error_output().to_string();
            // private github repository without ssh key access, try https with auth
            // @phpstan-ignore composerPcre.maybeUnsafeStrictGroups
            let github_ssh_match = Preg::is_match_strict_groups(
                &format!(
                    "{{^git@{}:(.+?)\\.git$}}i",
                    Self::get_github_domains_regex(&self.config)
                ),
                url,
            );
            let github_https_match = Preg::is_match_strict_groups(
                &format!(
                    "{{^https?://{}/(.*?)(?:\\.git)?$}}i",
                    Self::get_github_domains_regex(&self.config)
                ),
                url,
            );
            if let Some(m) = github_ssh_match.or(github_https_match) {
                let m1 = m.get(1).cloned().unwrap_or_default();
                let m2 = m.get(2).cloned().unwrap_or_default();
                if !self.io.has_authentication(&m1) {
                    let mut git_hub_util = GitHub::new(
                        self.io.as_ref(),
                        &self.config,
                        &self.process,
                        self.http_downloader
                            .as_ref()
                            .unwrap_or(&HttpDownloader::default()),
                    );
                    let message = "Cloning failed using an ssh key for authentication, enter your GitHub credentials to access private repos";

                    if !git_hub_util.authorize_oauth(&m1) && self.io.is_interactive() {
                        git_hub_util.authorize_oauth_interactively(&m1, Some(message));
                    }
                }

                if self.io.has_authentication(&m1) {
                    auth = Some(self.io.get_authentication(&m1));
                    let auth_inner = auth.as_ref().unwrap();
                    let username = auth_inner
                        .get("username")
                        .cloned()
                        .unwrap_or(None)
                        .unwrap_or_default();
                    let password = auth_inner
                        .get("password")
                        .cloned()
                        .unwrap_or(None)
                        .unwrap_or_default();
                    let auth_url = format!(
                        "https://{}:{}@{}/{}.git",
                        rawurlencode(&username),
                        rawurlencode(&password),
                        m1,
                        m2
                    );
                    if run_commands_inline(
                        &auth_url,
                        &mut self.process,
                        &mut last_command,
                        command_output.as_deref_mut(),
                    ) == 0
                    {
                        return Ok(());
                    }

                    credentials = vec![rawurlencode(&username), rawurlencode(&password)];
                    error_msg = self.process.get_error_output().to_string();
                }
            } else if let Some(m) = Preg::is_match_strict_groups(
                r"{^(https?)://(bitbucket\.org)/(.*?)(?:\.git)?$}i",
                url,
            )
            .or_else(|| {
                Preg::is_match_strict_groups(r"{^(git)@(bitbucket\.org):(.+?\.git)$}i", url)
            }) {
                // bitbucket either through oauth or app password, with fallback to ssh.
                let mut bitbucket_util = Bitbucket::new(
                    self.io.as_ref(),
                    &self.config,
                    &self.process,
                    self.http_downloader
                        .as_ref()
                        .unwrap_or(&HttpDownloader::default()),
                );

                let domain = m.get(2).cloned().unwrap_or_default();
                let mut repo_with_git_part = m.get(3).cloned().unwrap_or_default();
                if !str_ends_with(&repo_with_git_part, ".git") {
                    repo_with_git_part.push_str(".git");
                }
                if !self.io.has_authentication(&domain) {
                    let message = "Enter your Bitbucket credentials to access private repos";

                    if !bitbucket_util.authorize_oauth(&domain) && self.io.is_interactive() {
                        bitbucket_util.authorize_oauth_interactively(&domain, Some(message));
                        let access_token = bitbucket_util.get_token();
                        self.io.set_authentication(
                            domain.clone(),
                            "x-token-auth".to_string(),
                            Some(access_token),
                        );
                    }
                }

                // First we try to authenticate with whatever we have stored.
                if self.io.has_authentication(&domain) {
                    auth = Some(self.io.get_authentication(&domain));
                    let mut username = auth
                        .as_ref()
                        .unwrap()
                        .get("username")
                        .cloned()
                        .unwrap_or(None)
                        .unwrap_or_default();
                    let password = auth
                        .as_ref()
                        .unwrap()
                        .get("password")
                        .cloned()
                        .unwrap_or(None)
                        .unwrap_or_default();
                    // Bitbucket API tokens use the email address as the username for HTTP API calls and
                    // either the Bitbucket username or 'x-bitbucket-api-token-auth' as the username for git operations.
                    if strpos(&password, "ATAT") == Some(0) {
                        username = "x-bitbucket-api-token-auth".to_string();
                    }

                    let auth_url = format!(
                        "https://{}:{}@{}/{}",
                        rawurlencode(&username),
                        rawurlencode(&password),
                        domain,
                        repo_with_git_part
                    );

                    if run_commands_inline(
                        &auth_url,
                        &mut self.process,
                        &mut last_command,
                        command_output.as_deref_mut(),
                    ) == 0
                    {
                        return Ok(());
                    }

                    // We already have an access_token from a previous request.
                    if username != "x-token-auth" {
                        let access_token =
                            bitbucket_util.request_token(&domain, &username, &password);
                        if !access_token.is_empty() {
                            self.io.set_authentication(
                                domain.clone(),
                                "x-token-auth".to_string(),
                                Some(access_token),
                            );
                        }
                    }
                }

                if self.io.has_authentication(&domain) {
                    auth = Some(self.io.get_authentication(&domain));
                    let username = auth
                        .as_ref()
                        .unwrap()
                        .get("username")
                        .cloned()
                        .unwrap_or(None)
                        .unwrap_or_default();
                    let password = auth
                        .as_ref()
                        .unwrap()
                        .get("password")
                        .cloned()
                        .unwrap_or(None)
                        .unwrap_or_default();
                    let auth_url = format!(
                        "https://{}:{}@{}/{}",
                        rawurlencode(&username),
                        rawurlencode(&password),
                        domain,
                        repo_with_git_part
                    );
                    if run_commands_inline(
                        &auth_url,
                        &mut self.process,
                        &mut last_command,
                        command_output.as_deref_mut(),
                    ) == 0
                    {
                        return Ok(());
                    }

                    credentials = vec![rawurlencode(&username), rawurlencode(&password)];
                }
                // Falling back to ssh
                let ssh_url = format!("git@bitbucket.org:{}", repo_with_git_part);
                self.io.write_error(
                    PhpMixed::String(
                        "    No bitbucket authentication configured. Falling back to ssh."
                            .to_string(),
                    ),
                    true,
                    IOInterface::NORMAL,
                );
                if run_commands_inline(
                    &ssh_url,
                    &mut self.process,
                    &mut last_command,
                    command_output.as_deref_mut(),
                ) == 0
                {
                    return Ok(());
                }

                error_msg = self.process.get_error_output().to_string();
            } else if let Some(m) = Preg::is_match_strict_groups(
                &format!(
                    "{{^(git)@{}:(.+?\\.git)$}}i",
                    Self::get_gitlab_domains_regex(&self.config)
                ),
                url,
            )
            .or_else(|| {
                Preg::is_match_strict_groups(
                    &format!(
                        "{{^(https?)://{}/(.*)}}i",
                        Self::get_gitlab_domains_regex(&self.config)
                    ),
                    url,
                )
            }) {
                let mut m1 = m.get(1).cloned().unwrap_or_default();
                let m2 = m.get(2).cloned().unwrap_or_default();
                let m3 = m.get(3).cloned().unwrap_or_default();
                if m1 == "git" {
                    m1 = "https".to_string();
                }

                if !self.io.has_authentication(&m2) {
                    let mut git_lab_util = GitLab::new(
                        self.io.as_ref(),
                        &self.config,
                        &self.process,
                        self.http_downloader
                            .as_ref()
                            .unwrap_or(&HttpDownloader::default()),
                    );
                    let message =
                        "Cloning failed, enter your GitLab credentials to access private repos";

                    if !git_lab_util.authorize_oauth(&m2) && self.io.is_interactive() {
                        git_lab_util.authorize_oauth_interactively(&m1, &m2, Some(message));
                    }
                }

                if self.io.has_authentication(&m2) {
                    auth = Some(self.io.get_authentication(&m2));
                    let username = auth
                        .as_ref()
                        .unwrap()
                        .get("username")
                        .cloned()
                        .unwrap_or(None)
                        .unwrap_or_default();
                    let password = auth
                        .as_ref()
                        .unwrap()
                        .get("password")
                        .cloned()
                        .unwrap_or(None)
                        .unwrap_or_default();
                    let auth_url = if password == "private-token"
                        || password == "oauth2"
                        || password == "gitlab-ci-token"
                    {
                        format!(
                            "{}://{}:{}@{}/{}",
                            m1,
                            rawurlencode(&password),
                            rawurlencode(&username),
                            m2,
                            m3
                        ) // swap username and password
                    } else {
                        format!(
                            "{}://{}:{}@{}/{}",
                            m1,
                            rawurlencode(&username),
                            rawurlencode(&password),
                            m2,
                            m3
                        )
                    };

                    if run_commands_inline(
                        &auth_url,
                        &mut self.process,
                        &mut last_command,
                        command_output.as_deref_mut(),
                    ) == 0
                    {
                        return Ok(());
                    }

                    credentials = vec![rawurlencode(&username), rawurlencode(&password)];
                    error_msg = self.process.get_error_output().to_string();
                }
            } else if let Some(m) = self.get_authentication_failure(url) {
                // private non-github/gitlab/bitbucket repo that failed to authenticate
                let mut m1 = m.get(1).cloned().unwrap_or_default();
                let mut m2 = m.get(2).cloned().unwrap_or_default();
                let m3 = m.get(3).cloned().unwrap_or_default();
                let mut auth_parts: Option<String> = None;
                if str_contains(&m2, "@") {
                    let parts = explode("@", &m2);
                    auth_parts = parts.get(0).cloned();
                    m2 = parts.get(1).cloned().unwrap_or_default();
                }

                let mut store_auth: PhpMixed = PhpMixed::Bool(false);
                if self.io.has_authentication(&m2) {
                    auth = Some(self.io.get_authentication(&m2));
                } else if self.io.is_interactive() {
                    let mut default_username: Option<String> = None;
                    if let Some(ref parts) = auth_parts {
                        if !parts.is_empty() {
                            if str_contains(parts, ":") {
                                let split = explode(":", parts);
                                default_username = split.get(0).cloned();
                            } else {
                                default_username = Some(parts.clone());
                            }
                        }
                    }

                    self.io.write_error(
                        PhpMixed::String(format!(
                            "    Authentication required (<info>{}</info>):",
                            m2
                        )),
                        true,
                        IOInterface::NORMAL,
                    );
                    self.io.write_error(
                        PhpMixed::String(format!("<warning>{}</warning>", trim(&error_msg, None))),
                        true,
                        IOInterface::VERBOSE,
                    );
                    let mut auth_map: IndexMap<String, Option<String>> = IndexMap::new();
                    auth_map.insert(
                        "username".to_string(),
                        self.io
                            .ask(
                                "      Username: ".to_string(),
                                default_username
                                    .clone()
                                    .map(PhpMixed::String)
                                    .unwrap_or(PhpMixed::Null),
                            )
                            .as_string()
                            .map(|s| s.to_string()),
                    );
                    auth_map.insert(
                        "password".to_string(),
                        self.io.ask_and_hide_answer("      Password: ".to_string()),
                    );
                    auth = Some(auth_map);
                    store_auth = self.config.get("store-auths");
                }

                if let Some(auth_inner) = auth.as_ref() {
                    let username = auth_inner
                        .get("username")
                        .cloned()
                        .unwrap_or(None)
                        .unwrap_or_default();
                    let password = auth_inner
                        .get("password")
                        .cloned()
                        .unwrap_or(None)
                        .unwrap_or_default();
                    let auth_url = format!(
                        "{}{}:{}@{}{}",
                        m1,
                        rawurlencode(&username),
                        rawurlencode(&password),
                        m2,
                        m3
                    );

                    if run_commands_inline(
                        &auth_url,
                        &mut self.process,
                        &mut last_command,
                        command_output.as_deref_mut(),
                    ) == 0
                    {
                        self.io
                            .set_authentication(m2.clone(), username, Some(password));
                        let mut auth_helper = AuthHelper::new(self.io.as_ref(), &self.config);
                        auth_helper.store_auth(&m2, &store_auth);

                        return Ok(());
                    }

                    credentials = vec![rawurlencode(&username), rawurlencode(&password)];
                    error_msg = self.process.get_error_output().to_string();
                }
            }

            if initial_clone {
                if let Some(ref orig) = orig_cwd {
                    self.filesystem.remove_directory(orig);
                }
            }

            let mut last_command_str = match &last_command {
                PhpMixed::List(l) => {
                    let parts: Vec<String> = l
                        .iter()
                        .filter_map(|v| v.as_string().map(|s| s.to_string()))
                        .collect();
                    implode(" ", &parts)
                }
                _ => last_command.as_string().unwrap_or("").to_string(),
            };
            let mut error_msg = self.process.get_error_output().to_string();
            if (credentials.len() as i64) > 0 {
                last_command_str = self.mask_credentials(&last_command_str, &credentials);
                error_msg = self.mask_credentials(&error_msg, &credentials);
            }
            self.throw_exception(
                &format!("Failed to execute {}\n\n{}", last_command_str, error_msg),
                url,
            )?;
        }

        Ok(())
    }

    pub fn sync_mirror(&mut self, url: &str, dir: &str) -> Result<bool> {
        let composer_disable_network = Platform::get_env("COMPOSER_DISABLE_NETWORK");
        if composer_disable_network
            .as_ref()
            .map(|v| !v.is_empty() && v != "0")
            .unwrap_or(false)
            && composer_disable_network.as_deref() != Some("prime")
        {
            self.io.write_error(
                PhpMixed::String(format!(
                    "<warning>Aborting git mirror sync of {} as network is disabled</warning>",
                    url
                )),
                true,
                IOInterface::NORMAL,
            );

            return Ok(false);
        }

        // update the repo if it is a valid git repository
        let mut output = String::new();
        if is_dir(dir)
            && self.process.execute(
                &vec![
                    "git".to_string(),
                    "rev-parse".to_string(),
                    "--git-dir".to_string(),
                ],
                &mut output,
                Some(dir.to_string()),
            ) == 0
            && trim(&output, None) == "."
        {
            // PHP try/finally
            let try_result: Result<()> = (|| -> Result<()> {
                let commands = vec![
                    vec![
                        "git".to_string(),
                        "remote".to_string(),
                        "set-url".to_string(),
                        "origin".to_string(),
                        "--".to_string(),
                        "%url%".to_string(),
                    ],
                    vec![
                        "git".to_string(),
                        "remote".to_string(),
                        "update".to_string(),
                        "--prune".to_string(),
                        "origin".to_string(),
                    ],
                    vec!["git".to_string(), "gc".to_string(), "--auto".to_string()],
                ];

                self.run_commands(commands, url, Some(dir), false, None)?;

                Ok(())
            })();
            // finally
            let _ = self.run_commands(
                vec![vec![
                    "git".to_string(),
                    "remote".to_string(),
                    "set-url".to_string(),
                    "origin".to_string(),
                    "--".to_string(),
                    "%sanitizedUrl%".to_string(),
                ]],
                url,
                Some(dir),
                false,
                None,
            );

            if let Err(e) = try_result {
                self.io.write_error(
                    PhpMixed::String(format!("<error>Sync mirror failed: {}</error>", e)),
                    true,
                    IOInterface::DEBUG,
                );

                return Ok(false);
            }

            return Ok(true);
        }
        Self::check_for_repo_ownership_error(self.process.get_error_output(), dir, None)?;

        // clean up directory and do a fresh clone into it
        self.filesystem.remove_directory(dir);

        self.run_commands(
            vec![vec![
                "git".to_string(),
                "clone".to_string(),
                "--mirror".to_string(),
                "--".to_string(),
                "%url%".to_string(),
                dir.to_string(),
            ]],
            url,
            Some(dir),
            true,
            None,
        )?;

        self.run_commands(
            vec![vec![
                "git".to_string(),
                "remote".to_string(),
                "set-url".to_string(),
                "origin".to_string(),
                "--".to_string(),
                "%sanitizedUrl%".to_string(),
            ]],
            url,
            Some(dir),
            false,
            None,
        )?;

        Ok(true)
    }

    pub fn fetch_ref_or_sync_mirror(
        &mut self,
        url: &str,
        dir: &str,
        r#ref: &str,
        pretty_version: Option<&str>,
    ) -> Result<bool> {
        if self.check_ref_is_in_mirror(dir, r#ref)? {
            if Preg::is_match(r"{^[a-f0-9]{40}$}", r#ref).unwrap_or(false)
                && pretty_version.is_some()
            {
                let branch = Preg::replace(
                    r"{(?:^dev-|(?:\.x)?-dev$)}i",
                    "",
                    pretty_version.unwrap().to_string(),
                );
                let mut branches: Option<String> = None;
                let mut tags: Option<String> = None;
                let mut output = String::new();
                if self.process.execute(
                    &vec!["git".to_string(), "branch".to_string()],
                    &mut output,
                    Some(dir.to_string()),
                ) == 0
                {
                    branches = Some(output);
                }
                let mut output = String::new();
                if self.process.execute(
                    &vec!["git".to_string(), "tag".to_string()],
                    &mut output,
                    Some(dir.to_string()),
                ) == 0
                {
                    tags = Some(output);
                }

                // if the pretty version cannot be found as a branch (nor branch with 'v' in front of the branch as it may have been stripped when generating pretty name),
                // nor as a tag, then we sync the mirror as otherwise it will likely fail during install.
                // this can occur if a git tag gets created *after* the reference is already put into the cache, as the ref check above will then not sync the new tags
                // see https://github.com/composer/composer/discussions/11002
                if branches.is_some()
                    && !Preg::is_match(
                        &format!(r"{{^[\s*]*v?{}$}}m", preg_quote(&branch, None)),
                        branches.as_deref().unwrap_or(""),
                    )
                    .unwrap_or(false)
                    && tags.is_some()
                    && !Preg::is_match(
                        &format!(r"{{^[\s*]*{}$}}m", preg_quote(&branch, None)),
                        tags.as_deref().unwrap_or(""),
                    )
                    .unwrap_or(false)
                {
                    self.sync_mirror(url, dir)?;
                }
            }

            return Ok(true);
        }

        if self.sync_mirror(url, dir)? {
            return self.check_ref_is_in_mirror(dir, r#ref);
        }

        Ok(false)
    }

    pub fn get_no_show_signature_flag(process: &ProcessExecutor) -> String {
        let git_version = Self::get_version(process);
        if let Some(v) = git_version {
            if version_compare(&v, "2.10.0-rc0", ">=") {
                return " --no-show-signature".to_string();
            }
        }

        String::new()
    }

    /// @return list<string>
    pub fn get_no_show_signature_flags(process: &ProcessExecutor) -> Vec<String> {
        let flags = Self::get_no_show_signature_flag(process);
        if flags.is_empty() {
            return vec![];
        }

        explode(" ", &substr(&flags, 1, None))
    }

    /// Checks if git version supports --no-commit-header flag (git 2.33+)
    ///
    /// @internal
    pub fn supports_no_commit_header_flag(process: &ProcessExecutor) -> bool {
        let git_version = Self::get_version(process);

        git_version
            .map(|v| version_compare(&v, "2.33.0-rc0", ">="))
            .unwrap_or(false)
    }

    /// Builds a git rev-list command with --no-commit-header flag when supported (git 2.33+)
    ///
    /// @internal
    /// @param list<string> $arguments Additional arguments for git rev-list
    /// @return non-empty-list<string>
    pub fn build_rev_list_command(
        process: &ProcessExecutor,
        arguments: Vec<String>,
    ) -> Vec<String> {
        let mut command = vec!["git".to_string(), "rev-list".to_string()];
        if Self::supports_no_commit_header_flag(process) {
            command.push("--no-commit-header".to_string());
        }

        command.extend(arguments);
        command
    }

    /// Parses git rev-list output, removing 'commit <hash>' header lines for git < 2.33.
    ///
    /// When --no-commit-header is not available (git < 2.33), git rev-list --format outputs
    /// "commit <hash>" before formatted output. This removes those lines.
    ///
    /// @internal
    pub fn parse_rev_list_output(output: &str, process: &ProcessExecutor) -> String {
        // If git supports --no-commit-header, output is already clean
        if Self::supports_no_commit_header_flag(process) {
            return output.to_string();
        }

        // Filter out "commit <hash>" lines for older git versions
        Preg::replace(r"{^commit [a-f0-9]{40}\n?}m", "", output.to_string())
    }

    fn check_ref_is_in_mirror(&mut self, dir: &str, r#ref: &str) -> Result<bool> {
        let mut output = String::new();
        if is_dir(dir)
            && self.process.execute(
                &vec![
                    "git".to_string(),
                    "rev-parse".to_string(),
                    "--git-dir".to_string(),
                ],
                &mut output,
                Some(dir.to_string()),
            ) == 0
            && trim(&output, None) == "."
        {
            let mut ignored_output = String::new();
            let exit_code = self.process.execute(
                &vec![
                    "git".to_string(),
                    "rev-parse".to_string(),
                    "--quiet".to_string(),
                    "--verify".to_string(),
                    format!("{}^{{commit}}", r#ref),
                ],
                &mut ignored_output,
                Some(dir.to_string()),
            );
            if exit_code == 0 {
                return Ok(true);
            }
        }
        Self::check_for_repo_ownership_error(self.process.get_error_output(), dir, None)?;

        Ok(false)
    }

    /// @return array<int, string>|null
    fn get_authentication_failure(&self, url: &str) -> Option<IndexMap<i32, String>> {
        let m = Preg::is_match_strict_groups(r"{^(https?://)([^/]+)(.*)$}i", url)?;

        let auth_failures = [
            "fatal: Authentication failed",
            "remote error: Invalid username or password.",
            "error: 401 Unauthorized",
            "fatal: unable to access",
            "fatal: could not read Username",
        ];

        let error_output = self.process.get_error_output();
        for auth_failure in &auth_failures {
            if strpos(error_output, auth_failure).is_some() {
                return Some(m);
            }
        }

        None
    }

    pub fn get_mirror_default_branch(
        &mut self,
        url: &str,
        dir: &str,
        is_local_path_repository: bool,
    ) -> Option<String> {
        if Platform::get_env("COMPOSER_DISABLE_NETWORK")
            .map(|v| !v.is_empty() && v != "0")
            .unwrap_or(false)
        {
            return None;
        }

        let result: Result<Option<String>> = (|| -> Result<Option<String>> {
            let mut output_mixed = PhpMixed::String(String::new());
            if is_local_path_repository {
                let mut output = String::new();
                self.process.execute(
                    &vec![
                        "git".to_string(),
                        "remote".to_string(),
                        "show".to_string(),
                        "origin".to_string(),
                    ],
                    &mut output,
                    Some(dir.to_string()),
                );
                output_mixed = PhpMixed::String(output);
            } else {
                let commands = vec![
                    vec![
                        "git".to_string(),
                        "remote".to_string(),
                        "set-url".to_string(),
                        "origin".to_string(),
                        "--".to_string(),
                        "%url%".to_string(),
                    ],
                    vec![
                        "git".to_string(),
                        "remote".to_string(),
                        "show".to_string(),
                        "origin".to_string(),
                    ],
                    vec![
                        "git".to_string(),
                        "remote".to_string(),
                        "set-url".to_string(),
                        "origin".to_string(),
                        "--".to_string(),
                        "%sanitizedUrl%".to_string(),
                    ],
                ];

                self.run_commands(commands, url, Some(dir), false, Some(&mut output_mixed))?;
            }

            let lines = self
                .process
                .split_lines(output_mixed.as_string().unwrap_or(""));
            for line in lines {
                if let Some(matches) =
                    Preg::is_match_strict_groups(r"{^\s*HEAD branch:\s(.+)\s*$}m", &line)
                {
                    return Ok(Some(matches.get(1).cloned().unwrap_or_default()));
                }
            }

            Ok(None)
        })();
        match result {
            Ok(v) => v,
            Err(e) => {
                self.io.write_error(
                    PhpMixed::String(format!(
                        "<error>Failed to fetch root identifier from remote: {}</error>",
                        e
                    )),
                    true,
                    IOInterface::DEBUG,
                );
                None
            }
        }
    }

    pub fn clean_env(process: &ProcessExecutor) {
        // PHP: $process ?? new ProcessExecutor()
        let git_version = Self::get_version(process);
        if let Some(v) = git_version {
            if version_compare(&v, "2.3.0", ">=") {
                // added in git 2.3.0, prevents prompting the user for username/password
                if Platform::get_env("GIT_TERMINAL_PROMPT").as_deref() != Some("0") {
                    Platform::put_env("GIT_TERMINAL_PROMPT", "0");
                }
            } else {
                // added in git 1.7.1, prevents prompting the user for username/password
                if Platform::get_env("GIT_ASKPASS").as_deref() != Some("echo") {
                    Platform::put_env("GIT_ASKPASS", "echo");
                }
            }
        }

        // clean up rogue git env vars in case this is running in a git hook
        if Platform::get_env("GIT_DIR").is_some() {
            Platform::clear_env("GIT_DIR");
        }
        if Platform::get_env("GIT_WORK_TREE").is_some() {
            Platform::clear_env("GIT_WORK_TREE");
        }

        // Run processes with predictable LANGUAGE
        if Platform::get_env("LANGUAGE").as_deref() != Some("C") {
            Platform::put_env("LANGUAGE", "C");
        }

        // clean up env for OSX, see https://github.com/composer/composer/issues/2146#issuecomment-35478940
        Platform::clear_env("DYLD_LIBRARY_PATH");
    }

    /// @return non-empty-string
    pub fn get_github_domains_regex(config: &Config) -> String {
        let domains: Vec<String> = match config.get("github-domains") {
            PhpMixed::List(l) => l
                .iter()
                .filter_map(|v| v.as_string().map(|s| s.to_string()))
                .collect(),
            _ => vec![],
        };
        let escaped: Vec<String> = array_map(|s: &String| preg_quote(s, None), &domains);
        format!("({})", implode("|", &escaped))
    }

    /// @return non-empty-string
    pub fn get_gitlab_domains_regex(config: &Config) -> String {
        let domains: Vec<String> = match config.get("gitlab-domains") {
            PhpMixed::List(l) => l
                .iter()
                .filter_map(|v| v.as_string().map(|s| s.to_string()))
                .collect(),
            _ => vec![],
        };
        let escaped: Vec<String> = array_map(|s: &String| preg_quote(s, None), &domains);
        format!("({})", implode("|", &escaped))
    }

    /// @param non-empty-string $message
    ///
    /// @return never
    fn throw_exception(&mut self, message: &str, url: &str) -> Result<()> {
        // git might delete a directory when it fails and php will not know
        clearstatcache();

        let mut ignored_output = String::new();
        if self.process.execute(
            &vec!["git".to_string(), "--version".to_string()],
            &mut ignored_output,
            None,
        ) != 0
        {
            return Err(RuntimeException {
                message: Url::sanitize(format!(
                    "Failed to clone {}, git was not found, check that it is installed and in your PATH env.\n\n{}",
                    url,
                    self.process.get_error_output()
                )),
                code: 0,
            }
            .into());
        }

        Err(RuntimeException {
            message: Url::sanitize(message.to_string()),
            code: 0,
        }
        .into())
    }

    /// Retrieves the current git version.
    ///
    /// @return string|null The git version number, if present.
    pub fn get_version(process: &ProcessExecutor) -> Option<String> {
        let mut version = VERSION.lock().unwrap();
        if version.is_none() {
            *version = Some(None);
            let mut output = String::new();
            // TODO(phase-b): ProcessExecutor::execute takes &mut self; this static fn takes &ProcessExecutor
            // For now, mimic the call signature (compilation fix is Phase B)
            let exit_code: i64 = 0; // process.execute(&["git", "--version"].map(String::from).to_vec(), &mut output, None);
            if exit_code == 0 {
                if let Some(matches) =
                    Preg::is_match_strict_groups(r"/^git version (\d+(?:\.\d+)+)/m", &output)
                {
                    *version = Some(matches.get(1).cloned());
                }
            }
        }
        version.clone().unwrap_or(None)
    }

    /// @param string[] $credentials
    fn mask_credentials(&self, error: &str, credentials: &[String]) -> String {
        let mut masked_credentials: Vec<String> = vec![];

        for credential in credentials {
            if in_array(
                PhpMixed::String(credential.clone()),
                &PhpMixed::List(vec![
                    Box::new(PhpMixed::String("private-token".to_string())),
                    Box::new(PhpMixed::String("x-token-auth".to_string())),
                    Box::new(PhpMixed::String("oauth2".to_string())),
                    Box::new(PhpMixed::String("gitlab-ci-token".to_string())),
                    Box::new(PhpMixed::String("x-oauth-basic".to_string())),
                ]),
                false,
            ) {
                masked_credentials.push(credential.clone());
            } else if strlen(credential) > 6 {
                masked_credentials.push(format!(
                    "{}...{}",
                    substr(credential, 0, Some(3)),
                    substr(credential, -3, None)
                ));
            } else if strlen(credential) > 3 {
                masked_credentials.push(format!("{}...", substr(credential, 0, Some(3))));
            } else {
                masked_credentials.push("XXX".to_string());
            }
        }

        str_replace_array(credentials, &masked_credentials, error)
    }
}
