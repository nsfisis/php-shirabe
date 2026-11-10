//! ref: composer/src/Composer/Util/Hg.php sha1:63bfeb84d0a28100dec5607d0bf67fbee4f3ed34

use crate::config::Config;
use crate::io::{IOInterface, IOInterfaceImmutable};
use crate::util::{ProcessExecutor, Url};
use bstr::{BStr, BString, ByteSlice as _};
use shirabe_php_shim::{RuntimeException, bformat, php_regex, preg_match, rawurlencode};

static VERSION: std::sync::Mutex<Option<Option<BString>>> = std::sync::Mutex::new(None);

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
        cwd: Option<&std::path::Path>,
    ) -> anyhow::Result<()> {
        self.config.borrow_mut().prohibit_url_by_config(
            url,
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

        if let Some(matches) = matched
            && self
                .io
                .has_authentication(matches.name("host").unwrap_or_default())
        {
            let authenticated_url = if matches.name("proto") == Some(BStr::new("ssh")) {
                let user = if let Some(u) = matches.name("user") {
                    bformat!("{}@", rawurlencode(u))
                } else {
                    BString::default()
                };
                bformat!(
                    "{}://{user}{}{}",
                    matches.name("proto").unwrap_or_default(),
                    matches.name("host").unwrap_or_default(),
                    matches.name("path").unwrap_or_default()
                )
            } else {
                let auth = self
                    .io
                    .get_authentication(matches.name("host").unwrap_or_default());
                bformat!(
                    "{}://{}:{}@{}{}",
                    matches.name("proto").unwrap_or_default(),
                    rawurlencode(
                        auth.get(bstr::B("username"))
                            .and_then(|s| s.as_ref().map(|s| s.as_slice()))
                            .unwrap_or_default()
                    ),
                    rawurlencode(
                        auth.get(bstr::B("password"))
                            .and_then(|s| s.as_ref().map(|s| s.as_slice()))
                            .unwrap_or_default()
                    ),
                    matches.name("host").unwrap_or_default(),
                    matches.name("path").unwrap_or_default()
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
                bformat!("Failed to clone {url}, \n\n{error}").as_bstr(),
                url,
            );
        }

        let error = bformat!(
            "The given URL ({url}) does not match the required format (ssh|http(s)://(username:password@)example.com/path-to-repository)"
        );
        self.throw_exception(
            bformat!("Failed to clone {url}, \n\n{error}").as_bstr(),
            url,
        )
    }

    fn throw_exception(&self, message: &BStr, url: &BStr) -> anyhow::Result<()> {
        if Self::get_version(&self.process)?.is_none() {
            return Err(RuntimeException::new(Url::sanitize(bformat!("Failed to clone {url}, hg was not found, check that it is installed and in your PATH env.\n\n{}", self.process.borrow().get_error_output())))
            .into());
        }

        Err(RuntimeException::new(Url::sanitize(message)).into())
    }

    pub fn get_version(
        process: &std::rc::Rc<std::cell::RefCell<ProcessExecutor>>,
    ) -> anyhow::Result<Option<BString>> {
        let mut version = VERSION.lock().unwrap();
        if version.is_none() {
            *version = Some(None);
            let mut output = BString::default();
            if process.borrow_mut().execute(
                &[BString::from("hg"), BString::from("--version")],
                &mut output,
                None,
            )? == 0
                && let Some(matches) = preg_match(
                    php_regex!(r"/^.+? (\d+(?:\.\d+)+)(?:\+.*?)?\)?\r?\n/"),
                    &output,
                )
            {
                *version = Some(matches.get(1).map(BString::from));
            }
        }

        Ok(version.clone().unwrap_or(None))
    }
}
