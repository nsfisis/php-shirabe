//! ref: composer/src/Composer/Util/Hg.php

use crate::config::Config;
use crate::io::io_interface::IOInterface;
use crate::util::process_executor::ProcessExecutor;
use crate::util::url::Url;
use anyhow::Result;
use shirabe_external_packages::composer::pcre::preg::Preg;
use shirabe_php_shim::rawurlencode;
use std::sync::OnceLock;

static VERSION: OnceLock<Option<String>> = OnceLock::new();

#[derive(Debug)]
pub struct Hg {
    io: Box<dyn IOInterface>,
    config: Config,
    process: ProcessExecutor,
}

impl Hg {
    pub fn new(io: &dyn IOInterface, config: &Config, process: &ProcessExecutor) -> Self {
        todo!()
    }

    pub fn run_command(
        &self,
        command_callable: impl Fn(String) -> Vec<String>,
        url: String,
        cwd: Option<String>,
    ) -> Result<()> {
        self.config.prohibit_url_by_config(&url, &*self.io)?;

        // Try as is
        let command = command_callable(url.clone());
        let mut ignored_output = String::new();
        if self
            .process
            .execute(&command, &mut ignored_output, cwd.clone())
            == 0
        {
            return Ok(());
        }

        // Try with the authentication information available
        let matches = Preg::is_match_with_captures(
            r"(?i)^(?P<proto>ssh|https?)://(?:(?P<user>[^:@]+)(?::(?P<pass>[^:@]+))?@)?(?P<host>[^/]+)(?P<path>/.*)?",
            &url,
        )?;

        if let Some(matches) = matches {
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
                        rawurlencode(auth.get("username").map(|s| s.as_str()).unwrap_or("")),
                        rawurlencode(auth.get("password").map(|s| s.as_str()).unwrap_or("")),
                        matches.get("host").unwrap_or(&String::new()),
                        matches.get("path").unwrap_or(&String::new()),
                    )
                };

                let command = command_callable(authenticated_url);
                let mut ignored_output = String::new();
                if self.process.execute(&command, &mut ignored_output, cwd) == 0 {
                    return Ok(());
                }

                let error = self.process.get_error_output();
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
                Url::sanitize(&format!(
                    "Failed to clone {}, hg was not found, check that it is installed and in your PATH env.\n\n{}",
                    url,
                    self.process.get_error_output()
                ))
            );
        }

        anyhow::bail!("{}", Url::sanitize(message));
    }

    pub fn get_version(process: &ProcessExecutor) -> Option<&'static str> {
        VERSION
            .get_or_init(|| {
                let mut output = String::new();
                if process.execute(
                    &["hg".to_string(), "--version".to_string()],
                    &mut output,
                    None,
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
