//! ref: composer/src/Composer/Util/Hg.php

use crate::config::Config;
use crate::io::IOInterface;
use crate::io::IOInterfaceImmutable;
use crate::util::ProcessExecutor;
use crate::util::Url;
use shirabe_pcre::{CaptureKey, Preg};
use shirabe_php_shim::{php_regex, rawurlencode};
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
        config: std::rc::Rc<std::cell::RefCell<Config>>,
        process: std::rc::Rc<std::cell::RefCell<ProcessExecutor>>,
    ) -> Self {
        Self {
            io,
            config,
            process,
        }
    }

    pub fn run_command(
        &self,
        command_callable: impl Fn(String) -> Vec<String>,
        url: String,
        cwd: Option<String>,
    ) -> anyhow::Result<()> {
        self.config.borrow_mut().prohibit_url_by_config(
            &url,
            Some(self.io.clone()),
            &indexmap::IndexMap::new(),
        )?;

        // Try as is
        let command = command_callable(url.clone());
        let mut ignored_output = String::new();
        if self
            .process
            .borrow_mut()
            .execute_args(&command, &mut ignored_output, cwd.as_deref())
            == 0
        {
            return Ok(());
        }

        // Try with the authentication information available
        let matched = Preg::is_match3(
            php_regex!(
                r"{^(?P<proto>ssh|https?)://(?:(?P<user>[^:@]+)(?::(?P<pass>[^:@]+))?@)?(?P<host>[^/]+)(?P<path>/.*)?}mi"
            ),
            &url,
        );

        if let Some(matches) = matched
            && self.io.has_authentication(
                matches
                    .get(&CaptureKey::ByName("host".to_string()))
                    .unwrap_or(""),
            )
        {
            let authenticated_url = if matches.get(&CaptureKey::ByName("proto".to_string()))
                == Some("ssh")
            {
                let user = if let Some(u) = matches.get(&CaptureKey::ByName("user".to_string())) {
                    format!("{}@", rawurlencode(u))
                } else {
                    String::new()
                };
                format!(
                    "{}://{}{}{}",
                    matches
                        .get(&CaptureKey::ByName("proto".to_string()))
                        .unwrap_or(""),
                    user,
                    matches
                        .get(&CaptureKey::ByName("host".to_string()))
                        .unwrap_or(""),
                    matches
                        .get(&CaptureKey::ByName("path".to_string()))
                        .unwrap_or(""),
                )
            } else {
                let auth = self.io.get_authentication(
                    matches
                        .get(&CaptureKey::ByName("host".to_string()))
                        .unwrap_or(""),
                );
                format!(
                    "{}://{}:{}@{}{}",
                    matches
                        .get(&CaptureKey::ByName("proto".to_string()))
                        .unwrap_or(""),
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
                    matches
                        .get(&CaptureKey::ByName("host".to_string()))
                        .unwrap_or(""),
                    matches
                        .get(&CaptureKey::ByName("path".to_string()))
                        .unwrap_or(""),
                )
            };

            let command = command_callable(authenticated_url);
            let mut ignored_output = String::new();
            if self
                .process
                .borrow_mut()
                .execute_args(&command, &mut ignored_output, cwd.as_deref())
                == 0
            {
                return Ok(());
            }

            let error = self.process.borrow().get_error_output().to_string();
            return self.throw_exception(&format!("Failed to clone {}, \n\n{}", url, error), &url);
        }

        let error = format!(
            "The given URL ({}) does not match the required format (ssh|http(s)://(username:password@)example.com/path-to-repository)",
            url
        );
        self.throw_exception(&format!("Failed to clone {}, \n\n{}", url, error), &url)
    }

    fn throw_exception(&self, message: &str, url: &str) -> anyhow::Result<()> {
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
                    None,
                ) == 0
                    && let Some(matches) = Preg::is_match3(
                        php_regex!(r"/^.+? (\d+(?:\.\d+)+)(?:\+.*?)?\)?\r?\n/"),
                        &output,
                    )
                {
                    return matches.get(&CaptureKey::ByIndex(1)).map(str::to_string);
                }
                None
            })
            .as_deref()
    }
}
