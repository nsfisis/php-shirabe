//! ref: composer/src/Composer/Util/Svn.php

use crate::config::Config;
use crate::io::IOInterface;
use crate::io::IOInterfaceImmutable;
use crate::io::io_interface;
use crate::util::Platform;
use crate::util::ProcessExecutor;
use shirabe_pcre::Preg;
use shirabe_php_shim::{
    LogicException, PhpMixed, RuntimeException, implode, parse_url, php_regex, stripos, strpos,
    trim,
};
use std::sync::Mutex;

#[derive(Debug, Clone)]
pub struct SvnCredentials {
    pub username: String,
    pub password: String,
}

#[derive(Debug)]
pub struct Svn {
    /// @var ?array{username: string, password: string}
    credentials: Option<SvnCredentials>,
    /// @var bool
    has_auth: Option<bool>,
    /// @var IOInterface
    io: std::rc::Rc<std::cell::RefCell<dyn IOInterface>>,
    /// @var string
    url: String,
    /// @var bool
    cache_credentials: bool,
    /// @var ProcessExecutor
    process: std::rc::Rc<std::cell::RefCell<ProcessExecutor>>,
    /// @var int
    qty_auth_tries: i64,
    /// @var Config
    config: std::rc::Rc<std::cell::RefCell<Config>>,
}

/// @var string|null
static VERSION: Mutex<Option<String>> = Mutex::new(None);

impl Svn {
    const MAX_QTY_AUTH_TRIES: i64 = 5;

    pub fn new(
        url: String,
        io: std::rc::Rc<std::cell::RefCell<dyn IOInterface>>,
        config: std::rc::Rc<std::cell::RefCell<Config>>,
        process: Option<std::rc::Rc<std::cell::RefCell<ProcessExecutor>>>,
    ) -> Self {
        let process = process.unwrap_or_else(|| {
            std::rc::Rc::new(std::cell::RefCell::new(ProcessExecutor::new(Some(
                io.clone(),
            ))))
        });
        Self {
            url,
            io,
            config,
            process,
            credentials: None,
            has_auth: None,
            cache_credentials: true,
            qty_auth_tries: 0,
        }
    }

    pub fn clean_env() {
        // clean up env for OSX, see https://github.com/composer/composer/issues/2146#issuecomment-35478940
        Platform::clear_env("DYLD_LIBRARY_PATH");
    }

    /// Execute an SVN remote command and try to fix up the process with credentials
    /// if necessary.
    ///
    /// @param non-empty-list<string> $command SVN command to run
    /// @param string  $url     SVN url
    /// @param ?string $cwd     Working directory
    /// @param ?string $path    Target for a checkout
    /// @param bool    $verbose Output all output to the user
    ///
    /// @throws \RuntimeException
    pub fn execute(
        &mut self,
        command: Vec<String>,
        url: &str,
        cwd: Option<&str>,
        path: Option<&str>,
        verbose: bool,
    ) -> anyhow::Result<String> {
        // Ensure we are allowed to use this URL by config
        self.config.borrow_mut().prohibit_url_by_config(
            url,
            Some(self.io.clone()),
            &indexmap::IndexMap::new(),
        )?;

        self.execute_with_auth_retry(command, cwd, url, path, verbose)
            .map(|o| o.unwrap_or_default())
    }

    /// Execute an SVN local command and try to fix up the process with credentials
    /// if necessary.
    ///
    /// @param non-empty-list<string> $command SVN command to run
    /// @param string $path    Path argument passed thru to the command
    /// @param string $cwd     Working directory
    /// @param bool   $verbose Output all output to the user
    ///
    /// @throws \RuntimeException
    pub fn execute_local(
        &mut self,
        command: Vec<String>,
        path: &str,
        cwd: Option<&str>,
        verbose: bool,
    ) -> anyhow::Result<String> {
        // A local command has no remote url
        self.execute_with_auth_retry(command, cwd, "", Some(path), verbose)
            .map(|o| o.unwrap_or_default())
    }

    fn execute_with_auth_retry(
        &mut self,
        svn_command: Vec<String>,
        cwd: Option<&str>,
        url: &str,
        path: Option<&str>,
        verbose: bool,
    ) -> anyhow::Result<Option<String>> {
        // Regenerate the command at each try, to use the newly user-provided credentials
        let command = self.get_command(svn_command.clone(), url, path);

        let output: std::rc::Rc<std::cell::RefCell<Option<String>>> =
            std::rc::Rc::new(std::cell::RefCell::new(None));
        let io = self.io.clone();
        let output_for_handler = output.clone();
        let handler: Box<dyn FnMut(&str, &str) -> bool> =
            Box::new(move |r#type: &str, buffer: &str| {
                if r#type != "out" {
                    return false;
                }
                if strpos(buffer, "Redirecting to URL ") == Some(0) {
                    return false;
                }
                output_for_handler
                    .borrow_mut()
                    .get_or_insert_with(String::new)
                    .push_str(buffer);
                if verbose {
                    io.borrow().write_error2(buffer, false);
                }
                false
            });
        let status = self
            .process
            .borrow_mut()
            .execute(command.as_slice(), handler, cwd)?;
        let output = output.borrow().clone();
        if 0 == status {
            return Ok(output);
        }

        let error_output = self.process.borrow().get_error_output().to_string();
        let full_output = trim(
            &implode("\n", &[output.unwrap_or_default(), error_output]),
            None,
        );

        // the error is not auth-related
        if stripos(&full_output, "Could not authenticate to server:").is_none()
            && stripos(&full_output, "authorization failed").is_none()
            && stripos(&full_output, "svn: E170001:").is_none()
            && stripos(&full_output, "svn: E215004:").is_none()
        {
            return Err(RuntimeException::new(full_output).into());
        }

        if !self.has_auth() {
            self.do_auth_dance()?;
        }

        // try to authenticate if maximum quantity of tries not reached
        let tries = self.qty_auth_tries;
        self.qty_auth_tries += 1;
        if tries < Self::MAX_QTY_AUTH_TRIES {
            // restart the process
            return self.execute_with_auth_retry(svn_command, cwd, url, path, verbose);
        }

        Err(RuntimeException::new(format!("wrong credentials provided ({})", full_output)).into())
    }

    pub fn set_cache_credentials(&mut self, cache_credentials: bool) {
        self.cache_credentials = cache_credentials;
    }

    /// Repositories requests credentials, let's put them in.
    ///
    /// @throws \RuntimeException
    fn do_auth_dance(&mut self) -> anyhow::Result<&mut Self> {
        // cannot ask for credentials in non interactive mode
        if !self.io.is_interactive() {
            return Err(RuntimeException::new(
                "can not ask for authentication in non interactive mode".to_string(),
            )
            .into());
        }

        self.io.write_error3(
            &format!(
                "The Subversion server ({}) requested credentials:",
                self.url,
            ),
            true,
            io_interface::NORMAL,
        );

        self.has_auth = Some(true);
        self.credentials = Some(SvnCredentials {
            username: self
                .io
                .ask("Username: ".to_string(), PhpMixed::String("".to_string()))?
                .as_string()
                .unwrap_or("")
                .to_string(),
            password: self
                .io
                .ask_and_hide_answer("Password: ".to_string())
                .unwrap_or_default(),
        });

        self.cache_credentials = self.io.ask_confirmation(
            "Should Subversion cache these credentials? (yes/no) ".to_string(),
            true,
        );

        Ok(self)
    }

    /// A method to create the svn commands run.
    ///
    /// @param non-empty-list<string> $cmd  Usually 'svn ls' or something like that.
    /// @param string $url  Repo URL.
    /// @param string $path Target for a checkout
    fn get_command(&mut self, mut cmd: Vec<String>, url: &str, path: Option<&str>) -> Vec<String> {
        cmd.push("--non-interactive".to_string());
        cmd.extend(self.get_credential_args());
        cmd.push("--".to_string());
        cmd.push(url.to_string());

        if let Some(path) = path {
            cmd.push(path.to_string());
        }

        cmd
    }

    /// Return the credential string for the svn command.
    ///
    /// Adds --no-auth-cache when credentials are present.
    fn get_credential_args(&mut self) -> Vec<String> {
        if !self.has_auth() {
            return vec![];
        }

        let mut args = self.get_auth_cache_args();
        args.push("--username".to_string());
        args.push(self.get_username().unwrap());
        args.push("--password".to_string());
        args.push(self.get_password().unwrap());
        args
    }

    /// For testing only: invoke the crate-private `get_command`.
    pub fn __get_command(
        &mut self,
        cmd: Vec<String>,
        url: &str,
        path: Option<&str>,
    ) -> Vec<String> {
        self.get_command(cmd, url, path)
    }

    /// For testing only: invoke the crate-private `get_credential_args`.
    pub fn __get_credential_args(&mut self) -> Vec<String> {
        self.get_credential_args()
    }

    /// Get the password for the svn command. Can be empty.
    ///
    /// @throws \LogicException
    fn get_password(&self) -> anyhow::Result<String> {
        if self.credentials.is_none() {
            return Err(LogicException::new("No svn auth detected.".to_string()).into());
        }

        Ok(self.credentials.as_ref().unwrap().password.clone())
    }

    /// Get the username for the svn command.
    ///
    /// @throws \LogicException
    fn get_username(&self) -> anyhow::Result<String> {
        if self.credentials.is_none() {
            return Err(LogicException::new("No svn auth detected.".to_string()).into());
        }

        Ok(self.credentials.as_ref().unwrap().username.clone())
    }

    /// Detect Svn Auth.
    fn has_auth(&mut self) -> bool {
        if let Some(has_auth) = self.has_auth {
            return has_auth;
        }

        if !self.create_auth_from_config() {
            self.create_auth_from_url();
        }

        self.has_auth.unwrap_or(false)
    }

    /// Return the no-auth-cache switch.
    fn get_auth_cache_args(&self) -> Vec<String> {
        if self.cache_credentials {
            vec![]
        } else {
            vec!["--no-auth-cache".to_string()]
        }
    }

    /// Create the auth params from the configuration file.
    fn create_auth_from_config(&mut self) -> bool {
        if !self.config.borrow().has("http-basic") {
            self.has_auth = Some(false);
            return false;
        }

        let auth_config = self.config.borrow_mut().get("http-basic");

        let host = parse_url(&self.url).and_then(|parsed| parsed.host);
        let host_str = host.as_deref().unwrap_or("");
        let auth_for_host = auth_config
            .as_array()
            .and_then(|m| m.get(host_str))
            .cloned();
        if let Some(entry) = auth_for_host
            && let Some(entry_arr) = entry.as_array()
        {
            self.credentials = Some(SvnCredentials {
                username: entry_arr
                    .get("username")
                    .and_then(|v| v.as_string())
                    .unwrap_or("")
                    .to_string(),
                password: entry_arr
                    .get("password")
                    .and_then(|v| v.as_string())
                    .unwrap_or("")
                    .to_string(),
            });

            self.has_auth = Some(true);
            return true;
        }

        self.has_auth = Some(false);
        false
    }

    /// Create the auth params from the url
    fn create_auth_from_url(&mut self) -> bool {
        let Some(uri) = parse_url(&self.url) else {
            self.has_auth = Some(false);
            return false;
        };
        let Some(user) = uri.user.filter(|user| !user.is_empty() && user != "0") else {
            self.has_auth = Some(false);
            return false;
        };

        self.credentials = Some(SvnCredentials {
            username: user,
            password: uri
                .pass
                .filter(|pass| !pass.is_empty() && pass != "0")
                .unwrap_or_default(),
        });

        self.has_auth = Some(true);
        true
    }

    /// Returns the version of the svn binary contained in PATH
    pub fn binary_version(&mut self) -> Option<String> {
        let mut cached = VERSION.lock().unwrap();
        if cached.is_none() {
            let mut output = String::new();
            if 0 == self.process.borrow_mut().execute_args(
                &["svn".to_string(), "--version".to_string()],
                &mut output,
                None,
            ) && let Some(matches) = Preg::is_match3(php_regex!(r"{(\d+(?:\.\d+)+)}"), &output)
            {
                *cached = Some(matches.get(1).unwrap_or_default().to_string());
            }
        }

        cached.clone()
    }
}
