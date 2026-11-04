//! ref: composer/src/Composer/Command/DiagnoseCommand.php sha1:fc82ca5045451163f82c40d1acbc3a9c1feb0a38

use crate::advisory::Auditor;
use crate::command::base_command::base_command_initialize;
use crate::command::{BaseCommand, BaseCommandData};
use crate::composer;
use crate::config::Config;
use crate::downloader::TransportException;
use crate::event_dispatcher::EventFlags;
use crate::factory::Factory;
use crate::io::{BufferIO, IOInterface, IOInterfaceWrite, NullIO};
use crate::json::{JsonFile, JsonValidationException};
use crate::package::version::VersionParser;
use crate::package::{LockerInterface, RootPackage};
use crate::plugin::{CommandEvent, PluginEvents};
use crate::repository::{
    ComposerRepository, FilesystemRepository, PlatformRepository, RepositorySet,
};
use crate::self_update::{Keys, Versions};
use crate::util::http::{ProxyManager, RequestProxy};
use crate::util::{ConfigValidator, Git, HttpDownloader, IniHelper, Platform, ProcessExecutor};
use bstr::{B, BStr, BString, ByteSlice, ByteVec};
use indexmap::IndexMap;
use shirabe_php_shim::{
    AnyThrowable, Catch as _, CmpOp, Exception, InvalidArgumentException, PHP_EOL, PhpClass as _,
    PhpMixed, bconcat, disk_free_space, error_message, file_exists, filter_var_boolean, hash,
    impl_php_class, implode, path_from_bytes, php_regex, preg_match, rtrim2, str_replace, strpos,
    strstr, strstr3, strtolower, trim2, version_compare,
};
use shirabe_symfony_console::command::Command;
use shirabe_symfony_console::input::InputInterface;
use shirabe_symfony_console::output::OutputInterface;
use shirabe_symfony_process::ExecutableFinder;

/// What a `check_*` method hands to `output_result`.
#[derive(Debug)]
enum CheckResult {
    Ok,
    /// A falsey PHP result: counted as an error, with no message of its own.
    Failed,
    Message(BString),
    Messages(Vec<BString>),
    Exception(anyhow::Error),
}

/// What `get_github_rate_limit` returns: PHP yields either the `resources.core` entry of
/// GitHub's response, or a non-array value that goes straight to `output_result`.
#[derive(Debug)]
enum GithubRateLimit {
    Unavailable(CheckResult),
    Core { limit: i64, remaining: i64 },
}

#[derive(Debug)]
pub struct DiagnoseCommand {
    base_command_data: BaseCommandData,

    http_downloader: std::cell::RefCell<Option<std::rc::Rc<std::cell::RefCell<HttpDownloader>>>>,
    process: std::cell::RefCell<Option<std::rc::Rc<std::cell::RefCell<ProcessExecutor>>>>,
    exit_code: std::cell::Cell<i64>,
}

impl_php_class!(DiagnoseCommand, r"Composer\Command\DiagnoseCommand");

impl Default for DiagnoseCommand {
    fn default() -> Self {
        Self::new()
    }
}

impl DiagnoseCommand {
    pub fn new() -> Self {
        let command = DiagnoseCommand {
            base_command_data: BaseCommandData::new(None),
            http_downloader: std::cell::RefCell::new(None),
            process: std::cell::RefCell::new(None),
            exit_code: std::cell::Cell::new(0),
        };
        command
            .configure()
            .expect("DiagnoseCommand::configure uses static, valid metadata");
        command
    }

    fn check_composer_schema(&self) -> anyhow::Result<CheckResult> {
        let validator = ConfigValidator::new(self.get_io().clone());
        let (errors, _, warnings) = validator.validate(&Factory::get_composer_file()?, 0, 0);

        if !errors.is_empty() || !warnings.is_empty() {
            let mut messages: IndexMap<BString, Vec<BString>> = IndexMap::new();
            messages.insert(BString::from("error"), errors);
            messages.insert(BString::from("warning"), warnings);

            let mut output = BString::default();
            for (style, msgs) in &messages {
                for msg in msgs {
                    output.push_str(&bconcat!("<", style, ">", msg, "</", style, ">", PHP_EOL));
                }
            }

            return Ok(CheckResult::Message(
                rtrim2(&output, " \t\n\r\0\u{0B}").to_owned(),
            ));
        }

        Ok(CheckResult::Ok)
    }

    fn check_composer_lock_schema(
        &self,
        locker: &dyn LockerInterface,
    ) -> anyhow::Result<CheckResult> {
        let json = locker.get_json_file();

        match json.validate_schema(JsonFile::LOCK_SCHEMA, None) {
            Ok(_) => {}
            Err(e) => {
                if let Some(jve) = e.catch::<JsonValidationException>() {
                    let mut output = BString::default();
                    for error in jve.get_errors() {
                        output.push_str(&bconcat!("<error>", error, "</error>", PHP_EOL));
                    }

                    return Ok(CheckResult::Message(
                        trim2(&output, " \t\n\r\0\u{0B}").to_owned(),
                    ));
                }
                return Err(e);
            }
        }

        Ok(CheckResult::Ok)
    }

    fn check_git(&self) -> anyhow::Result<BString> {
        if !shirabe_php_rpc::get_diagnostics()?.function_exists("proc_open") {
            return Ok(BString::from(
                "<comment>proc_open is not available, git cannot be used</comment>",
            ));
        }

        let mut output = BString::default();
        let _ = self
            .process
            .borrow()
            .as_ref()
            .unwrap()
            .borrow_mut()
            .execute(
                vec![
                    BString::from("git"),
                    BString::from("config"),
                    BString::from("color.ui"),
                ],
                &mut output,
                None,
            );
        if strtolower(trim2(&output, " \t\n\r\0\u{0B}")) == "always" {
            return Ok(BString::from(
                "<comment>Your git color.ui setting is set to always, this is known to create issues. Use \"git config --global color.ui true\" to set it correctly.</comment>",
            ));
        }

        let process = self.process.borrow();
        let git_version = Git::get_version(process.as_ref().unwrap())?;
        let git_version = match git_version {
            Some(v) => v,
            None => return Ok(BString::from("<comment>No git process found</>")),
        };

        if version_compare("2.24.0", &git_version, CmpOp::Gt) {
            return Ok(bconcat!(
                "<warning>Your git version (",
                git_version,
                ") is too old and possibly will cause issues. Please upgrade to git 2.24 or above</>"
            ));
        }

        Ok(bconcat!(
            "<info>OK</> <comment>git version ",
            git_version,
            "</>"
        ))
    }

    fn check_http(
        &self,
        proto: &BStr,
        config: &std::rc::Rc<std::cell::RefCell<Config>>,
    ) -> anyhow::Result<CheckResult> {
        let result = self.check_connectivity_and_composer_network_http_enablement()?;
        if !matches!(result, CheckResult::Ok) {
            return Ok(result);
        }

        let mut result_list: Vec<BString> = vec![];
        let mut tls_warning: Option<BString> = None;
        if proto == "https" && config.borrow().get_bool("disable-tls") {
            tls_warning = Some(BString::from(
                "<warning>Shirabe is configured to disable SSL/TLS protection. This will leave remote HTTPS requests vulnerable to Man-In-The-Middle attacks.</warning>",
            ));
        }

        match self
            .http_downloader
            .borrow()
            .as_ref()
            .unwrap()
            .borrow_mut()
            .get(
                bconcat!(proto, "://repo.packagist.org/packages.json"),
                IndexMap::new(),
            ) {
            Ok(_) => {}
            Err(e) => {
                if let Some(te) = e.catch::<TransportException>() {
                    let hints = HttpDownloader::get_exception_hints(&e).unwrap_or_default();
                    if !hints.is_empty() {
                        for hint in hints {
                            result_list.push(hint);
                        }
                    }

                    result_list.push(bconcat!(
                        "<error>[",
                        AnyThrowable::of(&e)
                            .expect("the catch above proved the error carries a \\Throwable")
                            .php_class_name(),
                        "] ",
                        te.get_message(),
                        "</error>"
                    ));
                } else {
                    return Err(e);
                }
            }
        }

        if let Some(w) = tls_warning {
            result_list.push(w);
        }

        if !result_list.is_empty() {
            return Ok(CheckResult::Messages(result_list));
        }

        Ok(CheckResult::Ok)
    }

    fn check_composer_repo(
        &self,
        url: &BStr,
        config: &std::rc::Rc<std::cell::RefCell<Config>>,
    ) -> anyhow::Result<CheckResult> {
        let result = self.check_connectivity_and_composer_network_http_enablement()?;
        if !matches!(result, CheckResult::Ok) {
            return Ok(result);
        }

        let mut result_list: Vec<BString> = vec![];
        let mut tls_warning: Option<BString> = None;
        if url.starts_with(b"https://") && config.borrow().get_bool("disable-tls") {
            tls_warning = Some(BString::from(
                "<warning>Shirabe is configured to disable SSL/TLS protection. This will leave remote HTTPS requests vulnerable to Man-In-The-Middle attacks.</warning>",
            ));
        }

        match self
            .http_downloader
            .borrow()
            .as_ref()
            .unwrap()
            .borrow_mut()
            .get(url, IndexMap::new())
        {
            Ok(_) => {}
            Err(e) => {
                if let Some(te) = e.catch::<TransportException>() {
                    let hints = HttpDownloader::get_exception_hints(&e).unwrap_or_default();
                    if !hints.is_empty() {
                        for hint in hints {
                            result_list.push(hint);
                        }
                    }

                    result_list.push(bconcat!(
                        "<error>[",
                        AnyThrowable::of(&e)
                            .expect("the catch above proved the error carries a \\Throwable")
                            .php_class_name(),
                        "] ",
                        te.get_message(),
                        "</error>"
                    ));
                } else {
                    return Err(e);
                }
            }
        }

        if let Some(w) = tls_warning {
            result_list.push(w);
        }

        if !result_list.is_empty() {
            return Ok(CheckResult::Messages(result_list));
        }

        Ok(CheckResult::Ok)
    }

    fn check_http_proxy(
        &self,
        proxy: &RequestProxy,
        protocol: &BStr,
    ) -> anyhow::Result<CheckResult> {
        let result = self.check_connectivity_and_composer_network_http_enablement()?;
        if !matches!(result, CheckResult::Ok) {
            return Ok(result);
        }

        let result = (|| -> anyhow::Result<CheckResult> {
            let proxy_status = proxy.get_status(None).unwrap_or_default();

            if proxy.is_excluded_by_no_proxy() {
                return Ok(CheckResult::Message(bconcat!(
                    "<info>SKIP</> <comment>Because repo.packagist.org is ",
                    proxy_status,
                    "</>"
                )));
            }

            let json = self
                .http_downloader
                .borrow()
                .as_ref()
                .unwrap()
                .borrow_mut()
                .get(
                    bconcat!(protocol, "://repo.packagist.org/packages.json"),
                    IndexMap::new(),
                )?
                .decode_json()?;
            if let Some(provider_includes) =
                json.as_array().and_then(|a| a.get(B("provider-includes")))
            {
                let provider_includes_arr =
                    provider_includes.as_array().cloned().unwrap_or_default();
                let first = provider_includes_arr
                    .values()
                    .next()
                    .cloned()
                    .unwrap_or(PhpMixed::Null);
                let hash_val = first
                    .as_array()
                    .and_then(|a| a.get(B("sha256")))
                    .cloned()
                    .unwrap_or(PhpMixed::Null);
                let path = str_replace(
                    "%hash%",
                    hash_val.as_string().unwrap_or_default(),
                    provider_includes
                        .as_array()
                        .and_then(|a| a.keys().next())
                        .map(|key| key.as_slice())
                        .unwrap_or_default(),
                );
                let response = self
                    .http_downloader
                    .borrow()
                    .as_ref()
                    .unwrap()
                    .borrow_mut()
                    .get(
                        bconcat!(protocol, "://repo.packagist.org/", path),
                        IndexMap::new(),
                    )?;
                let provider = response.get_body().unwrap_or_default().to_vec();

                if hash("sha256", &provider) != hash_val.as_string().unwrap_or_default() {
                    return Ok(CheckResult::Message(bconcat!(
                        "<warning>It seems that your proxy (",
                        proxy_status,
                        ") is modifying ",
                        protocol,
                        " traffic on the fly</>"
                    )));
                }
            }

            Ok(CheckResult::Message(bconcat!(
                "<info>OK</> <comment>",
                proxy_status,
                "</>"
            )))
        })();
        match result {
            Err(e) if e.is_instanceof::<Exception>() => Ok(CheckResult::Exception(e)),
            result => result,
        }
    }

    fn check_github_oauth(&self, domain: &BStr, token: &BStr) -> anyhow::Result<CheckResult> {
        let result = self.check_connectivity_and_composer_network_http_enablement()?;
        if !matches!(result, CheckResult::Ok) {
            return Ok(result);
        }

        self.get_io().borrow_mut().set_authentication(
            BString::from(domain),
            BString::from(token),
            Some(BString::from("x-oauth-basic")),
        );
        let url = if domain == "github.com" {
            bconcat!("https://api.", domain, "/")
        } else {
            bconcat!("https://", domain, "/api/v3/")
        };

        let mut opts: IndexMap<BString, PhpMixed> = IndexMap::new();
        opts.insert("retry-auth-failure".into(), PhpMixed::Bool(false));

        match self
            .http_downloader
            .borrow()
            .as_ref()
            .unwrap()
            .borrow_mut()
            .get(&url, opts)
        {
            Ok(response) => {
                let expiration =
                    response.get_header(BStr::new("github-authentication-token-expiration"));

                if expiration.is_none() {
                    return Ok(CheckResult::Message(BString::from(
                        "<info>OK</> <comment>does not expire</>",
                    )));
                }

                Ok(CheckResult::Message(bconcat!(
                    "<info>OK</> <comment>expires on ",
                    expiration.unwrap(),
                    "</>"
                )))
            }
            Err(e) => {
                if let Some(te) = e.catch::<TransportException>()
                    && te.get_code() == 401
                {
                    return Ok(CheckResult::Message(bconcat!(
                        "<comment>The oauth token for ",
                        domain,
                        " seems invalid, run \"shirabe config --global --unset github-oauth.",
                        domain,
                        "\" to remove it</comment>"
                    )));
                }
                Ok(CheckResult::Exception(e))
            }
        }
    }

    fn get_github_rate_limit(
        &self,
        domain: &BStr,
        token: Option<&BStr>,
    ) -> anyhow::Result<GithubRateLimit> {
        let result = self.check_connectivity_and_composer_network_http_enablement()?;
        if !matches!(result, CheckResult::Ok) {
            return Ok(GithubRateLimit::Unavailable(result));
        }

        if let Some(t) = token {
            self.get_io().borrow_mut().set_authentication(
                BString::from(domain),
                BString::from(t),
                Some(BString::from("x-oauth-basic")),
            );
        }

        let url = if domain == "github.com" {
            bconcat!("https://api.", domain, "/rate_limit")
        } else {
            bconcat!("https://", domain, "/api/rate_limit")
        };
        let mut opts: IndexMap<BString, PhpMixed> = IndexMap::new();
        opts.insert("retry-auth-failure".into(), PhpMixed::Bool(false));
        let data = self
            .http_downloader
            .borrow()
            .as_ref()
            .unwrap()
            .borrow_mut()
            .get(&url, opts)?
            .decode_json()?;

        Ok(data
            .as_array()
            .and_then(|a| a.get(B("resources")))
            .and_then(|v| v.as_array())
            .and_then(|a| a.get(B("core")))
            .and_then(|core| core.as_array())
            .map(|core| GithubRateLimit::Core {
                limit: core.get(B("limit")).and_then(|v| v.as_int()).unwrap_or(0),
                remaining: core
                    .get(B("remaining"))
                    .and_then(|v| v.as_int())
                    .unwrap_or(0),
            })
            .unwrap_or(GithubRateLimit::Unavailable(CheckResult::Failed)))
    }

    fn check_disk_space(&self, config: &Config) -> anyhow::Result<CheckResult> {
        if !shirabe_php_rpc::get_diagnostics()?.function_exists("disk_free_space") {
            return Ok(CheckResult::Ok);
        }

        let min_space_free: f64 = (1024 * 1024) as f64;
        let home_dir = config.get_home()?;
        let dir = path_from_bytes(&home_dir);
        let df_home = disk_free_space(dir);
        if df_home.map(|d| d < min_space_free).unwrap_or(false) {
            return Ok(CheckResult::Message(bconcat!(
                "<error>The disk hosting ",
                dir,
                " is full</error>"
            )));
        }
        let vendor_dir = config.get_path("vendor-dir")?;
        let dir = path_from_bytes(&vendor_dir);
        let df_vendor = disk_free_space(dir);
        if df_vendor.map(|d| d < min_space_free).unwrap_or(false) {
            return Ok(CheckResult::Message(bconcat!(
                "<error>The disk hosting ",
                dir,
                " is full</error>"
            )));
        }

        Ok(CheckResult::Ok)
    }

    fn check_pub_keys(&self, config: &Config) -> anyhow::Result<CheckResult> {
        let home = config.get_home()?;
        let keys_tags = bconcat!(home, "/keys.tags.pub");
        let keys_tags = path_from_bytes(&keys_tags);
        let keys_dev = bconcat!(home, "/keys.dev.pub");
        let keys_dev = path_from_bytes(&keys_dev);
        let mut errors: Vec<BString> = vec![];
        let io = self.get_io();

        if file_exists(keys_tags) && file_exists(keys_dev) {
            io.write("");
        }

        if file_exists(keys_tags) {
            io.write(bconcat!(
                "Tags Public Key Fingerprint: ",
                Keys::fingerprint(keys_tags)?
            ));
        } else {
            errors.push(BString::from(
                "<error>Missing pubkey for tags verification</error>",
            ));
        }

        if file_exists(keys_dev) {
            io.write(bconcat!(
                "Dev Public Key Fingerprint: ",
                Keys::fingerprint(keys_dev)?
            ));
        } else {
            errors.push(BString::from(
                "<error>Missing pubkey for dev verification</error>",
            ));
        }

        if !errors.is_empty() {
            errors.push(BString::from(
                "<error>Run shirabe self-update --update-keys to set them up</error>",
            ));
        }

        Ok(if !errors.is_empty() {
            CheckResult::Messages(errors)
        } else {
            CheckResult::Ok
        })
    }

    fn check_version(
        &self,
        config: &std::rc::Rc<std::cell::RefCell<Config>>,
    ) -> anyhow::Result<CheckResult> {
        let result = self.check_connectivity_and_composer_network_http_enablement()?;
        if !matches!(result, CheckResult::Ok) {
            return Ok(result);
        }

        let mut versions_util = Versions::new(
            config.clone(),
            self.http_downloader.borrow().clone().unwrap(),
        );
        let latest = match versions_util.get_latest(None) {
            Ok(Ok(l)) => l,
            Ok(Err(e)) => return Ok(CheckResult::Exception(e.into())),
            Err(e) => return Ok(CheckResult::Exception(e)),
        };

        let latest_version = latest
            .get(B("version"))
            .and_then(|v| v.as_string())
            .unwrap_or_default();
        if composer::VERSION != latest_version && composer::VERSION != "@package_version@" {
            return Ok(CheckResult::Message(bconcat!(
                "<comment>You are not running the latest ",
                versions_util.get_channel()?,
                " version, run `shirabe self-update` to update (",
                composer::VERSION,
                " => ",
                latest_version,
                ")</comment>"
            )));
        }

        Ok(CheckResult::Ok)
    }

    fn check_composer_audit(
        &self,
        config: &std::rc::Rc<std::cell::RefCell<Config>>,
    ) -> anyhow::Result<CheckResult> {
        let result = self.check_connectivity_and_composer_network_http_enablement()?;
        if !matches!(result, CheckResult::Ok) {
            return Ok(result);
        }

        let auditor = Auditor;
        let mut repo_set = RepositorySet::new(
            "stable",
            IndexMap::new(),
            vec![],
            IndexMap::new(),
            IndexMap::new(),
            IndexMap::new(),
        );
        // PHP reads the installed.json of the Composer that runs; here that is the one in the
        // Composer PHP runtime. The handle holds the file in place while the repository reads it.
        let installed =
            shirabe_php_rpc::composer_runtime::local_file("vendor/composer/installed.json")?;
        let installed_json = JsonFile::new(installed.path(), None, None)?;
        if !installed_json.exists() {
            return Ok(CheckResult::Message(BString::from(
                "<warning>Could not find Composer's installed.json, this must be a non-standard Composer installation.</>",
            )));
        }

        let local_repo = FilesystemRepository::new(installed_json, false, None, None)?;
        let version = composer::get_version();
        let mut packages = local_repo.inner.get_canonical_packages();
        if version != "@package_version@" {
            let version_parser = VersionParser::new();
            let normalized_version = version_parser.normalize(&version, None)?;
            let root_pkg = RootPackage::new(
                BString::from("composer/composer"),
                normalized_version,
                version,
            );
            packages.push(crate::package::RootPackageHandle::from_root_package(root_pkg).into());
        }
        let mut repo_config: IndexMap<BString, PhpMixed> = IndexMap::new();
        repo_config.insert("type".into(), PhpMixed::string(BString::from("composer")));
        repo_config.insert(
            "url".into(),
            PhpMixed::string(BString::from("https://packagist.org")),
        );
        let composer_repo_as_repo =
            crate::repository::RepositoryInterfaceHandle::new(ComposerRepository::new(
                repo_config,
                std::rc::Rc::new(std::cell::RefCell::new(NullIO::new())),
                &config.borrow(),
                self.http_downloader.borrow().clone().unwrap(),
                None,
            )?);
        repo_set.add_repository(composer_repo_as_repo)?;

        let io: std::rc::Rc<std::cell::RefCell<dyn IOInterface>> =
            std::rc::Rc::new(std::cell::RefCell::new(BufferIO::new(
                BString::default(),
                shirabe_symfony_console::output::VERBOSITY_NORMAL,
                None,
            )?));
        let result = match auditor.audit(
            &io,
            &repo_set,
            packages,
            bstr::BStr::new(Auditor::FORMAT_TABLE),
            true,
            IndexMap::new(),
            bstr::BStr::new(Auditor::ABANDONED_IGNORE),
            IndexMap::new(),
            false,
            IndexMap::new(),
        ) {
            Ok(r) => r,
            Err(e) => {
                return Ok(CheckResult::Message(bconcat!(
                    "<highlight>Failed performing audit: ",
                    error_message(&e),
                    "</>"
                )));
            }
        };

        if result > 0 {
            return Ok(CheckResult::Message(bconcat!(
                "<highlight>Audit found some issues:</>",
                PHP_EOL,
                io.borrow()
                    .as_any()
                    .downcast_ref::<BufferIO>()
                    .unwrap()
                    .get_output()
            )));
        }

        Ok(CheckResult::Ok)
    }

    fn get_curl_version(&self) -> anyhow::Result<BString> {
        let diagnostics = shirabe_php_rpc::get_diagnostics()?;
        if diagnostics.extension_loaded("curl") {
            if !HttpDownloader::is_curl_enabled() {
                return Ok(BString::from(
                    "<error>disabled via disable_functions; Shirabe itself does not use curl, but some plugins may be affected</error>",
                ));
            }

            let version = diagnostics
                .curl
                .as_ref()
                .expect("the diagnose payload carries curl details while the extension is loaded");
            let libz_version = version
                .libz_version
                .as_ref()
                .filter(|v| !v.is_empty())
                .map_or(bstr::BStr::new("missing"), |v| v.as_bstr());
            let brotli_version = version
                .brotli_version
                .as_ref()
                .filter(|v| !v.is_empty())
                .map_or(bstr::BStr::new("missing"), |v| v.as_bstr());
            let ssl_version = version
                .ssl_version
                .as_ref()
                .filter(|v| !v.is_empty())
                .map_or(bstr::BStr::new("missing"), |v| v.as_bstr());
            let has_zstd = match (version.features, version.version_zstd) {
                (Some(features), Some(zstd)) => features & zstd != 0,
                _ => false,
            };
            let mut http_versions = BString::from("1.0, 1.1");
            if let (Some(features), Some(http2)) = (version.features, version.version_http2)
                && version.has_http_version_2_0
                && http2 & features != 0
            {
                http_versions.push_str(", 2");
            }
            if let (Some(features), Some(http3)) = (version.features, version.version_http3)
                && features & http3 != 0
            {
                http_versions.push_str(", 3");
            }

            return Ok(bconcat!(
                "<comment>",
                version.version,
                "</comment> libz <comment>",
                libz_version,
                "</comment> brotli <comment>",
                brotli_version,
                "</comment> zstd <comment>",
                if has_zstd { "supported" } else { "missing" },
                "</comment> ssl <comment>",
                ssl_version,
                "</comment> HTTP <comment>",
                http_versions,
                "</comment>"
            ));
        }

        Ok(BString::from(
            "<error>missing; Shirabe itself does not use curl, but some plugins may be affected</error>",
        ))
    }

    fn output_result(&self, result: CheckResult) {
        let io = self.get_io();
        let messages = match result {
            CheckResult::Ok => {
                io.write("<info>OK</info>");

                return;
            }
            CheckResult::Exception(e) => {
                let exception = AnyThrowable::of(&e)
                    .expect("a check_* method passes on only an \\Exception it caught");
                vec![bconcat!(
                    "<error>[",
                    exception.php_class_name(),
                    "] ",
                    exception.get_message(),
                    "</error>"
                )]
            }
            CheckResult::Failed => vec![],
            // PHP treats '' and '0' as falsey, so such a message never reaches the output loop.
            CheckResult::Message(message) if message.is_empty() || message == "0" => vec![],
            CheckResult::Message(message) => vec![message],
            CheckResult::Messages(messages) => messages,
        };

        // falsey results should be considered as an error, even if there is nothing to output
        let mut had_error = messages.is_empty();
        let mut had_warning = false;
        for message in &messages {
            if strpos(message, "<error>").is_some() {
                had_error = true;
            } else if strpos(message, "<warning>").is_some() {
                had_warning = true;
            }
        }

        if had_error {
            io.write("<error>FAIL</error>");
            self.exit_code.set(self.exit_code.get().max(2));
        } else if had_warning {
            io.write("<warning>WARNING</warning>");
            self.exit_code.set(self.exit_code.get().max(1));
        }

        for message in &messages {
            io.write(trim2(message, " \t\n\r\0\u{0B}"));
        }
    }

    fn check_platform(&self) -> anyhow::Result<CheckResult> {
        let mut output = BString::default();
        let mut display_ini_message = false;

        let mut ini_message = bconcat!(PHP_EOL, PHP_EOL, IniHelper::get_message()?);
        ini_message.push_str(&bconcat!(PHP_EOL, "If you can not modify the ini file, you can also run `php -d option=value` to modify ini values on the fly. You can use -d multiple times."));

        let diagnostics = shirabe_php_rpc::get_diagnostics()?;

        // PHP stores `true` for an issue with no detail, and a version string otherwise.
        let mut errors: IndexMap<BString, Option<BString>> = IndexMap::new();
        let mut warnings: IndexMap<BString, Option<BString>> = IndexMap::new();

        if !diagnostics.function_exists("json_decode") {
            errors.insert(BString::from("json"), None);
        }

        if !diagnostics.extension_loaded("Phar") {
            errors.insert(BString::from("phar"), None);
        }

        if !diagnostics.extension_loaded("filter") {
            errors.insert(BString::from("filter"), None);
        }

        if !diagnostics.extension_loaded("hash") {
            errors.insert(BString::from("hash"), None);
        }

        if !diagnostics.extension_loaded("iconv") && !diagnostics.extension_loaded("mbstring") {
            errors.insert(BString::from("iconv_mbstring"), None);
        }

        if !filter_var_boolean(diagnostics.ini_get("allow_url_fopen").unwrap_or_default()) {
            errors.insert(BString::from("allow_url_fopen"), None);
        }

        if diagnostics.extension_loaded("ionCube Loader")
            && diagnostics.ioncube_loader_iversion < 40009
        {
            errors.insert(
                BString::from("ioncube"),
                Some(diagnostics.ioncube_loader_version.to_owned()),
            );
        }

        if diagnostics.php_version_id < 70205 {
            errors.insert(
                BString::from("php"),
                Some(diagnostics.php_version.to_owned()),
            );
        }

        if !diagnostics.extension_loaded("openssl") {
            errors.insert(BString::from("openssl"), None);
        }

        if diagnostics.extension_loaded("openssl")
            && diagnostics.openssl_version_number < 0x1000100f
        {
            warnings.insert(BString::from("openssl_version"), None);
        }

        if !diagnostics.has_hhvm_version
            && !diagnostics.extension_loaded("apcu")
            && filter_var_boolean(diagnostics.ini_get("apc.enable_cli").unwrap_or_default())
        {
            warnings.insert(BString::from("apc_cli"), None);
        }

        if !diagnostics.extension_loaded("zlib") {
            warnings.insert(BString::from("zlib"), None);
        }

        if let Some(phpinfo_match) = preg_match(
            php_regex!("{Configure Command(?: *</td><td class=\"v\">| *=> *)(.*?)(?:</td>|$)}m"),
            &diagnostics.phpinfo_general,
        ) {
            let configure = phpinfo_match.get(1).unwrap_or_default();

            if configure.contains_str("--enable-sigchild") {
                warnings.insert(BString::from("sigchild"), None);
            }

            if configure.contains_str("--with-curlwrappers") {
                warnings.insert(BString::from("curlwrappers"), None);
            }
        }

        if filter_var_boolean(
            diagnostics
                .ini_get("xdebug.profiler_enabled")
                .unwrap_or_default(),
        ) {
            warnings.insert(BString::from("xdebug_profile"), None);
        } else if diagnostics.xdebug_active {
            warnings.insert(BString::from("xdebug_loaded"), None);
        }

        if diagnostics.has_php_windows_version_build
            && (version_compare(&diagnostics.php_version, "7.2.23", CmpOp::Lt)
                || (version_compare(&diagnostics.php_version, "7.3.0", CmpOp::Ge)
                    && version_compare(&diagnostics.php_version, "7.3.10", CmpOp::Lt)))
        {
            warnings.insert(
                BString::from("onedrive"),
                Some(diagnostics.php_version.to_owned()),
            );
        }

        if diagnostics.extension_loaded("uopz")
            && !(filter_var_boolean(diagnostics.ini_get("uopz.disable").unwrap_or_default())
                || filter_var_boolean(diagnostics.ini_get("uopz.exit").unwrap_or_default()))
        {
            warnings.insert(BString::from("uopz"), None);
        }

        let out_fn = |msg: &BStr, style: &BStr, output: &mut BString| {
            output.push_str(&bconcat!("<", style, ">", msg, "</", style, ">", PHP_EOL));
        };

        if !errors.is_empty() {
            for (error, current) in &errors {
                let text = match error.as_bytes() {
                    b"json" => bconcat!(
                        PHP_EOL,
                        "The json extension is missing.",
                        PHP_EOL,
                        "Install it or recompile php without --disable-json"
                    ),
                    b"phar" => bconcat!(
                        PHP_EOL,
                        "The phar extension is missing.",
                        PHP_EOL,
                        "Install it or recompile php without --disable-phar"
                    ),
                    b"filter" => bconcat!(
                        PHP_EOL,
                        "The filter extension is missing.",
                        PHP_EOL,
                        "Install it or recompile php without --disable-filter"
                    ),
                    b"hash" => bconcat!(
                        PHP_EOL,
                        "The hash extension is missing.",
                        PHP_EOL,
                        "Install it or recompile php without --disable-hash"
                    ),
                    b"iconv_mbstring" => bconcat!(
                        PHP_EOL,
                        "The iconv OR mbstring extension is required and both are missing.",
                        PHP_EOL,
                        "Install either of them or recompile php without --disable-iconv"
                    ),
                    b"php" => bconcat!(
                        PHP_EOL,
                        "Your PHP (",
                        current
                            .as_deref()
                            .expect("checkPlatform stores the PHP version with the php error"),
                        ") is too old, you must upgrade to PHP 7.2.5 or higher."
                    ),
                    b"allow_url_fopen" => {
                        display_ini_message = true;
                        bconcat!(
                            PHP_EOL,
                            "The allow_url_fopen setting is incorrect.",
                            PHP_EOL,
                            "Add the following to the end of your `php.ini`:",
                            PHP_EOL,
                            "    allow_url_fopen = On"
                        )
                    }
                    b"ioncube" => {
                        display_ini_message = true;
                        bconcat!(
                            PHP_EOL,
                            "Your ionCube Loader extension (",
                            current.as_deref().expect(
                                "checkPlatform stores the loader version with the ioncube error"
                            ),
                            ") is incompatible with Phar files.",
                            PHP_EOL,
                            "Upgrade to ionCube 4.0.9 or higher or remove this line (path may be different) from your `php.ini` to disable it:",
                            PHP_EOL,
                            "    zend_extension = /usr/lib/php5/20090626+lfs/ioncube_loader_lin_5.3.so"
                        )
                    }
                    b"openssl" => bconcat!(
                        PHP_EOL,
                        "The openssl extension is missing, which means that secure HTTPS transfers are impossible.",
                        PHP_EOL,
                        "If possible you should enable it or recompile php with --with-openssl"
                    ),
                    other => {
                        return Err(InvalidArgumentException::new(bconcat!("DiagnoseCommand: Unknown error type \"", other, "\". Please report at https://github.com/nsfisis/php-shirabe/issues/new."))
                        .into());
                    }
                };
                out_fn(text.as_bstr(), BStr::new("error"), &mut output);
            }

            output.push_str(PHP_EOL);
        }

        if !warnings.is_empty() {
            for (warning, current) in &warnings {
                let text = match warning.as_bytes() {
                    b"apc_cli" => {
                        display_ini_message = true;
                        bconcat!(
                            "The apc.enable_cli setting is incorrect.",
                            PHP_EOL,
                            "Add the following to the end of your `php.ini`:",
                            PHP_EOL,
                            "  apc.enable_cli = Off"
                        )
                    }
                    b"zlib" => {
                        display_ini_message = true;
                        bconcat!(
                            "The zlib extension is not loaded, this can slow down Shirabe a lot.",
                            PHP_EOL,
                            "If possible, enable it or recompile php with --with-zlib",
                            PHP_EOL
                        )
                    }
                    b"sigchild" => bconcat!(
                        "PHP was compiled with --enable-sigchild which can cause issues on some platforms.",
                        PHP_EOL,
                        "Recompile it without this flag if possible, see also:",
                        PHP_EOL,
                        "  https://bugs.php.net/bug.php?id=22999"
                    ),
                    b"curlwrappers" => bconcat!(
                        "PHP was compiled with --with-curlwrappers which will cause issues with HTTP authentication and GitHub.",
                        PHP_EOL,
                        " Recompile it without this flag if possible"
                    ),
                    b"openssl_version" => {
                        // Attempt to parse version number out, fallback to whole string value.
                        let openssl_version_text =
                            diagnostics.openssl_version_text.clone().unwrap_or_default();
                        let openssl_trimmed = trim2(
                            strstr(&openssl_version_text, " ").unwrap_or_default(),
                            " \t\n\r\0\u{0B}",
                        );
                        let mut openssl_version =
                            strstr3(&openssl_trimmed, " ", true).unwrap_or_default();
                        if openssl_version.is_empty() {
                            openssl_version = openssl_version_text;
                        }

                        bconcat!(
                            "The OpenSSL library (",
                            openssl_version,
                            ") used by PHP does not support TLSv1.2 or TLSv1.1.",
                            PHP_EOL,
                            "If possible you should upgrade OpenSSL to version 1.0.1 or above."
                        )
                    }
                    b"xdebug_loaded" => bconcat!(
                        "The xdebug extension is loaded, this can slow down Shirabe a little.",
                        PHP_EOL,
                        " Disabling it when using Shirabe is recommended."
                    ),
                    b"xdebug_profile" => {
                        display_ini_message = true;
                        bconcat!(
                            "The xdebug.profiler_enabled setting is enabled, this can slow down Shirabe a lot.",
                            PHP_EOL,
                            "Add the following to the end of your `php.ini` to disable it:",
                            PHP_EOL,
                            "  xdebug.profiler_enabled = 0"
                        )
                    }
                    b"onedrive" => bconcat!(
                        "The Windows OneDrive folder is not supported on PHP versions below 7.2.23 and 7.3.10.",
                        PHP_EOL,
                        "Upgrade your PHP (",
                        current.as_deref().expect(
                            "checkPlatform stores the PHP version with the onedrive warning"
                        ),
                        ") to use this location with Shirabe.",
                        PHP_EOL
                    ),
                    b"uopz" => bconcat!(
                        "The uopz extension ignores exit calls and may not work with all Shirabe commands.",
                        PHP_EOL,
                        "Disabling it when using Shirabe is recommended."
                    ),
                    other => {
                        return Err(InvalidArgumentException::new(bconcat!("DiagnoseCommand: Unknown warning type \"", other, "\". Please report at https://github.com/nsfisis/php-shirabe/issues/new."))
                        .into());
                    }
                };
                out_fn(text.as_bstr(), BStr::new("comment"), &mut output);
            }
        }

        if display_ini_message {
            out_fn(ini_message.as_bstr(), BStr::new("comment"), &mut output);
        }

        let composer_ipresolve = Platform::get_env("COMPOSER_IPRESOLVE").unwrap_or_default();
        if composer_ipresolve == "4" || composer_ipresolve == "6" {
            warnings.insert(BString::from("ipresolve"), None);
            out_fn(
                bconcat!(
                    "The COMPOSER_IPRESOLVE env var is set to ",
                    Platform::get_env("COMPOSER_IPRESOLVE").unwrap_or_default(),
                    " which may result in network failures below."
                )
                .as_bstr(),
                BStr::new("comment"),
                &mut output,
            );
        }

        Ok(if warnings.is_empty() && errors.is_empty() {
            CheckResult::Ok
        } else {
            CheckResult::Message(output)
        })
    }

    /// Check if allow_url_fopen is ON
    fn check_connectivity(&self) -> anyhow::Result<CheckResult> {
        // PHP: if (!ini_get('allow_url_fopen')) — a missing setting, "" and "0" are all falsey.
        let allow_url_fopen = shirabe_php_rpc::get_diagnostics()?.ini_get("allow_url_fopen");
        if !allow_url_fopen.is_some_and(|value| !value.is_empty() && value != "0") {
            return Ok(CheckResult::Message(BString::from(
                "<info>SKIP</> <comment>Because allow_url_fopen is missing.</>",
            )));
        }

        Ok(CheckResult::Ok)
    }

    fn check_connectivity_and_composer_network_http_enablement(
        &self,
    ) -> anyhow::Result<CheckResult> {
        let result = self.check_connectivity()?;
        if !matches!(result, CheckResult::Ok) {
            return Ok(result);
        }

        let result = self.check_composer_network_http_enablement();
        if !matches!(result, CheckResult::Ok) {
            return Ok(result);
        }

        Ok(CheckResult::Ok)
    }

    /// Check if Composer network is enabled for HTTP/S
    fn check_composer_network_http_enablement(&self) -> CheckResult {
        if Platform::get_env("COMPOSER_DISABLE_NETWORK")
            .map(|v| !v.is_empty() && v != "0")
            .unwrap_or(false)
        {
            return CheckResult::Message(BString::from(
                "<info>SKIP</> <comment>Network is disabled by COMPOSER_DISABLE_NETWORK.</>",
            ));
        }

        CheckResult::Ok
    }
}

#[async_trait::async_trait(?Send)]
impl Command for DiagnoseCommand {
    fn configure(&self) -> anyhow::Result<()> {
        self.set_name("diagnose".into())?;
        self.set_description("Diagnoses the system to identify common errors".into());
        self.set_help(
            "The <info>diagnose</info> command checks common errors to help debugging problems.\n\n\
             The process exit code will be 1 in case of warnings and 2 for errors.\n\n\
             Read more at https://getcomposer.org/doc/03-cli.md#diagnose".into(),
        );
        Ok(())
    }

    async fn execute(
        &self,
        input: std::rc::Rc<std::cell::RefCell<dyn InputInterface>>,
        output: std::rc::Rc<std::cell::RefCell<dyn OutputInterface>>,
    ) -> anyhow::Result<i64> {
        let mut composer = self.try_composer(None, None);
        let io: std::rc::Rc<std::cell::RefCell<dyn IOInterface>> = self.get_io().clone();

        let config: std::rc::Rc<std::cell::RefCell<Config>>;
        if let Some(ref mut c) = composer {
            let c = crate::composer::composer_full(c);
            config = c.get_config();

            let mut command_event = CommandEvent::new6(
                PluginEvents::COMMAND,
                "diagnose",
                input,
                output,
                vec![],
                EventFlags::default(),
            );
            let command_event_name = command_event.get_name().to_owned();
            c.get_event_dispatcher()
                .borrow_mut()
                .dispatch(Some(command_event_name.as_ref()), Some(&mut command_event))?;
            *self.process.borrow_mut() = Some(
                c.get_loop()
                    .borrow()
                    .get_process_executor()
                    .map(std::rc::Rc::clone)
                    .unwrap_or_else(|| {
                        std::rc::Rc::new(std::cell::RefCell::new(ProcessExecutor::new(Some(
                            io.clone(),
                        ))))
                    }),
            );
        } else {
            config = std::rc::Rc::new(std::cell::RefCell::new(Factory::create_config(None, None)?));

            *self.process.borrow_mut() = Some(std::rc::Rc::new(std::cell::RefCell::new(
                ProcessExecutor::new(Some(io.clone())),
            )));
        }
        let mut config_inner = IndexMap::new();
        config_inner.insert("secure-http".into(), PhpMixed::Bool(false));
        let mut secure_http_wrap: IndexMap<BString, PhpMixed> = IndexMap::new();
        secure_http_wrap.insert("config".into(), PhpMixed::Array(config_inner));
        let config = config;
        config
            .borrow_mut()
            .merge(&secure_http_wrap, Config::SOURCE_COMMAND);
        let _ = config.borrow_mut().prohibit_url_by_config(
            "http://repo.packagist.org".into(),
            Some(std::rc::Rc::new(std::cell::RefCell::new(NullIO::new()))),
            &IndexMap::new(),
        );

        *self.http_downloader.borrow_mut() = Some(std::rc::Rc::new(std::cell::RefCell::new(
            Factory::create_http_downloader(io.clone(), &config, indexmap::IndexMap::new())?,
        )));

        // TODO(distribution): PHP tests `__FILE__`, which starts with `phar:` when Composer runs
        // from its phar. Shirabe ships as a native binary, so this never matches and the pubkey
        // and version checks below never run.
        if strpos(file!(), "phar:") == Some(0) {
            io.write_no_newline("Checking pubkeys: ");
            let r = self.check_pub_keys(&config.borrow())?;
            self.output_result(r);

            io.write_no_newline("Checking Shirabe version: ");
            let r = self.check_version(&config)?;
            self.output_result(r);
        }

        io.write(bconcat!(
            "Shirabe version: <comment>",
            composer::SHIRABE_VERSION,
            "</comment> (based on Composer <comment>",
            composer::VERSION,
            "</comment>)"
        ));

        io.write_no_newline("Checking Shirabe and its dependencies for vulnerabilities: ");
        let r = self.check_composer_audit(&config)?;
        self.output_result(r);

        let platform_overrides = config.borrow().get_map_or_empty("platform")?;
        let platform_overrides_unboxed: indexmap::IndexMap<BString, PhpMixed> =
            platform_overrides.into_iter().collect();
        let mut platform_repo =
            PlatformRepository::new(vec![], platform_overrides_unboxed, None, None).unwrap();
        let php_pkg = <PlatformRepository as crate::repository::RepositoryInterface>::find_package(
            &mut platform_repo,
            bstr::BStr::new("php"),
            crate::repository::FindPackageConstraint::String(BString::from("*")),
        )?
        .unwrap();
        let mut php_version = php_pkg.get_pretty_version();
        if let Some(cp) = php_pkg.as_complete()
            && cp
                .get_description()
                .unwrap_or_default()
                .contains_str("overridden")
        {
            php_version = bconcat!(php_version, " - ", cp.get_description().unwrap_or_default());
        }

        io.write(bconcat!(
            "PHP version: <comment>",
            php_version,
            "</comment>"
        ));

        let diagnostics = shirabe_php_rpc::get_diagnostics()?;

        if let Some(php_binary) = &diagnostics.php_binary {
            io.write(bconcat!(
                "PHP binary path: <comment>",
                php_binary,
                "</comment>"
            ));
        }

        io.write(bconcat!(
            "OpenSSL version: ",
            match &diagnostics.openssl_version_text {
                Some(text) => bconcat!("<comment>", text, "</comment>"),
                None => BString::from("<error>missing</error>"),
            }
        ));
        io.write(bconcat!("curl version: ", self.get_curl_version()?));

        let finder = ExecutableFinder::new();
        let has_system_unzip = finder.find("unzip", None, &[]).is_some();
        let mut bin_7zip = BString::default();
        let has_system_7zip = if finder
            .find("7z", None, &[BString::from("C:\\Program Files\\7-Zip")])
            .is_some()
        {
            bin_7zip = BString::from("7z");
            true
        } else if !Platform::is_windows() && finder.find("7zz", None, &[]).is_some() {
            bin_7zip = BString::from("7zz");
            true
        } else if !Platform::is_windows() && finder.find("7za", None, &[]).is_some() {
            bin_7zip = BString::from("7za");
            true
        } else {
            false
        };

        io.write(bconcat!("zip: ", if diagnostics.extension_loaded("zip") {
                "<comment>extension present</comment>"
            } else {
                "<comment>extension not loaded</comment>"
            }, ", ", if has_system_unzip {
                BString::from("<comment>unzip present</comment>")
            } else {
                BString::from("<comment>unzip not available</comment>")
            }, ", ", if has_system_7zip {
                bconcat!("<comment>7-Zip present (", bin_7zip, ")</comment>")
            } else {
                BString::from("<comment>7-Zip not available</comment>")
            }, if (has_system_7zip || has_system_unzip) && !diagnostics.function_exists("proc_open") {
                ", <warning>proc_open is disabled or not present, unzip/7-z will not be usable</warning>"
            } else {
                ""
            }));

        if let Some(ref mut c) = composer {
            let c = crate::composer::composer_full(c);
            io.write(bconcat!(
                "Active plugins: ",
                implode(
                    ", ",
                    &c.get_plugin_manager().borrow().get_registered_plugins()
                )
            ));

            io.write_no_newline("Checking composer.json: ");
            let r = self.check_composer_schema()?;
            self.output_result(r);

            if c.get_locker().borrow_mut().is_locked() {
                io.write_no_newline("Checking composer.lock: ");
                let locker = c.get_locker().clone();
                let locker = locker.borrow();
                let r = self.check_composer_lock_schema(&*locker)?;
                self.output_result(r);
            }
        }

        io.write_no_newline("Checking platform settings: ");
        let r = self.check_platform()?;
        self.output_result(r);

        io.write_no_newline("Checking git settings: ");
        let r = self.check_git()?;
        self.output_result(CheckResult::Message(r));

        io.write_no_newline("Checking http connectivity to packagist: ");
        let r = self.check_http(BStr::new("http"), &config)?;
        self.output_result(r);

        io.write_no_newline("Checking https connectivity to packagist: ");
        let r = self.check_http(BStr::new("https"), &config)?;
        self.output_result(r);

        let repositories = config.borrow().get_repositories();
        for repo in repositories {
            let repo_arr = repo.1.as_array().cloned().unwrap_or_default();
            if repo_arr.get(B("type")).and_then(|v| v.as_string()) == Some("composer".into())
                && repo_arr.get(B("url")).is_some()
            {
                let repo_arr_unboxed: indexmap::IndexMap<BString, PhpMixed> = repo_arr
                    .iter()
                    .map(|(k, v)| (k.clone(), v.clone()))
                    .collect();
                let composer_repo = ComposerRepository::new(
                    repo_arr_unboxed,
                    self.get_io().clone(),
                    &config.borrow(),
                    self.http_downloader.borrow().clone().unwrap(),
                    None,
                )
                .unwrap();
                // PHP: ReflectionMethod($composerRepo, 'getPackagesJsonUrl')
                // We surface the same internal call by directly invoking the equivalent method.
                // TODO(plugin): support reflection-based access if plugin code requires it.
                let url = composer_repo.get_packages_json_url();
                if !url.starts_with(b"http") {
                    continue;
                }
                if url.starts_with(b"https://repo.packagist.org") {
                    continue;
                }
                io.write_no_newline(bconcat!(
                    "Checking connectivity to ",
                    repo_arr
                        .get(B("url"))
                        .and_then(|v| v.as_string())
                        .unwrap_or_default(),
                    ": "
                ));
                let r = self.check_composer_repo(url.as_bstr(), &config)?;
                self.output_result(r);
            }
        }

        let protos: Vec<&BStr> = if config.borrow().get_bool("disable-tls") {
            vec![BStr::new("http")]
        } else {
            vec![BStr::new("http"), BStr::new("https")]
        };
        let proxy_check_result: anyhow::Result<(), anyhow::Error> = (|| -> anyhow::Result<()> {
            for proto in &protos {
                let proxy = ProxyManager::get_instance()
                    .as_ref()
                    .unwrap()
                    .get_proxy_for_request(bconcat!(proto, "://repo.packagist.org"))?;
                if !proxy.get_status(None)?.is_empty() {
                    let r#type = if proxy.is_secure() { "HTTPS" } else { "HTTP" };
                    io.write_no_newline(bconcat!("Checking ", r#type, " proxy with ", proto, ": "));
                    let r = self.check_http_proxy(&proxy, proto)?;
                    self.output_result(r);
                }
            }
            Ok(())
        })();
        if let Err(e) = proxy_check_result {
            if let Some(_te) = e.catch::<TransportException>() {
                io.write_no_newline("Checking HTTP proxy: ");
                let status = self.check_connectivity_and_composer_network_http_enablement()?;
                self.output_result(if matches!(status, CheckResult::Message(_)) {
                    status
                } else {
                    CheckResult::Exception(e)
                });
            } else {
                return Err(e);
            }
        }

        let oauth = config.borrow().get_map("github-oauth")?;
        if oauth.len() as i64 > 0 {
            for (domain, token) in &oauth {
                io.write_no_newline(bconcat!("Checking ", domain, " oauth access: "));
                let r = self
                    .check_github_oauth(domain.as_bstr(), token.as_string().unwrap_or_default())?;
                self.output_result(r);
            }
        } else {
            io.write_no_newline("Checking github.com rate limit: ");
            match self.get_github_rate_limit(BStr::new("github.com"), None) {
                Ok(GithubRateLimit::Unavailable(rate)) => self.output_result(rate),
                Ok(GithubRateLimit::Core { limit, remaining }) => {
                    if 10 > remaining {
                        io.write("<warning>WARNING</warning>");
                        io.write(bconcat!("<comment>GitHub has a rate limit on their API. You currently have <options=bold>", remaining, "</options=bold> out of <options=bold>", limit, "</options=bold> requests left.\nSee https://developer.github.com/v3/#rate-limiting and also\n    https://getcomposer.org/doc/articles/troubleshooting.md#api-rate-limit-and-oauth-tokens</comment>"));
                    } else {
                        self.output_result(CheckResult::Ok);
                    }
                }
                Err(e) => {
                    if let Some(te) = e.catch::<TransportException>()
                        && te.get_code() == 401
                    {
                        self.output_result(CheckResult::Message(BString::from("<comment>The oauth token for github.com seems invalid, run \"shirabe config --global --unset github-oauth.github.com\" to remove it</comment>")));
                    } else {
                        self.output_result(CheckResult::Exception(e));
                    }
                }
            }
        }

        io.write_no_newline("Checking disk free space: ");
        let r = self.check_disk_space(&config.borrow())?;
        self.output_result(r);

        Ok(self.exit_code.get())
    }

    fn initialize(
        &self,
        input: std::rc::Rc<std::cell::RefCell<dyn InputInterface>>,
        output: std::rc::Rc<std::cell::RefCell<dyn OutputInterface>>,
    ) -> anyhow::Result<()> {
        base_command_initialize(self, input, output)
    }

    fn complete(
        &self,
        input: &shirabe_symfony_console::completion::CompletionInput,
        suggestions: &mut shirabe_symfony_console::completion::CompletionSuggestions,
    ) -> anyhow::Result<()> {
        crate::command::base_command::base_command_complete(self, input, suggestions)
    }

    shirabe_symfony_console::delegate_command_trait_impls_to_inner!(base_command_data);
}

impl BaseCommand for DiagnoseCommand {
    fn base_command_data(&self) -> &crate::command::BaseCommandData {
        &self.base_command_data
    }

    crate::delegate_base_command_trait_impls_to_inner!(base_command_data);
}
