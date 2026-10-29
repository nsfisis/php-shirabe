//! ref: composer/src/Composer/Util/Hg.php

use crate::config::Config;
use crate::io::{IOInterface, IOInterfaceImmutable};
use crate::util::{ProcessExecutor, Url};
use bstr::{BStr, BString, ByteSlice};
use shirabe_php_shim::{bconcat, legacy_lossy, php_regex, preg_match, rawurlencode};
use std::path::Path;
use std::sync::Mutex;

static VERSION: Mutex<Option<Option<String>>> = Mutex::new(None);

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
        command_callable: impl Fn(BString) -> Vec<BString>,
        url: &BStr,
        cwd: Option<&Path>,
    ) -> anyhow::Result<()> {
        // TODO(bytes): Config::prohibit_url_by_config() takes a &str.
        self.config.borrow_mut().prohibit_url_by_config(
            &legacy_lossy(url),
            Some(self.io.clone()),
            &indexmap::IndexMap::new(),
        )?;

        // Try as is
        let command = command_callable(url.to_owned());
        let mut ignored_output = BString::default();
        if self
            .process
            .borrow_mut()
            .execute(&command, &mut ignored_output, cwd)?
            == 0
        {
            return Ok(());
        }

        // Try with the authentication information available
        let matched = preg_match(
            php_regex!(
                r"{^(?P<proto>ssh|https?)://(?:(?P<user>[^:@]+)(?::(?P<pass>[^:@]+))?@)?(?P<host>[^/]+)(?P<path>/.*)?}mi"
            ),
            url,
        );

        // TODO(bytes): IOInterface authentications are keyed and valued by Strings.
        if let Some(matches) = matched
            && self
                .io
                .has_authentication(&legacy_lossy(matches.name("host").unwrap_or_default()))
        {
            let authenticated_url = if matches.name("proto") == Some(BStr::new("ssh")) {
                let user = if let Some(u) = matches.name("user") {
                    bconcat!(rawurlencode(u), "@")
                } else {
                    BString::default()
                };
                bconcat!(
                    matches.name("proto").unwrap_or_default(),
                    "://",
                    user,
                    matches.name("host").unwrap_or_default(),
                    matches.name("path").unwrap_or_default(),
                )
            } else {
                let auth = self
                    .io
                    .get_authentication(&legacy_lossy(matches.name("host").unwrap_or_default()));
                bconcat!(
                    matches.name("proto").unwrap_or_default(),
                    "://",
                    rawurlencode(
                        auth.get("username")
                            .and_then(|s| s.as_deref())
                            .unwrap_or_default()
                    ),
                    ":",
                    rawurlencode(
                        auth.get("password")
                            .and_then(|s| s.as_deref())
                            .unwrap_or_default()
                    ),
                    "@",
                    matches.name("host").unwrap_or_default(),
                    matches.name("path").unwrap_or_default(),
                )
            };

            let command = command_callable(authenticated_url);
            let mut ignored_output = BString::default();
            if self
                .process
                .borrow_mut()
                .execute(&command, &mut ignored_output, cwd)?
                == 0
            {
                return Ok(());
            }

            let error = self.process.borrow().get_error_output();
            return self.throw_exception(
                bconcat!("Failed to clone ", url, ", \n\n", error).as_bstr(),
                url,
            );
        }

        let error = bconcat!(
            "The given URL (",
            url,
            ") does not match the required format (ssh|http(s)://(username:password@)example.com/path-to-repository)"
        );
        self.throw_exception(
            bconcat!("Failed to clone ", url, ", \n\n", error).as_bstr(),
            url,
        )
    }

    fn throw_exception(&self, message: &BStr, url: &BStr) -> anyhow::Result<()> {
        if Self::get_version(&self.process)?.is_none() {
            anyhow::bail!(
                "{}",
                Url::sanitize(bconcat!(
                    "Failed to clone ",
                    url,
                    ", hg was not found, check that it is installed and in your PATH env.\n\n",
                    self.process.borrow().get_error_output()
                ))
                .display_lossy()
            );
        }

        anyhow::bail!("{}", Url::sanitize(message).display_lossy());
    }

    pub fn get_version(
        process: &std::rc::Rc<std::cell::RefCell<ProcessExecutor>>,
    ) -> anyhow::Result<Option<String>> {
        let mut version = VERSION.lock().unwrap();
        if version.is_none() {
            *version = Some(None);
            let mut output = String::new();
            if process.borrow_mut().execute(
                &["hg".to_string(), "--version".to_string()],
                &mut output,
                None,
            )? == 0
                && let Some(matches) = preg_match(
                    php_regex!(r"/^.+? (\d+(?:\.\d+)+)(?:\+.*?)?\)?\r?\n/"),
                    &output,
                )
            {
                *version = Some(matches.get(1).map(|s| legacy_lossy(s).into_owned()));
            }
        }

        Ok(version.clone().unwrap_or(None))
    }
}
