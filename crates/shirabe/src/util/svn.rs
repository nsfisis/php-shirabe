//! ref: composer/src/Composer/Util/Svn.php

use crate::config::Config;
use crate::io::{IOInterface, IOInterfaceImmutable, io_interface};
use crate::util::{Platform, ProcessExecutor};
use bstr::{B, BStr, BString, ByteVec};
use shirabe_php_shim::{
    LogicException, PhpMixed, RuntimeException, implode, legacy_lossy, parse_url, path_to_bytes,
    php_regex, preg_match, stripos, strpos, trim,
};
use std::path::Path;
use std::sync::Mutex;

#[derive(Debug, Clone)]
pub struct SvnCredentials {
    pub username: BString,
    pub password: BString,
}

#[derive(Debug)]
pub struct Svn {
    credentials: Option<SvnCredentials>,
    has_auth: Option<bool>,
    io: std::rc::Rc<std::cell::RefCell<dyn IOInterface>>,
    url: BString,
    cache_credentials: bool,
    process: std::rc::Rc<std::cell::RefCell<ProcessExecutor>>,
    qty_auth_tries: i64,
    config: std::rc::Rc<std::cell::RefCell<Config>>,
}

static VERSION: Mutex<Option<BString>> = Mutex::new(None);

impl Svn {
    const MAX_QTY_AUTH_TRIES: i64 = 5;

    pub fn new(
        url: BString,
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
    pub fn execute(
        &mut self,
        command: Vec<BString>,
        url: &BStr,
        cwd: Option<&Path>,
        path: Option<&Path>,
        verbose: bool,
    ) -> anyhow::Result<BString> {
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
    pub fn execute_local(
        &mut self,
        command: Vec<BString>,
        path: &Path,
        cwd: Option<&Path>,
        verbose: bool,
    ) -> anyhow::Result<BString> {
        // A local command has no remote url
        self.execute_with_auth_retry(command, cwd, BStr::new(""), Some(path), verbose)
            .map(|o| o.unwrap_or_default())
    }

    fn execute_with_auth_retry(
        &mut self,
        svn_command: Vec<BString>,
        cwd: Option<&Path>,
        url: &BStr,
        path: Option<&Path>,
        verbose: bool,
    ) -> anyhow::Result<Option<BString>> {
        // Regenerate the command at each try, to use the newly user-provided credentials
        let command = self.get_command(svn_command.clone(), url, path);

        let output: std::rc::Rc<std::cell::RefCell<Option<BString>>> =
            std::rc::Rc::new(std::cell::RefCell::new(None));
        let io = self.io.clone();
        let output_for_handler = output.clone();
        let handler: Box<dyn FnMut(&str, &bstr::BStr) -> anyhow::Result<bool>> =
            Box::new(move |r#type: &str, buffer: &bstr::BStr| {
                if r#type != "out" {
                    return Ok(false);
                }
                if strpos(buffer, "Redirecting to URL ") == Some(0) {
                    return Ok(false);
                }
                output_for_handler
                    .borrow_mut()
                    .get_or_insert_with(BString::default)
                    .push_str(buffer);
                if verbose {
                    io.borrow().write_error2(&legacy_lossy(buffer), false);
                }
                Ok(false)
            });
        let status = self
            .process
            .borrow_mut()
            .execute(command.as_slice(), handler, cwd)?;
        let output = output.borrow().clone();
        if 0 == status {
            return Ok(output);
        }

        let error_output = self.process.borrow().get_error_output();
        let full_output = trim(implode("\n", &[output.unwrap_or_default(), error_output]));

        // the error is not auth-related
        if stripos(&full_output, "Could not authenticate to server:").is_none()
            && stripos(&full_output, "authorization failed").is_none()
            && stripos(&full_output, "svn: E170001:").is_none()
            && stripos(&full_output, "svn: E215004:").is_none()
        {
            return Err(RuntimeException::new(full_output.display_lossy().to_string()).into());
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

        Err(RuntimeException::new(format!(
            "wrong credentials provided ({})",
            full_output.display_lossy()
        ))
        .into())
    }

    pub fn set_cache_credentials(&mut self, cache_credentials: bool) {
        self.cache_credentials = cache_credentials;
    }

    /// Repositories requests credentials, let's put them in.
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
                self.url.display_lossy(),
            ),
            true,
            io_interface::NORMAL,
        );

        self.has_auth = Some(true);
        self.credentials = Some(SvnCredentials {
            username: self
                .io
                .ask("Username: ".into(), PhpMixed::string(""))?
                .as_string()
                .unwrap_or_default()
                .to_owned(),
            password: self
                .io
                .ask_and_hide_answer("Password: ".to_string())?
                .unwrap_or_default()
                .into(),
        });

        self.cache_credentials = self.io.ask_confirmation(
            "Should Subversion cache these credentials? (yes/no) ".to_string(),
            true,
        )?;

        Ok(self)
    }

    /// A method to create the svn commands run.
    fn get_command(
        &mut self,
        mut cmd: Vec<BString>,
        url: &BStr,
        path: Option<&Path>,
    ) -> Vec<BString> {
        cmd.push("--non-interactive".into());
        cmd.extend(self.get_credential_args());
        cmd.push("--".into());
        cmd.push(url.to_owned());

        if let Some(path) = path {
            cmd.push(path_to_bytes(path).into());
        }

        cmd
    }

    /// Return the credential string for the svn command.
    ///
    /// Adds --no-auth-cache when credentials are present.
    fn get_credential_args(&mut self) -> Vec<BString> {
        if !self.has_auth() {
            return vec![];
        }

        let mut args = self.get_auth_cache_args();
        args.push("--username".into());
        args.push(self.get_username().unwrap());
        args.push("--password".into());
        args.push(self.get_password().unwrap());
        args
    }

    /// For testing only: invoke the crate-private `get_command`.
    pub fn __get_command(
        &mut self,
        cmd: Vec<BString>,
        url: &BStr,
        path: Option<&Path>,
    ) -> Vec<BString> {
        self.get_command(cmd, url, path)
    }

    /// For testing only: invoke the crate-private `get_credential_args`.
    pub fn __get_credential_args(&mut self) -> Vec<BString> {
        self.get_credential_args()
    }

    /// Get the password for the svn command. Can be empty.
    fn get_password(&self) -> anyhow::Result<BString> {
        if self.credentials.is_none() {
            return Err(LogicException::new("No svn auth detected.".to_string()).into());
        }

        Ok(self.credentials.as_ref().unwrap().password.clone())
    }

    /// Get the username for the svn command.
    fn get_username(&self) -> anyhow::Result<BString> {
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
    fn get_auth_cache_args(&self) -> Vec<BString> {
        if self.cache_credentials {
            vec![]
        } else {
            vec!["--no-auth-cache".into()]
        }
    }

    /// Create the auth params from the configuration file.
    fn create_auth_from_config(&mut self) -> bool {
        if !self.config.borrow().has("http-basic") {
            self.has_auth = Some(false);
            return false;
        }

        let auth_config = self.config.borrow_mut().get("http-basic");

        let host = parse_url(&legacy_lossy(&self.url)).and_then(|parsed| parsed.host);
        let host_str = host.as_deref().unwrap_or_default();
        let auth_for_host = auth_config
            .as_array()
            .and_then(|m| m.get(B(host_str)))
            .cloned();
        if let Some(entry) = auth_for_host
            && let Some(entry_arr) = entry.as_array()
        {
            self.credentials = Some(SvnCredentials {
                username: entry_arr
                    .get(B("username"))
                    .and_then(|v| v.as_string())
                    .unwrap_or_default()
                    .to_owned(),
                password: entry_arr
                    .get(B("password"))
                    .and_then(|v| v.as_string())
                    .unwrap_or_default()
                    .to_owned(),
            });

            self.has_auth = Some(true);
            return true;
        }

        self.has_auth = Some(false);
        false
    }

    /// Create the auth params from the url
    fn create_auth_from_url(&mut self) -> bool {
        let Some(uri) = parse_url(&legacy_lossy(&self.url)) else {
            self.has_auth = Some(false);
            return false;
        };
        let Some(user) = uri.user.filter(|user| !user.is_empty() && user != "0") else {
            self.has_auth = Some(false);
            return false;
        };

        self.credentials = Some(SvnCredentials {
            username: user.into(),
            password: uri
                .pass
                .filter(|pass| !pass.is_empty() && pass != "0")
                .unwrap_or_default()
                .into(),
        });

        self.has_auth = Some(true);
        true
    }

    /// Returns the version of the svn binary contained in PATH
    pub fn binary_version(&mut self) -> anyhow::Result<Option<BString>> {
        let mut cached = VERSION.lock().unwrap();
        if cached.is_none() {
            let mut output = BString::default();
            if 0 == self.process.borrow_mut().execute(
                &[BString::from("svn"), "--version".into()],
                &mut output,
                None,
            )? && let Some(matches) = preg_match(php_regex!(r"{(\d+(?:\.\d+)+)}"), &output)
            {
                *cached = Some(matches.get(1).unwrap_or_default().to_owned());
            }
        }

        Ok(cached.clone())
    }
}
