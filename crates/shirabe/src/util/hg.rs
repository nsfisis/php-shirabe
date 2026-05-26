//! ref: composer/src/Composer/Util/Hg.php

use crate::config::Config;
use crate::io::IOInterface;
use crate::io::IOInterfaceImmutable;
use crate::util::ProcessExecutor;
use crate::util::Url;
use anyhow::Result;
use shirabe_external_packages::composer::pcre::Preg;
use shirabe_php_shim::rawurlencode;
use std::sync::OnceLock;

static VERSION: OnceLock<Option<String>> = OnceLock::new();

#[derive(Debug)]
pub struct Hg {
    io: std::rc::Rc<std::cell::RefCell<dyn IOInterface>>,
    config: std::rc::Rc<std::cell::RefCell<Config>>,
    process: std::rc::Rc<std::cell::RefCell<ProcessExecutor>>,
}

impl Hg {
    pub fn new(
        io: std::rc::Rc<std::cell::RefCell<dyn IOInterface>>,
        config: &Config,
        process: &std::rc::Rc<std::cell::RefCell<ProcessExecutor>>,
    ) -> Self {
        todo!()
    }

    pub fn run_command(
        &self,
        command_callable: impl Fn(String) -> Vec<String>,
        url: String,
        cwd: Option<String>,
    ) -> Result<()> {
        self.config.borrow_mut().prohibit_url_by_config(
            &url,
            Some(&*self.io.borrow()),
            &indexmap::IndexMap::new(),
        )?;

        // Try as is
        let command = command_callable(url.clone());
        let mut ignored_output = String::new();
        if self
            .process
            .borrow_mut()
            .execute_args(&command, &mut ignored_output, cwd.clone())
            == 0
        {
            return Ok(());
        }

        // Try with the authentication information available
        let mut matches: indexmap::IndexMap<String, String> = indexmap::IndexMap::new();
        let matched = Preg::is_match_named(
            r"(?i)^(?P<proto>ssh|https?)://(?:(?P<user>[^:@]+)(?::(?P<pass>[^:@]+))?@)?(?P<host>[^/]+)(?P<path>/.*)?",
            &url,
            &mut matches,
        )?;

        if matched {
            if self
                .io
                .has_authentication(matches.get("host").map(|s| s.as_str()).unwrap_or(""))
            {
                let authenticated_url = if matches.get("proto").map(|s| s.as_str()) == Some("ssh") {
                    let user = if let Some(u) = matches.get("user") {
                        format!("{}@", rawurlencode(u))
                    } else {
                        String::new()
                    };
                    format!(
                        "{}://{}{}{}",
                        matches.get("proto").unwrap_or(&String::new()),
                        user,
                        matches.get("host").unwrap_or(&String::new()),
                        matches.get("path").unwrap_or(&String::new()),
                    )
                } else {
                    let auth = self
                        .io
                        .get_authentication(matches.get("host").map(|s| s.as_str()).unwrap_or(""));
                    format!(
                        "{}://{}:{}@{}{}",
                        matches.get("proto").unwrap_or(&String::new()),
                        rawurlencode(
                            auth.get("username")
                                .and_then(|s| s.as_deref())
                                .unwrap_or("")
                        ),
                        rawurlencode(
                            auth.get("password")
                                .and_then(|s| s.as_deref())
                                .unwrap_or("")
                        ),
                        matches.get("host").unwrap_or(&String::new()),
                        matches.get("path").unwrap_or(&String::new()),
                    )
                };

                let command = command_callable(authenticated_url);
                let mut ignored_output = String::new();
                if self
                    .process
                    .borrow_mut()
                    .execute_args(&command, &mut ignored_output, cwd)
                    == 0
                {
                    return Ok(());
                }

                let error = self.process.borrow().get_error_output().to_string();
                return self
                    .throw_exception(&format!("Failed to clone {}, \n\n{}", url, error), &url);
            }
        }

        let error = format!(
            "The given URL ({}) does not match the required format (ssh|http(s)://(username:password@)example.com/path-to-repository)",
            url
        );
        self.throw_exception(&format!("Failed to clone {}, \n\n{}", url, error), &url)
    }

    fn throw_exception(&self, message: &str, url: &str) -> Result<()> {
        if Self::get_version(&self.process).is_none() {
            anyhow::bail!(
                "{}",
                Url::sanitize(format!(
                    "Failed to clone {}, hg was not found, check that it is installed and in your PATH env.\n\n{}",
                    url,
                    self.process.borrow().get_error_output()
                ))
            );
        }

        anyhow::bail!("{}", Url::sanitize(message.to_string()));
    }

    pub fn get_version(
        process: &std::rc::Rc<std::cell::RefCell<ProcessExecutor>>,
    ) -> Option<&'static str> {
        VERSION
            .get_or_init(|| {
                let mut output = String::new();
                if process.borrow_mut().execute_args(
                    &["hg".to_string(), "--version".to_string()],
                    &mut output,
                    (),
                ) == 0
                {
                    if let Ok(Some(matches)) = Preg::is_match_with_indexed_captures(
                        r"/^.+? (\d+(?:\.\d+)+)(?:\+.*?)?\)?\r?\n/",
                        &output,
                    ) {
                        return matches.into_iter().nth(1);
                    }
                }
                None
            })
            .as_deref()
    }
}
