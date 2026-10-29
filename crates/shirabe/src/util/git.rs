//! ref: composer/src/Composer/Util/Git.php

use crate::config::Config;
use crate::io::{IOInterface, IOInterfaceImmutable, io_interface};
use crate::util::{
    AuthHelper, Bitbucket, CommandLine, Filesystem, GitHub, GitLab, HttpDownloader, Platform,
    ProcessExecutor, StoreAuth, Url,
};
use bstr::{BStr, BString, ByteSlice};
use indexmap::IndexMap;
use shirabe_php_shim::{
    AnyThrowable, CmpOp, InvalidArgumentException, PHP_EOL, PhpMixed, PregMatches,
    RuntimeException, array_map, bconcat, clearstatcache, explode, explode_with_limit, implode,
    in_array_loose, in_array_strict, is_dir, path_to_bytes, php_regex, preg_is_match, preg_match,
    preg_quote, preg_replace, rawurldecode, rawurlencode, str_replace_array, strlen, strpos,
    substr, trim, version_compare,
};
use std::path::{Path, PathBuf};
use std::sync::Mutex;

#[derive(Debug)]
pub struct Git {
    io: std::rc::Rc<std::cell::RefCell<dyn IOInterface>>,
    config: std::rc::Rc<std::cell::RefCell<Config>>,
    process: std::rc::Rc<std::cell::RefCell<ProcessExecutor>>,
    filesystem: std::rc::Rc<std::cell::RefCell<Filesystem>>,
    http_downloader: Option<std::rc::Rc<std::cell::RefCell<HttpDownloader>>>,
}

static VERSION: Mutex<Option<Option<BString>>> = Mutex::new(None);

impl Git {
    pub fn new(
        io: std::rc::Rc<std::cell::RefCell<dyn IOInterface>>,
        config: std::rc::Rc<std::cell::RefCell<Config>>,
        process: std::rc::Rc<std::cell::RefCell<ProcessExecutor>>,
        fs: std::rc::Rc<std::cell::RefCell<Filesystem>>,
    ) -> Self {
        Self {
            io,
            config,
            process,
            filesystem: fs,
            http_downloader: None,
        }
    }

    pub fn check_for_repo_ownership_error(
        output: &BStr,
        path: &Path,
        io: Option<std::rc::Rc<std::cell::RefCell<dyn IOInterface>>>,
    ) -> anyhow::Result<()> {
        if output.contains_str("fatal: detected dubious ownership") {
            let msg = format!(
                "The repository at \"{}\" does not have the correct ownership and git refuses to use it:{}{}{}",
                path.display(),
                PHP_EOL,
                PHP_EOL,
                output.display_lossy()
            );
            match io {
                None => {
                    return Err(RuntimeException::new(msg).into());
                }
                Some(io) => {
                    io.write_error3(
                        &format!("<warning>{}</warning>", msg),
                        true,
                        io_interface::NORMAL,
                    );
                }
            }
        }
        Ok(())
    }

    pub fn set_http_downloader(
        &mut self,
        http_downloader: std::rc::Rc<std::cell::RefCell<HttpDownloader>>,
    ) {
        self.http_downloader = Some(http_downloader);
    }

    /// Runs a set of commands using the $url or a variation of it (with auth, ssh, ..)
    ///
    /// Commands should use %url% placeholders for the URL instead of inlining it to allow this function to do its job
    /// %sanitizedUrl% is also automatically replaced by the url without user/pass
    ///
    /// As soon as a single command fails it will halt, so assume the commands are run as && in bash
    pub fn run_commands(
        &mut self,
        commands: Vec<Vec<BString>>,
        url: &BStr,
        cwd: Option<&Path>,
        initial_clone: bool,
        command_output: impl RunCommandOutput,
    ) -> anyhow::Result<()> {
        let mut callables: Vec<Box<dyn Fn(&BStr) -> CommandLine>> = vec![];
        for cmd in commands {
            callables.push(Box::new(move |url: &BStr| -> CommandLine {
                let mut map: IndexMap<BString, BString> = IndexMap::new();
                map.insert("%url%".into(), url.to_owned());
                map.insert(
                    "%sanitizedUrl%".into(),
                    preg_replace(php_regex!(r"{://([^@]+?):(.+?)@}"), "://", url),
                );

                CommandLine::Args(array_map(
                    |value: &BString| map.get(value).cloned().unwrap_or_else(|| value.clone()),
                    &cmd,
                ))
            }));
        }

        self.run_command(callables, url, cwd, initial_clone, command_output)
    }

    /// For testing only. Public seam over the (deprecated) private `run_command`,
    /// mirroring `Git::runCommand` as exercised by `GitTest`.
    pub fn __run_command(
        &mut self,
        command_callable: Vec<Box<dyn Fn(&BStr) -> CommandLine>>,
        url: &BStr,
        cwd: Option<&Path>,
        initial_clone: bool,
        command_output: impl RunCommandOutput,
    ) -> anyhow::Result<()> {
        self.run_command(command_callable, url, cwd, initial_clone, command_output)
    }

    fn run_command(
        &mut self,
        command_callable: Vec<Box<dyn Fn(&BStr) -> CommandLine>>,
        url: &BStr,
        cwd: Option<&Path>,
        initial_clone: bool,
        mut command_output: impl RunCommandOutput,
    ) -> anyhow::Result<()> {
        let command_callables = command_callable;
        let mut last_command = CommandLine::Shell(BString::default());

        // Ensure we are allowed to use this URL by config
        self.config.borrow_mut().prohibit_url_by_config(
            url,
            Some(self.io.clone()),
            &IndexMap::new(),
        )?;

        let orig_cwd: Option<PathBuf> = if initial_clone {
            cwd.map(Path::to_path_buf)
        } else {
            None
        };

        // PHP closure: $runCommands = function ($url) use (...) { ... };
        let run_commands_inline = |url_arg: &BStr,
                                   this_process: &mut ProcessExecutor,
                                   last_cmd: &mut CommandLine,
                                   output: &mut dyn RunCommandOutput|
         -> i64 {
            let collect_outputs = output.collect_outputs();
            let mut outputs: Vec<BString> = vec![];

            let mut status: i64 = 0;
            for (counter, callable) in command_callables.iter().enumerate() {
                let cmd = callable(url_arg);
                *last_cmd = cmd.clone();
                let exec_cwd = if initial_clone && counter == 0 {
                    None
                } else {
                    cwd
                };
                if collect_outputs {
                    let mut local_output = BString::default();
                    status = this_process
                        .execute(cmd, &mut local_output, exec_cwd)
                        .unwrap_or(1);
                    outputs.push(local_output);
                } else {
                    status = this_process
                        .execute(cmd, output.make_handler().unwrap(), exec_cwd)
                        .unwrap_or(1);
                }
                if status != 0 {
                    break;
                }
            }

            if collect_outputs {
                output.write_back(implode("", &outputs));
            }

            status
        };

        if preg_is_match(php_regex!(r"{^ssh://[^@]+@[^:]+:[^0-9]+}"), url) {
            return Err(InvalidArgumentException::new(format!(
                "The source URL {} is invalid, ssh URLs should have a port number after \":\".\nUse ssh://git@example.com:22/path or just git@example.com:path if you do not want to provide a password or custom port.",
                url.display_lossy()
            ))
            .into());
        }

        if !initial_clone {
            // capture username/password from URL if there is one and we have no auth configured yet
            let mut output = BString::default();
            self.process.borrow_mut().execute(
                &[BString::from("git"), "remote".into(), "-v".into()][..],
                &mut output,
                cwd,
            )?;
            if let Some(m) = preg_match(
                php_regex!(r"{^(?:composer|origin)\s+https?://(.+):(.+)@([^/]+)}im"),
                &output,
            ) {
                let m3 = m.get(3).unwrap_or_default();
                if !self.io.has_authentication(m3) {
                    self.io.borrow_mut().set_authentication(
                        m3.to_owned(),
                        rawurldecode(m.get(1).unwrap_or_default()),
                        Some(rawurldecode(m.get(2).unwrap_or_default())),
                    );
                }
            }
        }

        let protocols = self.config.borrow_mut().get("github-protocols");
        // public github, autoswitch protocols
        if let Some(m) = preg_match(
            format!(
                "{{^(?:https?|git)://{}/(.*)}}",
                Self::get_github_domains_regex(&self.config.borrow())
            ),
            url,
        ) {
            let mut messages: Vec<BString> = vec![];
            let protocols_list: Vec<BString> = match &protocols {
                PhpMixed::List(l) => l
                    .iter()
                    .filter_map(|v| v.as_string().map(BStr::to_owned))
                    .collect(),
                _ => vec![],
            };
            for protocol in &protocols_list {
                let m1 = m.get(1).unwrap_or_default();
                let m2 = m.get(2).unwrap_or_default();
                let proto_url = if protocol == "ssh" {
                    bconcat!("git@", m1, ":", m2)
                } else {
                    bconcat!(protocol, "://", m1, "/", m2)
                };

                if run_commands_inline(
                    proto_url.as_bstr(),
                    &mut self.process.borrow_mut(),
                    &mut last_command,
                    &mut command_output,
                ) == 0
                {
                    return Ok(());
                }
                messages.push(bconcat!(
                    "- ",
                    proto_url,
                    "\n",
                    preg_replace(r"#^#m", "  ", self.process.borrow().get_error_output())
                ));

                if initial_clone && let Some(ref orig) = orig_cwd {
                    self.filesystem.borrow_mut().remove_directory(orig);
                }
            }

            // failed to checkout, first check git accessibility
            let m1 = m.get(1).unwrap_or_default();
            if !self.io.has_authentication(m1) && !self.io.is_interactive() {
                self.throw_exception(
                    bconcat!(
                        "Failed to clone ",
                        url,
                        " via ",
                        implode(", ", &protocols_list),
                        " protocols, aborting.\n\n",
                        implode("\n", &messages)
                    )
                    .as_bstr(),
                    url,
                )?;
            }
        }

        // if we have a private github url and the ssh protocol is disabled then we skip it and directly fallback to https
        let protocols = self.config.borrow_mut().get("github-protocols");
        let bypass_ssh_for_github = preg_is_match(
            format!(
                "{{^git@{}:(.+?)\\.git$}}i",
                Self::get_github_domains_regex(&self.config.borrow())
            ),
            url,
        ) && !in_array_strict("ssh".to_string(), protocols.values());

        let mut auth: Option<IndexMap<String, Option<BString>>> = None;
        let mut credentials: Vec<BString> = vec![];
        if bypass_ssh_for_github
            || 0 != run_commands_inline(
                url,
                &mut self.process.borrow_mut(),
                &mut last_command,
                &mut command_output,
            )
        {
            let mut error_msg = self.process.borrow().get_error_output();
            // private github repository without ssh key access, try https with auth
            let github_matched = preg_match(
                format!(
                    "{{^git@{}:(.+?)\\.git$}}i",
                    Self::get_github_domains_regex(&self.config.borrow())
                ),
                url,
            )
            .or_else(|| {
                preg_match(
                    format!(
                        "{{^https?://{}/(.*?)(?:\\.git)?$}}i",
                        Self::get_github_domains_regex(&self.config.borrow())
                    ),
                    url,
                )
            });
            if let Some(m) = github_matched {
                let m1 = m.get(1).unwrap_or_default();
                let m2 = m.get(2).unwrap_or_default();
                let origin = m1;
                if !self.io.has_authentication(origin) {
                    let mut git_hub_util = GitHub::new(
                        self.io.clone(),
                        self.config.clone(),
                        Some(self.process.clone()),
                        self.http_downloader.clone(),
                    )?;
                    let message = "Cloning failed using an ssh key for authentication, enter your GitHub credentials to access private repos";

                    if !git_hub_util.authorize_oauth(origin)? && self.io.is_interactive() {
                        git_hub_util.authorize_oauth_interactively(origin, Some(message))?;
                    }
                }

                if self.io.has_authentication(origin) {
                    auth = Some(self.io.get_authentication(origin));
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
                    let auth_url = bconcat!(
                        "https://",
                        rawurlencode(&username),
                        ":",
                        rawurlencode(&password),
                        "@",
                        m1,
                        "/",
                        m2,
                        ".git"
                    );
                    if run_commands_inline(
                        auth_url.as_bstr(),
                        &mut self.process.borrow_mut(),
                        &mut last_command,
                        &mut command_output,
                    ) == 0
                    {
                        return Ok(());
                    }

                    credentials = vec![rawurlencode(&username), rawurlencode(&password)];
                    error_msg = self.process.borrow().get_error_output();
                }
            } else if let Some(m) = preg_match(
                php_regex!(r"{^(https?)://(bitbucket\.org)/(.*?)(?:\.git)?$}i"),
                url,
            )
            .or_else(|| preg_match(php_regex!(r"{^(git)@(bitbucket\.org):(.+?\.git)$}i"), url))
            {
                // bitbucket either through oauth or app password, with fallback to ssh.
                let mut bitbucket_util = Bitbucket::new(
                    self.io.clone(),
                    self.config.clone(),
                    Some(self.process.clone()),
                    self.http_downloader.clone(),
                    None,
                )?;

                let domain_bytes = m.get(2).unwrap_or_default();
                let domain = domain_bytes;
                let mut repo_with_git_part = m.get(3).unwrap_or_default().to_owned();
                if !repo_with_git_part.ends_with(b".git") {
                    repo_with_git_part.extend_from_slice(b".git");
                }
                if !self.io.has_authentication(domain) {
                    let message = "Enter your Bitbucket credentials to access private repos";

                    if !bitbucket_util.authorize_oauth(domain)? && self.io.is_interactive() {
                        bitbucket_util.authorize_oauth_interactively(domain, Some(message))?;
                        let access_token = bitbucket_util.get_token();
                        self.io.borrow_mut().set_authentication(
                            domain.to_owned(),
                            BString::from("x-token-auth"),
                            Some(access_token),
                        );
                    }
                }

                // First we try to authenticate with whatever we have stored.
                if self.io.has_authentication(domain) {
                    auth = Some(self.io.get_authentication(domain));
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
                        username = BString::from("x-bitbucket-api-token-auth");
                    }

                    let auth_url = bconcat!(
                        "https://",
                        rawurlencode(&username),
                        ":",
                        rawurlencode(&password),
                        "@",
                        domain_bytes,
                        "/",
                        repo_with_git_part
                    );

                    if run_commands_inline(
                        auth_url.as_bstr(),
                        &mut self.process.borrow_mut(),
                        &mut last_command,
                        &mut command_output,
                    ) == 0
                    {
                        return Ok(());
                    }

                    // We already have an access_token from a previous request.
                    if username != "x-token-auth" {
                        let access_token = bitbucket_util.request_token(
                            domain,
                            username.as_bstr(),
                            password.as_bstr(),
                        )?;
                        if !access_token.is_empty() {
                            self.io.borrow_mut().set_authentication(
                                domain.to_owned(),
                                BString::from("x-token-auth"),
                                Some(access_token),
                            );
                        }
                    }
                }

                if self.io.has_authentication(domain) {
                    auth = Some(self.io.get_authentication(domain));
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
                    let auth_url = bconcat!(
                        "https://",
                        rawurlencode(&username),
                        ":",
                        rawurlencode(&password),
                        "@",
                        domain_bytes,
                        "/",
                        repo_with_git_part
                    );
                    if run_commands_inline(
                        auth_url.as_bstr(),
                        &mut self.process.borrow_mut(),
                        &mut last_command,
                        &mut command_output,
                    ) == 0
                    {
                        return Ok(());
                    }

                    credentials = vec![rawurlencode(&username), rawurlencode(&password)];
                }
                // Falling back to ssh
                let ssh_url = bconcat!("git@bitbucket.org:", repo_with_git_part);
                self.io.write_error3(
                    "    No bitbucket authentication configured. Falling back to ssh.",
                    true,
                    io_interface::NORMAL,
                );
                if run_commands_inline(
                    ssh_url.as_bstr(),
                    &mut self.process.borrow_mut(),
                    &mut last_command,
                    &mut command_output,
                ) == 0
                {
                    return Ok(());
                }

                error_msg = self.process.borrow().get_error_output();
            } else if let Some(m) = preg_match(
                format!(
                    "{{^(git)@{}:(.+?\\.git)$}}i",
                    Self::get_gitlab_domains_regex(&self.config.borrow())
                ),
                url,
            )
            .or_else(|| {
                preg_match(
                    format!(
                        "{{^(https?)://{}/(.*)}}i",
                        Self::get_gitlab_domains_regex(&self.config.borrow())
                    ),
                    url,
                )
            }) {
                let mut m1 = m.get(1).unwrap_or_default().to_owned();
                let m2 = m.get(2).unwrap_or_default();
                let m3 = m.get(3).unwrap_or_default();
                if m1 == "git" {
                    m1 = "https".into();
                }

                let origin = m2;
                if !self.io.has_authentication(origin) {
                    let mut git_lab_util = GitLab::new(
                        self.io.clone(),
                        self.config.clone(),
                        Some(self.process.clone()),
                        self.http_downloader.clone(),
                    )?;
                    let message =
                        "Cloning failed, enter your GitLab credentials to access private repos";

                    if !git_lab_util.authorize_oauth(origin)? && self.io.is_interactive() {
                        git_lab_util.authorize_oauth_interactively(
                            m1.as_bstr(),
                            origin,
                            Some(message),
                        )?;
                    }
                }

                if self.io.has_authentication(origin) {
                    auth = Some(self.io.get_authentication(origin));
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
                        bconcat!(
                            m1,
                            "://",
                            rawurlencode(&password),
                            ":",
                            rawurlencode(&username),
                            "@",
                            m2,
                            "/",
                            m3
                        ) // swap username and password
                    } else {
                        bconcat!(
                            m1,
                            "://",
                            rawurlencode(&username),
                            ":",
                            rawurlencode(&password),
                            "@",
                            m2,
                            "/",
                            m3
                        )
                    };

                    if run_commands_inline(
                        auth_url.as_bstr(),
                        &mut self.process.borrow_mut(),
                        &mut last_command,
                        &mut command_output,
                    ) == 0
                    {
                        return Ok(());
                    }

                    credentials = vec![rawurlencode(&username), rawurlencode(&password)];
                    error_msg = self.process.borrow().get_error_output();
                }
            } else if let Some(m) = self.get_authentication_failure(url) {
                // private non-github/gitlab/bitbucket repo that failed to authenticate
                let m1 = m.get(1).unwrap_or_default();
                let mut m2 = m.get(2).unwrap_or_default().to_owned();
                let m3 = m.get(3).unwrap_or_default();
                let mut auth_parts: Option<BString> = None;
                if m2.contains_str("@") {
                    let mut parts = explode_with_limit("@", &m2, 2).into_iter();
                    auth_parts = parts.next();
                    m2 = parts.next().unwrap_or_default();
                }

                let origin = m2.clone();
                let mut store_auth: PhpMixed = PhpMixed::Bool(false);
                if self.io.has_authentication(origin.as_bstr()) {
                    auth = Some(self.io.get_authentication(origin.as_bstr()));
                } else if self.io.is_interactive() {
                    let mut default_username: Option<BString> = None;
                    if let Some(ref parts) = auth_parts
                        && !parts.is_empty()
                    {
                        if parts.contains_str(":") {
                            default_username = explode_with_limit(":", parts, 2).into_iter().next();
                        } else {
                            default_username = Some(parts.clone());
                        }
                    }

                    self.io.write_error3(
                        &format!(
                            "    Authentication required (<info>{}</info>):",
                            m2.display_lossy()
                        ),
                        true,
                        io_interface::NORMAL,
                    );
                    self.io.write_error3(
                        &format!("<warning>{}</warning>", trim(&error_msg).display_lossy()),
                        true,
                        io_interface::VERBOSE,
                    );
                    let mut auth_map: IndexMap<String, Option<BString>> = IndexMap::new();
                    auth_map.insert(
                        "username".to_string(),
                        self.io
                            .ask(
                                "      Username: ".to_string(),
                                default_username
                                    .map(PhpMixed::string)
                                    .unwrap_or(PhpMixed::Null),
                            )?
                            .as_string()
                            .map(BString::from),
                    );
                    auth_map.insert(
                        "password".to_string(),
                        self.io
                            .ask_and_hide_answer("      Password: ".to_string())?
                            .map(BString::from),
                    );
                    auth = Some(auth_map);
                    store_auth = self.config.borrow_mut().get("store-auths");
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
                    let auth_url = bconcat!(
                        m1,
                        rawurlencode(&username),
                        ":",
                        rawurlencode(&password),
                        "@",
                        m2,
                        m3
                    );

                    if run_commands_inline(
                        auth_url.as_bstr(),
                        &mut self.process.borrow_mut(),
                        &mut last_command,
                        &mut command_output,
                    ) == 0
                    {
                        self.io.borrow_mut().set_authentication(
                            origin.clone(),
                            username,
                            Some(password),
                        );
                        let auth_helper = AuthHelper::new(self.io.clone(), self.config.clone());
                        let store_auth_enum = match &store_auth {
                            PhpMixed::String(s) if s == "prompt" => StoreAuth::Prompt,
                            PhpMixed::Bool(b) => StoreAuth::Bool(*b),
                            _ => StoreAuth::Bool(false),
                        };
                        auth_helper.store_auth(origin.as_bstr(), store_auth_enum)?;

                        return Ok(());
                    }

                    credentials = vec![rawurlencode(&username), rawurlencode(&password)];
                    error_msg = self.process.borrow().get_error_output();
                }
            }

            if initial_clone && let Some(ref orig) = orig_cwd {
                self.filesystem.borrow_mut().remove_directory(orig);
            }

            let mut last_command_str = match &last_command {
                CommandLine::Args(args) => implode(" ", args),
                CommandLine::Shell(command) => command.clone(),
            };
            if !credentials.is_empty() {
                last_command_str = self.mask_credentials(last_command_str.as_bstr(), &credentials);
                error_msg = self.mask_credentials(error_msg.as_bstr(), &credentials);
            }
            self.throw_exception(
                bconcat!("Failed to execute ", last_command_str, "\n\n", error_msg).as_bstr(),
                url,
            )?;
        }

        Ok(())
    }

    pub fn sync_mirror(&mut self, url: &BStr, dir: &Path) -> anyhow::Result<bool> {
        let composer_disable_network = Platform::get_env("COMPOSER_DISABLE_NETWORK");
        if composer_disable_network
            .as_ref()
            .map(|v| !v.is_empty() && v != "0")
            .unwrap_or(false)
            && composer_disable_network.as_deref() != Some("prime")
        {
            self.io.write_error3(
                &format!(
                    "<warning>Aborting git mirror sync of {} as network is disabled</warning>",
                    url.display_lossy()
                ),
                true,
                io_interface::NORMAL,
            );

            return Ok(false);
        }

        // update the repo if it is a valid git repository
        let mut output = BString::default();
        if is_dir(dir)
            && self.process.borrow_mut().execute(
                &[BString::from("git"), "rev-parse".into(), "--git-dir".into()],
                &mut output,
                Some(dir),
            )? == 0
            && trim(&output) == "."
        {
            // PHP try/finally
            let try_result: anyhow::Result<()> = (|| -> anyhow::Result<()> {
                let commands: Vec<Vec<BString>> = vec![
                    vec![
                        "git".into(),
                        "remote".into(),
                        "set-url".into(),
                        "origin".into(),
                        "--".into(),
                        "%url%".into(),
                    ],
                    vec![
                        "git".into(),
                        "remote".into(),
                        "update".into(),
                        "--prune".into(),
                        "origin".into(),
                    ],
                    vec!["git".into(), "gc".into(), "--auto".into()],
                ];

                self.run_commands(commands, url, Some(dir), false, ())?;

                Ok(())
            })();
            // finally
            let finally_result = self.run_commands(
                vec![vec![
                    "git".into(),
                    "remote".into(),
                    "set-url".into(),
                    "origin".into(),
                    "--".into(),
                    "%sanitizedUrl%".into(),
                ]],
                url,
                Some(dir),
                false,
                (),
            );
            let outcome = match finally_result {
                Ok(()) => try_result,
                Err(mut thrown) => {
                    if let Some(pending) = try_result.err().map(std::sync::Arc::new)
                        && let Some(exception) = thrown.downcast_mut::<AnyThrowable>()
                    {
                        exception.set_previous(pending);
                    }
                    Err(thrown)
                }
            };

            if let Err(e) = outcome {
                self.io.write_error3(
                    &format!("<error>Sync mirror failed: {}</error>", e),
                    true,
                    io_interface::DEBUG,
                );

                return Ok(false);
            }

            return Ok(true);
        }
        Self::check_for_repo_ownership_error(
            self.process.borrow().get_error_output().as_bstr(),
            dir,
            None,
        )?;

        // clean up directory and do a fresh clone into it
        self.filesystem.borrow_mut().remove_directory(dir);

        self.run_commands(
            vec![vec![
                "git".into(),
                "clone".into(),
                "--mirror".into(),
                "--".into(),
                "%url%".into(),
                BString::from(path_to_bytes(dir)),
            ]],
            url,
            Some(dir),
            true,
            (),
        )?;

        self.run_commands(
            vec![vec![
                "git".into(),
                "remote".into(),
                "set-url".into(),
                "origin".into(),
                "--".into(),
                "%sanitizedUrl%".into(),
            ]],
            url,
            Some(dir),
            false,
            (),
        )?;

        Ok(true)
    }

    pub fn fetch_ref_or_sync_mirror(
        &mut self,
        url: &BStr,
        dir: &Path,
        r#ref: &BStr,
        pretty_version: Option<&BStr>,
    ) -> anyhow::Result<bool> {
        if self.check_ref_is_in_mirror(dir, r#ref)? {
            if preg_is_match(php_regex!(r"{^[a-f0-9]{40}$}"), r#ref)
                && let Some(pretty_version) = pretty_version
            {
                let branch = preg_replace(
                    php_regex!(r"{(?:^dev-|(?:\.x)?-dev$)}i"),
                    "",
                    pretty_version,
                );
                let mut branches: Option<BString> = None;
                let mut tags: Option<BString> = None;
                let mut output = BString::default();
                if self.process.borrow_mut().execute(
                    &[BString::from("git"), "branch".into()],
                    &mut output,
                    Some(dir),
                )? == 0
                {
                    branches = Some(output);
                }
                let mut output = BString::default();
                if self.process.borrow_mut().execute(
                    &[BString::from("git"), "tag".into()],
                    &mut output,
                    Some(dir),
                )? == 0
                {
                    tags = Some(output);
                }

                // if the pretty version cannot be found as a branch (nor branch with 'v' in front of the branch as it may have been stripped when generating pretty name),
                // nor as a tag, then we sync the mirror as otherwise it will likely fail during install.
                // this can occur if a git tag gets created *after* the reference is already put into the cache, as the ref check above will then not sync the new tags
                // see https://github.com/composer/composer/discussions/11002
                if branches.is_some()
                    && !preg_is_match(
                        format!(r"{{^[\s*]*v?{}$}}m", preg_quote(&branch, None)),
                        branches.as_deref().map(Vec::as_slice).unwrap_or_default(),
                    )
                    && tags.is_some()
                    && !preg_is_match(
                        format!(r"{{^[\s*]*{}$}}m", preg_quote(&branch, None)),
                        tags.as_deref().map(Vec::as_slice).unwrap_or_default(),
                    )
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

    pub fn get_no_show_signature_flag(
        process: &std::rc::Rc<std::cell::RefCell<ProcessExecutor>>,
    ) -> anyhow::Result<BString> {
        let git_version = Self::get_version(process)?;
        if let Some(v) = git_version
            && version_compare(&v, "2.10.0-rc0", CmpOp::Ge)
        {
            return Ok(" --no-show-signature".into());
        }

        Ok(BString::default())
    }

    pub fn get_no_show_signature_flags(
        process: &std::rc::Rc<std::cell::RefCell<ProcessExecutor>>,
    ) -> anyhow::Result<Vec<BString>> {
        let flags = Self::get_no_show_signature_flag(process)?;
        if flags.is_empty() {
            return Ok(vec![]);
        }

        Ok(explode(" ", substr(&flags, 1, None)))
    }

    /// Checks if git version supports --no-commit-header flag (git 2.33+)
    pub fn supports_no_commit_header_flag(
        process: &std::rc::Rc<std::cell::RefCell<ProcessExecutor>>,
    ) -> anyhow::Result<bool> {
        let git_version = Self::get_version(process)?;

        Ok(git_version
            .map(|v| version_compare(&v, "2.33.0-rc0", CmpOp::Ge))
            .unwrap_or(false))
    }

    /// Builds a git rev-list command with --no-commit-header flag when supported (git 2.33+)
    pub fn build_rev_list_command(
        process: &std::rc::Rc<std::cell::RefCell<ProcessExecutor>>,
        arguments: Vec<BString>,
    ) -> anyhow::Result<Vec<BString>> {
        let mut command: Vec<BString> = vec!["git".into(), "rev-list".into()];
        if Self::supports_no_commit_header_flag(process)? {
            command.push("--no-commit-header".into());
        }

        command.extend(arguments);
        Ok(command)
    }

    /// Parses git rev-list output, removing 'commit <hash>' header lines for git < 2.33.
    ///
    /// When --no-commit-header is not available (git < 2.33), git rev-list --format outputs
    /// "commit <hash>" before formatted output. This removes those lines.
    pub fn parse_rev_list_output(
        output: impl AsRef<[u8]>,
        process: &std::rc::Rc<std::cell::RefCell<ProcessExecutor>>,
    ) -> anyhow::Result<BString> {
        let output = output.as_ref();
        // If git supports --no-commit-header, output is already clean
        if Self::supports_no_commit_header_flag(process)? {
            return Ok(output.into());
        }

        // Filter out "commit <hash>" lines for older git versions
        Ok(preg_replace(
            php_regex!(r"{^commit [a-f0-9]{40}\n?}m"),
            "",
            output,
        ))
    }

    fn check_ref_is_in_mirror(&mut self, dir: &Path, r#ref: &BStr) -> anyhow::Result<bool> {
        let mut output = BString::default();
        if is_dir(dir)
            && self.process.borrow_mut().execute(
                &[BString::from("git"), "rev-parse".into(), "--git-dir".into()],
                &mut output,
                Some(dir),
            )? == 0
            && trim(&output) == "."
        {
            let mut ignored_output = BString::default();
            let exit_code = self.process.borrow_mut().execute(
                &[
                    BString::from("git"),
                    "rev-parse".into(),
                    "--quiet".into(),
                    "--verify".into(),
                    bconcat!(r#ref, "^{commit}"),
                ],
                &mut ignored_output,
                Some(dir),
            )?;
            if exit_code == 0 {
                return Ok(true);
            }
        }
        Self::check_for_repo_ownership_error(
            self.process.borrow().get_error_output().as_bstr(),
            dir,
            None,
        )?;

        Ok(false)
    }

    fn get_authentication_failure<'u>(&self, url: &'u BStr) -> Option<PregMatches<'u>> {
        let m = preg_match(php_regex!(r"{^(https?://)([^/]+)(.*)$}i"), url)?;

        let auth_failures = [
            "fatal: Authentication failed",
            "remote error: Invalid username or password.",
            "error: 401 Unauthorized",
            "fatal: unable to access",
            "fatal: could not read Username",
        ];

        let error_output = self.process.borrow().get_error_output();
        for auth_failure in &auth_failures {
            if strpos(&error_output, auth_failure).is_some() {
                return Some(m);
            }
        }

        None
    }

    pub fn get_mirror_default_branch(
        &mut self,
        url: &BStr,
        dir: &Path,
        is_local_path_repository: bool,
    ) -> Option<BString> {
        if Platform::get_env("COMPOSER_DISABLE_NETWORK")
            .map(|v| !v.is_empty() && v != "0")
            .unwrap_or(false)
        {
            return None;
        }

        let result: anyhow::Result<Option<BString>> = (|| -> anyhow::Result<Option<BString>> {
            let mut output_mixed = PhpMixed::Null;
            if is_local_path_repository {
                let mut output = BString::default();
                self.process.borrow_mut().execute(
                    &[
                        BString::from("git"),
                        "remote".into(),
                        "show".into(),
                        "origin".into(),
                    ],
                    &mut output,
                    Some(dir),
                )?;
                output_mixed = PhpMixed::string(output);
            } else {
                let commands: Vec<Vec<BString>> = vec![
                    vec![
                        "git".into(),
                        "remote".into(),
                        "set-url".into(),
                        "origin".into(),
                        "--".into(),
                        "%url%".into(),
                    ],
                    vec![
                        "git".into(),
                        "remote".into(),
                        "show".into(),
                        "origin".into(),
                    ],
                    vec![
                        "git".into(),
                        "remote".into(),
                        "set-url".into(),
                        "origin".into(),
                        "--".into(),
                        "%sanitizedUrl%".into(),
                    ],
                ];

                self.run_commands(commands, url, Some(dir), false, &mut output_mixed)?;
            }

            let lines = self
                .process
                .borrow()
                .split_lines(output_mixed.as_string().unwrap_or_default());
            for line in lines {
                if let Some(matches) =
                    preg_match(php_regex!(r"{^\s*HEAD branch:\s(.+)\s*$}m"), &line)
                {
                    return Ok(Some(matches.get(1).unwrap_or_default().to_owned()));
                }
            }

            Ok(None)
        })();
        match result {
            Ok(v) => v,
            Err(e) => {
                self.io.write_error3(
                    &format!(
                        "<error>Failed to fetch root identifier from remote: {}</error>",
                        e
                    ),
                    true,
                    io_interface::DEBUG,
                );
                None
            }
        }
    }

    pub fn clean_env(
        process: &std::rc::Rc<std::cell::RefCell<ProcessExecutor>>,
    ) -> anyhow::Result<()> {
        // PHP: $process ?? new ProcessExecutor()
        let git_version = Self::get_version(process)?;
        if let Some(v) = git_version {
            if version_compare(&v, "2.3.0", CmpOp::Ge) {
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

        Ok(())
    }

    pub fn get_github_domains_regex(config: &Config) -> String {
        Self::get_domains_regex(config, "github-domains")
    }

    pub fn get_gitlab_domains_regex(config: &Config) -> String {
        Self::get_domains_regex(config, "gitlab-domains")
    }

    fn get_domains_regex(config: &Config, key: &str) -> String {
        let domains: Vec<BString> = match config.get(key) {
            PhpMixed::List(l) => l
                .iter()
                .filter_map(|v| v.as_string().map(BStr::to_owned))
                .collect(),
            _ => vec![],
        };
        let escaped: Vec<String> = array_map(|s: &BString| preg_quote(s, None), &domains);
        format!("({})", escaped.join("|"))
    }

    fn throw_exception(&mut self, message: &BStr, url: &BStr) -> anyhow::Result<()> {
        // git might delete a directory when it fails and php will not know
        clearstatcache();

        let mut ignored_output = BString::default();
        if self.process.borrow_mut().execute(
            &[BString::from("git"), "--version".into()],
            &mut ignored_output,
            None,
        )? != 0
        {
            return Err(RuntimeException::new(
                Url::sanitize(bconcat!(
                    "Failed to clone ",
                    url,
                    ", git was not found, check that it is installed and in your PATH env.\n\n",
                    self.process.borrow().get_error_output()
                ))
                .display_lossy()
                .to_string(),
            )
            .into());
        }

        Err(RuntimeException::new(Url::sanitize(message).display_lossy().to_string()).into())
    }

    /// Retrieves the current git version.
    pub fn get_version(
        process: &std::rc::Rc<std::cell::RefCell<ProcessExecutor>>,
    ) -> anyhow::Result<Option<BString>> {
        let mut version = VERSION.lock().unwrap();
        if version.is_none() {
            *version = Some(None);
            let mut output = BString::default();
            let exit_code: i64 = process.borrow_mut().execute(
                &[BString::from("git"), "--version".into()],
                &mut output,
                None,
            )?;
            if exit_code == 0
                && let Some(matches) =
                    preg_match(php_regex!(r"/^git version (\d+(?:\.\d+)+)/m"), &output)
            {
                *version = Some(matches.get(1).map(BStr::to_owned));
            }
        }
        Ok(version.clone().unwrap_or(None))
    }

    /// For testing only. Resets the cached git `version` static so the next
    /// `get_version` call re-runs `git --version`, mirroring the
    /// `ReflectionProperty(GitUtil::class, 'version')->setValue(null, false)`
    /// done in VersionGuesserTest's setUp/tearDown.
    pub fn __reset_version() {
        *VERSION.lock().unwrap() = None;
    }

    /// For testing only. Seeds the cached git `version` static, mirroring the
    /// `ReflectionProperty(GitUtil::class, 'version')->setValue(null, $version)`
    /// done in GitDownloaderTest's `initGitVersion`. `Some(v)` records a detected
    /// version; `None` records that git is unavailable, both without shelling out.
    pub fn __set_version(version: Option<BString>) {
        *VERSION.lock().unwrap() = Some(version);
    }

    fn mask_credentials(&self, error: &BStr, credentials: &[BString]) -> BString {
        let mut masked_credentials: Vec<BString> = vec![];

        for credential in credentials {
            if in_array_loose(
                PhpMixed::string(credential.clone()),
                &[
                    PhpMixed::string("private-token"),
                    PhpMixed::string("x-token-auth"),
                    PhpMixed::string("oauth2"),
                    PhpMixed::string("gitlab-ci-token"),
                    PhpMixed::string("x-oauth-basic"),
                ],
            ) {
                masked_credentials.push(credential.clone());
            } else if strlen(credential) > 6 {
                masked_credentials.push(bconcat!(
                    substr(credential, 0, Some(3)),
                    "...",
                    substr(credential, -3, None)
                ));
            } else if strlen(credential) > 3 {
                masked_credentials.push(bconcat!(substr(credential, 0, Some(3)), "..."));
            } else {
                masked_credentials.push("XXX".into());
            }
        }

        str_replace_array(credentials, &masked_credentials, error)
    }
}

/// Models the `mixed &$commandOutput` parameter of `Git::runCommand` (cf.
/// `composer/src/Composer/Util/Git.php`). In PHP the behaviour is selected by
/// `is_callable($commandOutput)`; here each behaviour is a distinct implementing type, mirroring
/// `ProcessExecutor`'s [`IntoExecOutput`](crate::util::process_executor::IntoExecOutput):
///
/// | PHP call site | meaning | implementor | `collect_outputs` |
/// |---|---|---|---|
/// | `runCommands($c, ...)` (no `$commandOutput`) | accumulate then discard | `()` | `true` |
/// | `runCommands($c, ..., $var)` by-ref | accumulate and assign back | `&mut PhpMixed` | `true` |
/// | `runCommands($c, ..., $callable)` | drive the child through the handler | [`GitRunCommandOutputHandler`] | `false` |
pub trait RunCommandOutput {
    /// PHP: `$collectOutputs = !is_callable($commandOutput)`.
    fn collect_outputs(&self) -> bool;

    /// In callable mode, builds the per-command output handler passed to
    /// `ProcessExecutor::execute`. `None` in the accumulate/discard modes.
    fn make_handler(&self) -> Option<Box<dyn FnMut(&str, &bstr::BStr) -> anyhow::Result<bool>>>;

    /// PHP: `$commandOutput = implode('', $outputs);` — only meaningful in the accumulate-and-assign
    /// mode; a no-op for the discard and callable variants.
    fn write_back(&mut self, value: bstr::BString);
}

/// `runCommands(..., $commandOutput)` where the caller never reads `$commandOutput` (PHP default
/// `null`): outputs are accumulated per command but thrown away.
impl RunCommandOutput for () {
    fn collect_outputs(&self) -> bool {
        true
    }

    fn make_handler(&self) -> Option<Box<dyn FnMut(&str, &bstr::BStr) -> anyhow::Result<bool>>> {
        None
    }

    fn write_back(&mut self, _value: bstr::BString) {}
}

/// `runCommands(..., $var)` by-ref: outputs are accumulated and assigned back as a
/// [`PhpMixed::String`].
impl RunCommandOutput for &mut PhpMixed {
    fn collect_outputs(&self) -> bool {
        true
    }

    fn make_handler(&self) -> Option<Box<dyn FnMut(&str, &bstr::BStr) -> anyhow::Result<bool>>> {
        None
    }

    fn write_back(&mut self, value: bstr::BString) {
        **self = PhpMixed::string(value);
    }
}

/// `runCommands(..., $callable)`: the callable is used as the output handler for every command.
/// PHP passes the same `$commandOutput` callable to each `process->execute` call, so the handler
/// state is shared across all commands and protocol/auth retries.
pub struct GitRunCommandOutputHandler {
    handler: std::rc::Rc<std::cell::RefCell<dyn FnMut(&str, &bstr::BStr) -> anyhow::Result<bool>>>,
}

impl std::fmt::Debug for GitRunCommandOutputHandler {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("GitRunCommandOutputHandler").finish()
    }
}

impl GitRunCommandOutputHandler {
    pub fn new(handler: impl FnMut(&str, &bstr::BStr) -> anyhow::Result<bool> + 'static) -> Self {
        Self {
            handler: std::rc::Rc::new(std::cell::RefCell::new(handler)),
        }
    }
}

impl RunCommandOutput for GitRunCommandOutputHandler {
    fn collect_outputs(&self) -> bool {
        false
    }

    fn make_handler(&self) -> Option<Box<dyn FnMut(&str, &bstr::BStr) -> anyhow::Result<bool>>> {
        let handler = self.handler.clone();
        Some(Box::new(move |r#type: &str, buffer: &bstr::BStr| {
            (handler.borrow_mut())(r#type, buffer)
        }))
    }

    fn write_back(&mut self, _value: bstr::BString) {}
}
