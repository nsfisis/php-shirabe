//! ref: composer/src/Composer/Command/DiagnoseCommand.php

use crate::advisory::Auditor;
use crate::command::base_command::base_command_initialize;
use crate::command::{BaseCommand, BaseCommandData};
use crate::composer;
use crate::config::Config;
use crate::downloader::TransportException;
use crate::factory::Factory;
use crate::io::BufferIO;
use crate::io::IOInterface;
use crate::io::IOInterfaceImmutable;
use crate::io::NullIO;
use crate::json::JsonFile;
use crate::json::JsonValidationException;
use crate::package::LockerInterface;
use crate::package::RootPackage;
use crate::package::version::VersionParser;
use crate::plugin::CommandEvent;
use crate::plugin::PluginEvents;
use crate::repository::ComposerRepository;
use crate::repository::FilesystemRepository;
use crate::repository::PlatformRepository;
use crate::repository::RepositorySet;
use crate::self_update::Keys;
use crate::self_update::Versions;
use crate::util::ConfigValidator;
use crate::util::Git;
use crate::util::HttpDownloader;
use crate::util::IniHelper;
use crate::util::Platform;
use crate::util::ProcessExecutor;
use crate::util::http::ProxyManager;
use crate::util::http::RequestProxy;
use indexmap::IndexMap;
use shirabe_php_shim::PhpClass as _;
use shirabe_php_shim::{
    AnyThrowable, Catch as _, CmpOp, InvalidArgumentException, PHP_EOL, PhpMixed, RuntimeException,
    disk_free_space, file_exists, filter_var_boolean, hash, impl_php_class, implode, php_regex,
    preg_match, rtrim, str_replace, strpos, strstr, strstr3, strtolower, trim, version_compare,
};
use shirabe_symfony_console::command::Command;
use shirabe_symfony_console::input::InputInterface;
use shirabe_symfony_console::output::OutputInterface;
use shirabe_symfony_process::ExecutableFinder;

/// What a `check_*` method hands to `output_result`, mirroring PHP's `true|string|string[]`.
#[derive(Debug)]
enum CheckResult {
    Ok,
    /// A falsey PHP result: counted as an error, with no message of its own.
    Failed,
    Message(String),
    Messages(Vec<String>),
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
            let mut messages: IndexMap<String, Vec<String>> = IndexMap::new();
            messages.insert("error".to_string(), errors);
            messages.insert("warning".to_string(), warnings);

            let mut output = String::new();
            for (style, msgs) in &messages {
                for msg in msgs {
                    output.push_str(&format!("<{}>{}</{}>{}", style, msg, style, PHP_EOL));
                }
            }

            return Ok(CheckResult::Message(rtrim(
                &output,
                Some(" \t\n\r\0\u{0B}"),
            )));
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
                    let mut output = String::new();
                    for error in jve.get_errors() {
                        output.push_str(&format!("<error>{}</error>{}", error, PHP_EOL));
                    }

                    return Ok(CheckResult::Message(trim(&output, Some(" \t\n\r\0\u{0B}"))));
                }
                return Err(e);
            }
        }

        Ok(CheckResult::Ok)
    }

    fn check_git(&self) -> anyhow::Result<String> {
        if !shirabe_php_rpc::get_diagnostics().function_exists("proc_open") {
            return Ok(
                "<comment>proc_open is not available, git cannot be used</comment>".to_string(),
            );
        }

        let mut output = String::new();
        let _ = self
            .process
            .borrow()
            .as_ref()
            .unwrap()
            .borrow_mut()
            .execute(
                vec![
                    "git".to_string(),
                    "config".to_string(),
                    "color.ui".to_string(),
                ],
                &mut output,
                None,
            );
        if strtolower(&trim(&output, Some(" \t\n\r\0\u{0B}"))) == "always" {
            return Ok("<comment>Your git color.ui setting is set to always, this is known to create issues. Use \"git config --global color.ui true\" to set it correctly.</comment>".to_string());
        }

        let process = self.process.borrow();
        let git_version = Git::get_version(process.as_ref().unwrap())?;
        let git_version = match git_version {
            Some(v) => v,
            None => return Ok("<comment>No git process found</>".to_string()),
        };

        if version_compare("2.24.0", &git_version, CmpOp::Gt) {
            return Ok(format!(
                "<warning>Your git version ({}) is too old and possibly will cause issues. Please upgrade to git 2.24 or above</>",
                git_version
            ));
        }

        Ok(format!(
            "<info>OK</> <comment>git version {}</>",
            git_version
        ))
    }

    fn check_http(
        &self,
        proto: &str,
        config: &std::rc::Rc<std::cell::RefCell<Config>>,
    ) -> anyhow::Result<CheckResult> {
        let result = self.check_connectivity_and_composer_network_http_enablement();
        if !matches!(result, CheckResult::Ok) {
            return Ok(result);
        }

        let mut result_list: Vec<String> = vec![];
        let mut tls_warning: Option<String> = None;
        if proto == "https" && config.borrow().get("disable-tls").as_bool() == Some(true) {
            tls_warning = Some("<warning>Shirabe is configured to disable SSL/TLS protection. This will leave remote HTTPS requests vulnerable to Man-In-The-Middle attacks.</warning>".to_string());
        }

        match self
            .http_downloader
            .borrow()
            .as_ref()
            .unwrap()
            .borrow_mut()
            .get(
                &format!("{}://repo.packagist.org/packages.json", proto),
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

                    result_list.push(format!(
                        "<error>[{}] {}</error>",
                        AnyThrowable::of(e.as_ref())
                            .expect("the catch above proved the error carries a \\Throwable")
                            .php_class_name(),
                        te.get_message()
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
        url: &str,
        config: &std::rc::Rc<std::cell::RefCell<Config>>,
    ) -> anyhow::Result<CheckResult> {
        let result = self.check_connectivity_and_composer_network_http_enablement();
        if !matches!(result, CheckResult::Ok) {
            return Ok(result);
        }

        let mut result_list: Vec<String> = vec![];
        let mut tls_warning: Option<String> = None;
        if url.starts_with("https://") && config.borrow().get("disable-tls").as_bool() == Some(true)
        {
            tls_warning = Some("<warning>Shirabe is configured to disable SSL/TLS protection. This will leave remote HTTPS requests vulnerable to Man-In-The-Middle attacks.</warning>".to_string());
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

                    result_list.push(format!(
                        "<error>[{}] {}</error>",
                        AnyThrowable::of(e.as_ref())
                            .expect("the catch above proved the error carries a \\Throwable")
                            .php_class_name(),
                        te.get_message()
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
        protocol: &str,
    ) -> anyhow::Result<CheckResult> {
        let result = self.check_connectivity_and_composer_network_http_enablement();
        if !matches!(result, CheckResult::Ok) {
            return Ok(result);
        }

        // TODO(error-model): PHP catches every \Exception below and returns it, so `output_result`
        // renders it as `<error>[class] message</error>` and the caller goes on to the next
        // protocol. The errors below propagate instead, cutting the proxy checks short.
        let proxy_status = proxy.get_status(None).unwrap_or_default();

        if proxy.is_excluded_by_no_proxy() {
            return Ok(CheckResult::Message(format!(
                "<info>SKIP</> <comment>Because repo.packagist.org is {}</>",
                proxy_status
            )));
        }

        let json = self
            .http_downloader
            .borrow()
            .as_ref()
            .unwrap()
            .borrow_mut()
            .get(
                &format!("{}://repo.packagist.org/packages.json", protocol),
                IndexMap::new(),
            )?
            .decode_json()?;
        if let Some(provider_includes) = json.as_array().and_then(|a| a.get("provider-includes")) {
            let provider_includes_arr = provider_includes.as_array().cloned().unwrap_or_default();
            let first = provider_includes_arr
                .values()
                .next()
                .cloned()
                .unwrap_or(PhpMixed::Null);
            let hash_val = first
                .as_array()
                .and_then(|a| a.get("sha256"))
                .cloned()
                .unwrap_or(PhpMixed::Null);
            let path = str_replace(
                "%hash%",
                hash_val.as_string().unwrap_or(""),
                &provider_includes
                    .as_array()
                    .and_then(|a| a.keys().next().cloned())
                    .unwrap_or_default(),
            );
            let response = self
                .http_downloader
                .borrow()
                .as_ref()
                .unwrap()
                .borrow_mut()
                .get(
                    &format!("{}://repo.packagist.org/{}", protocol, path),
                    IndexMap::new(),
                )?;
            let provider = response.get_body().unwrap_or_default().to_string();

            if hash("sha256", &provider) != hash_val.as_string().unwrap_or("") {
                return Ok(CheckResult::Message(format!(
                    "<warning>It seems that your proxy ({}) is modifying {} traffic on the fly</>",
                    proxy_status, protocol
                )));
            }
        }

        Ok(CheckResult::Message(format!(
            "<info>OK</> <comment>{}</>",
            proxy_status
        )))
    }

    fn check_github_oauth(&self, domain: &str, token: &str) -> anyhow::Result<CheckResult> {
        let result = self.check_connectivity_and_composer_network_http_enablement();
        if !matches!(result, CheckResult::Ok) {
            return Ok(result);
        }

        self.get_io().borrow_mut().set_authentication(
            domain.to_string(),
            token.to_string(),
            Some("x-oauth-basic".to_string()),
        );
        let url = if domain == "github.com" {
            format!("https://api.{}/", domain)
        } else {
            format!("https://{}/api/v3/", domain)
        };

        let mut opts: IndexMap<String, PhpMixed> = IndexMap::new();
        opts.insert("retry-auth-failure".to_string(), PhpMixed::Bool(false));

        match self
            .http_downloader
            .borrow()
            .as_ref()
            .unwrap()
            .borrow_mut()
            .get(&url, opts)
        {
            Ok(response) => {
                let expiration = response.get_header("github-authentication-token-expiration");

                if expiration.is_none() {
                    return Ok(CheckResult::Message(
                        "<info>OK</> <comment>does not expire</>".to_string(),
                    ));
                }

                Ok(CheckResult::Message(format!(
                    "<info>OK</> <comment>expires on {}</>",
                    expiration.unwrap()
                )))
            }
            Err(e) => {
                if let Some(te) = e.catch::<TransportException>()
                    && te.get_code() == 401
                {
                    return Ok(CheckResult::Message(format!(
                        "<comment>The oauth token for {} seems invalid, run \"shirabe config --global --unset github-oauth.{}\" to remove it</comment>",
                        domain, domain
                    )));
                }
                Ok(CheckResult::Message(format!(
                    "<error>[{}] {}</error>",
                    AnyThrowable::of(e.as_ref())
                        .expect("PHP reaches this only with a caught \\Throwable")
                        .php_class_name(),
                    e
                )))
            }
        }
    }

    fn get_github_rate_limit(
        &self,
        domain: &str,
        token: Option<&str>,
    ) -> anyhow::Result<GithubRateLimit> {
        let result = self.check_connectivity_and_composer_network_http_enablement();
        if !matches!(result, CheckResult::Ok) {
            return Ok(GithubRateLimit::Unavailable(result));
        }

        if let Some(t) = token {
            self.get_io().borrow_mut().set_authentication(
                domain.to_string(),
                t.to_string(),
                Some("x-oauth-basic".to_string()),
            );
        }

        let url = if domain == "github.com" {
            format!("https://api.{}/rate_limit", domain)
        } else {
            format!("https://{}/api/rate_limit", domain)
        };
        let mut opts: IndexMap<String, PhpMixed> = IndexMap::new();
        opts.insert("retry-auth-failure".to_string(), PhpMixed::Bool(false));
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
            .and_then(|a| a.get("resources"))
            .and_then(|v| v.as_array())
            .and_then(|a| a.get("core"))
            .and_then(|core| core.as_array())
            .map(|core| GithubRateLimit::Core {
                limit: core.get("limit").and_then(|v| v.as_int()).unwrap_or(0),
                remaining: core.get("remaining").and_then(|v| v.as_int()).unwrap_or(0),
            })
            .unwrap_or(GithubRateLimit::Unavailable(CheckResult::Failed)))
    }

    fn check_disk_space(&self, config: &Config) -> CheckResult {
        if !shirabe_php_rpc::get_diagnostics().function_exists("disk_free_space") {
            return CheckResult::Ok;
        }

        let min_space_free: f64 = (1024 * 1024) as f64;
        let home_dir = config.get("home").as_string().unwrap_or("").to_string();
        let vendor_dir = config
            .get("vendor-dir")
            .as_string()
            .unwrap_or("")
            .to_string();
        let mut dir = home_dir.clone();
        let df_home = disk_free_space(&home_dir);
        if df_home.map(|d| d < min_space_free).unwrap_or(false) {
            return CheckResult::Message(format!(
                "<error>The disk hosting {} is full</error>",
                dir
            ));
        }
        dir = vendor_dir.clone();
        let df_vendor = disk_free_space(&vendor_dir);
        if df_vendor.map(|d| d < min_space_free).unwrap_or(false) {
            return CheckResult::Message(format!(
                "<error>The disk hosting {} is full</error>",
                dir
            ));
        }

        CheckResult::Ok
    }

    fn check_pub_keys(&self, config: &Config) -> anyhow::Result<CheckResult> {
        let home = config.get("home").as_string().unwrap_or("").to_string();
        let mut errors: Vec<String> = vec![];
        let io = self.get_io();

        if file_exists(format!("{}/keys.tags.pub", home))
            && file_exists(format!("{}/keys.dev.pub", home))
        {
            io.write("");
        }

        if file_exists(format!("{}/keys.tags.pub", home)) {
            io.write(&format!(
                "Tags Public Key Fingerprint: {}",
                Keys::fingerprint(&format!("{}/keys.tags.pub", home))?
            ));
        } else {
            errors.push("<error>Missing pubkey for tags verification</error>".to_string());
        }

        if file_exists(format!("{}/keys.dev.pub", home)) {
            io.write(&format!(
                "Dev Public Key Fingerprint: {}",
                Keys::fingerprint(&format!("{}/keys.dev.pub", home))?
            ));
        } else {
            errors.push("<error>Missing pubkey for dev verification</error>".to_string());
        }

        if !errors.is_empty() {
            errors.push(
                "<error>Run shirabe self-update --update-keys to set them up</error>".to_string(),
            );
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
        let result = self.check_connectivity_and_composer_network_http_enablement();
        if !matches!(result, CheckResult::Ok) {
            return Ok(result);
        }

        let mut versions_util = Versions::new(
            config.clone(),
            self.http_downloader.borrow().clone().unwrap(),
        );
        let latest = match versions_util.get_latest(None) {
            Ok(Ok(l)) => l,
            Ok(Err(e)) => {
                return Ok(CheckResult::Message(format!(
                    "<error>[{}] {}</error>",
                    "UnexpectedValueException",
                    e.get_message()
                )));
            }
            Err(e) => {
                return Ok(CheckResult::Message(format!(
                    "<error>[{}] {}</error>",
                    AnyThrowable::of(e.as_ref())
                        .expect("PHP reaches this only with a caught \\Throwable")
                        .php_class_name(),
                    e
                )));
            }
        };

        let latest_version = latest
            .get("version")
            .and_then(|v| v.as_string())
            .unwrap_or("")
            .to_string();
        if composer::VERSION != latest_version && composer::VERSION != "@package_version@" {
            return Ok(CheckResult::Message(format!(
                "<comment>You are not running the latest {} version, run `shirabe self-update` to update ({} => {})</comment>",
                versions_util.get_channel()?,
                composer::VERSION,
                latest_version
            )));
        }

        Ok(CheckResult::Ok)
    }

    fn check_composer_audit(
        &self,
        config: &std::rc::Rc<std::cell::RefCell<Config>>,
    ) -> anyhow::Result<CheckResult> {
        let result = self.check_connectivity_and_composer_network_http_enablement();
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
        // TODO(bytes): JsonFile holds its path as a string, since it takes http URLs too, so the
        // path has to be representable as UTF-8.
        let path = installed.path();
        let path = path.to_str().ok_or_else(|| {
            RuntimeException::new(format!("Path contains invalid UTF-8: {}", path.display()))
        })?;
        let installed_json = JsonFile::new(path.to_string(), None, None)?;
        if !installed_json.exists() {
            return Ok(CheckResult::Message(
                "<warning>Could not find Composer's installed.json, this must be a non-standard Composer installation.</>".to_string(),
            ));
        }

        let local_repo = FilesystemRepository::new(installed_json, false, None, None)?;
        let version = composer::get_version();
        let mut packages = local_repo.inner.get_canonical_packages();
        if version != "@package_version@" {
            let version_parser = VersionParser::new();
            let normalized_version = version_parser.normalize(&version, None)?;
            let root_pkg =
                RootPackage::new("composer/composer".to_string(), normalized_version, version);
            packages.push(crate::package::RootPackageHandle::from_root_package(root_pkg).into());
        }
        let mut repo_config: IndexMap<String, PhpMixed> = IndexMap::new();
        repo_config.insert("type".to_string(), PhpMixed::String("composer".to_string()));
        repo_config.insert(
            "url".to_string(),
            PhpMixed::String("https://packagist.org".to_string()),
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
                String::new(),
                shirabe_symfony_console::output::VERBOSITY_NORMAL,
                None,
            )?));
        let result = match auditor.audit(
            &io,
            &repo_set,
            packages,
            Auditor::FORMAT_TABLE,
            true,
            IndexMap::new(),
            Auditor::ABANDONED_IGNORE,
            IndexMap::new(),
            false,
            IndexMap::new(),
        ) {
            Ok(r) => r,
            Err(e) => {
                return Ok(CheckResult::Message(format!(
                    "<highlight>Failed performing audit: {}</>",
                    e
                )));
            }
        };

        if result > 0 {
            return Ok(CheckResult::Message(format!(
                "<highlight>Audit found some issues:</>{}{}",
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

    fn get_curl_version(&self) -> String {
        if shirabe_php_rpc::get_diagnostics().extension_loaded("curl") {
            if !HttpDownloader::is_curl_enabled() {
                return "<error>disabled via disable_functions, using php streams fallback, which reduces performance</error>".to_string();
            }

            let version = shirabe_php_rpc::get_diagnostics()
                .curl
                .as_ref()
                .expect("the diagnose payload carries curl details while the extension is loaded");
            let libz_version = version
                .libz_version
                .as_deref()
                .filter(|v| !v.is_empty())
                .unwrap_or("missing");
            let brotli_version = version
                .brotli_version
                .as_deref()
                .filter(|v| !v.is_empty())
                .unwrap_or("missing");
            let ssl_version = version
                .ssl_version
                .as_deref()
                .filter(|v| !v.is_empty())
                .unwrap_or("missing");
            let has_zstd = match (version.features, version.version_zstd) {
                (Some(features), Some(zstd)) => features & zstd != 0,
                _ => false,
            };
            let mut http_versions = "1.0, 1.1".to_string();
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

            return format!(
                "<comment>{}</comment> libz <comment>{}</comment> brotli <comment>{}</comment> zstd <comment>{}</comment> ssl <comment>{}</comment> HTTP <comment>{}</comment>",
                version.version,
                libz_version,
                brotli_version,
                if has_zstd { "supported" } else { "missing" },
                ssl_version,
                http_versions,
            );
        }

        "<error>missing, using php streams fallback, which reduces performance</error>".to_string()
    }

    // PHP: $result instanceof \Exception → already converted to string at call sites here
    fn output_result(&self, result: CheckResult) {
        let io = self.get_io();
        let messages = match result {
            CheckResult::Ok => {
                io.write("<info>OK</info>");

                return;
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
            io.write(&trim(message, Some(" \t\n\r\0\u{0B}")));
        }
    }

    fn check_platform(&self) -> anyhow::Result<CheckResult> {
        let mut output = String::new();
        let mut display_ini_message = false;

        let mut ini_message = format!("{}{}{}", PHP_EOL, PHP_EOL, IniHelper::get_message());
        ini_message.push_str(&format!("{}If you can not modify the ini file, you can also run `php -d option=value` to modify ini values on the fly. You can use -d multiple times.", PHP_EOL));

        let diagnostics = shirabe_php_rpc::get_diagnostics();

        // PHP stores `true` for an issue with no detail, and a version string otherwise.
        let mut errors: IndexMap<String, Option<String>> = IndexMap::new();
        let mut warnings: IndexMap<String, Option<String>> = IndexMap::new();

        if !diagnostics.function_exists("json_decode") {
            errors.insert("json".to_string(), None);
        }

        if !diagnostics.extension_loaded("Phar") {
            errors.insert("phar".to_string(), None);
        }

        if !diagnostics.extension_loaded("filter") {
            errors.insert("filter".to_string(), None);
        }

        if !diagnostics.extension_loaded("hash") {
            errors.insert("hash".to_string(), None);
        }

        if !diagnostics.extension_loaded("iconv") && !diagnostics.extension_loaded("mbstring") {
            errors.insert("iconv_mbstring".to_string(), None);
        }

        if !filter_var_boolean(diagnostics.ini_get("allow_url_fopen").unwrap_or("")) {
            errors.insert("allow_url_fopen".to_string(), None);
        }

        if diagnostics.extension_loaded("ionCube Loader")
            && diagnostics.ioncube_loader_iversion < 40009
        {
            errors.insert(
                "ioncube".to_string(),
                Some(diagnostics.ioncube_loader_version.clone()),
            );
        }

        if diagnostics.php_version_id < 70205 {
            errors.insert("php".to_string(), Some(diagnostics.php_version.clone()));
        }

        if !diagnostics.extension_loaded("openssl") {
            errors.insert("openssl".to_string(), None);
        }

        if diagnostics.extension_loaded("openssl")
            && diagnostics.openssl_version_number < 0x1000100f
        {
            warnings.insert("openssl_version".to_string(), None);
        }

        if !diagnostics.has_hhvm_version
            && !diagnostics.extension_loaded("apcu")
            && filter_var_boolean(diagnostics.ini_get("apc.enable_cli").unwrap_or(""))
        {
            warnings.insert("apc_cli".to_string(), None);
        }

        if !diagnostics.extension_loaded("zlib") {
            warnings.insert("zlib".to_string(), None);
        }

        if let Some(phpinfo_match) = preg_match(
            php_regex!("{Configure Command(?: *</td><td class=\"v\">| *=> *)(.*?)(?:</td>|$)}m"),
            &diagnostics.phpinfo_general,
        ) {
            let configure = phpinfo_match.get(1).unwrap_or_default().to_string();
            let configure = configure.as_str();

            if configure.contains("--enable-sigchild") {
                warnings.insert("sigchild".to_string(), None);
            }

            if configure.contains("--with-curlwrappers") {
                warnings.insert("curlwrappers".to_string(), None);
            }
        }

        if filter_var_boolean(diagnostics.ini_get("xdebug.profiler_enabled").unwrap_or("")) {
            warnings.insert("xdebug_profile".to_string(), None);
        } else if diagnostics.xdebug_active {
            warnings.insert("xdebug_loaded".to_string(), None);
        }

        if diagnostics.has_php_windows_version_build
            && (version_compare(&diagnostics.php_version, "7.2.23", CmpOp::Lt)
                || (version_compare(&diagnostics.php_version, "7.3.0", CmpOp::Ge)
                    && version_compare(&diagnostics.php_version, "7.3.10", CmpOp::Lt)))
        {
            warnings.insert(
                "onedrive".to_string(),
                Some(diagnostics.php_version.clone()),
            );
        }

        if diagnostics.extension_loaded("uopz")
            && !(filter_var_boolean(diagnostics.ini_get("uopz.disable").unwrap_or(""))
                || filter_var_boolean(diagnostics.ini_get("uopz.exit").unwrap_or("")))
        {
            warnings.insert("uopz".to_string(), None);
        }

        let out_fn = |msg: &str, style: &str, output: &mut String| {
            output.push_str(&format!("<{}>{}</{}>{}", style, msg, style, PHP_EOL));
        };

        if !errors.is_empty() {
            for (error, current) in &errors {
                let text = match error.as_str() {
                    "json" => format!(
                        "{}The json extension is missing.{}Install it or recompile php without --disable-json",
                        PHP_EOL, PHP_EOL
                    ),
                    "phar" => format!(
                        "{}The phar extension is missing.{}Install it or recompile php without --disable-phar",
                        PHP_EOL, PHP_EOL
                    ),
                    "filter" => format!(
                        "{}The filter extension is missing.{}Install it or recompile php without --disable-filter",
                        PHP_EOL, PHP_EOL
                    ),
                    "hash" => format!(
                        "{}The hash extension is missing.{}Install it or recompile php without --disable-hash",
                        PHP_EOL, PHP_EOL
                    ),
                    "iconv_mbstring" => format!(
                        "{}The iconv OR mbstring extension is required and both are missing.{}Install either of them or recompile php without --disable-iconv",
                        PHP_EOL, PHP_EOL
                    ),
                    "php" => format!(
                        "{}Your PHP ({}) is too old, you must upgrade to PHP 7.2.5 or higher.",
                        PHP_EOL,
                        current
                            .as_deref()
                            .expect("checkPlatform stores the PHP version with the php error")
                    ),
                    "allow_url_fopen" => {
                        display_ini_message = true;
                        format!(
                            "{}The allow_url_fopen setting is incorrect.{}Add the following to the end of your `php.ini`:{}    allow_url_fopen = On",
                            PHP_EOL, PHP_EOL, PHP_EOL
                        )
                    }
                    "ioncube" => {
                        display_ini_message = true;
                        format!(
                            "{}Your ionCube Loader extension ({}) is incompatible with Phar files.{}Upgrade to ionCube 4.0.9 or higher or remove this line (path may be different) from your `php.ini` to disable it:{}    zend_extension = /usr/lib/php5/20090626+lfs/ioncube_loader_lin_5.3.so",
                            PHP_EOL,
                            current.as_deref().expect(
                                "checkPlatform stores the loader version with the ioncube error"
                            ),
                            PHP_EOL,
                            PHP_EOL
                        )
                    }
                    "openssl" => format!(
                        "{}The openssl extension is missing, which means that secure HTTPS transfers are impossible.{}If possible you should enable it or recompile php with --with-openssl",
                        PHP_EOL, PHP_EOL
                    ),
                    other => {
                        return Err(InvalidArgumentException::new(format!(
                            "DiagnoseCommand: Unknown error type \"{}\". Please report at https://github.com/nsfisis/php-shirabe/issues/new.",
                            other,
                        ))
                        .into());
                    }
                };
                out_fn(&text, "error", &mut output);
            }

            output.push_str(PHP_EOL);
        }

        if !warnings.is_empty() {
            for (warning, current) in &warnings {
                let text = match warning.as_str() {
                    "apc_cli" => {
                        display_ini_message = true;
                        format!(
                            "The apc.enable_cli setting is incorrect.{}Add the following to the end of your `php.ini`:{}  apc.enable_cli = Off",
                            PHP_EOL, PHP_EOL
                        )
                    }
                    "zlib" => {
                        display_ini_message = true;
                        format!(
                            "The zlib extension is not loaded, this can slow down Shirabe a lot.{}If possible, enable it or recompile php with --with-zlib{}",
                            PHP_EOL, PHP_EOL
                        )
                    }
                    "sigchild" => format!(
                        "PHP was compiled with --enable-sigchild which can cause issues on some platforms.{}Recompile it without this flag if possible, see also:{}  https://bugs.php.net/bug.php?id=22999",
                        PHP_EOL, PHP_EOL
                    ),
                    "curlwrappers" => format!(
                        "PHP was compiled with --with-curlwrappers which will cause issues with HTTP authentication and GitHub.{} Recompile it without this flag if possible",
                        PHP_EOL
                    ),
                    "openssl_version" => {
                        // Attempt to parse version number out, fallback to whole string value.
                        let openssl_version_text =
                            diagnostics.openssl_version_text.clone().unwrap_or_default();
                        let openssl_trimmed = trim(
                            &strstr(&openssl_version_text, " ").unwrap_or_default(),
                            Some(" \t\n\r\0\u{0B}"),
                        );
                        let mut openssl_version =
                            strstr3(&openssl_trimmed, " ", true).unwrap_or_default();
                        if openssl_version.is_empty() {
                            openssl_version = openssl_version_text;
                        }

                        format!(
                            "The OpenSSL library ({}) used by PHP does not support TLSv1.2 or TLSv1.1.{}If possible you should upgrade OpenSSL to version 1.0.1 or above.",
                            openssl_version, PHP_EOL
                        )
                    }
                    "xdebug_loaded" => format!(
                        "The xdebug extension is loaded, this can slow down Shirabe a little.{} Disabling it when using Shirabe is recommended.",
                        PHP_EOL
                    ),
                    "xdebug_profile" => {
                        display_ini_message = true;
                        format!(
                            "The xdebug.profiler_enabled setting is enabled, this can slow down Shirabe a lot.{}Add the following to the end of your `php.ini` to disable it:{}  xdebug.profiler_enabled = 0",
                            PHP_EOL, PHP_EOL
                        )
                    }
                    "onedrive" => format!(
                        "The Windows OneDrive folder is not supported on PHP versions below 7.2.23 and 7.3.10.{}Upgrade your PHP ({}) to use this location with Shirabe.{}",
                        PHP_EOL,
                        current.as_deref().expect(
                            "checkPlatform stores the PHP version with the onedrive warning"
                        ),
                        PHP_EOL
                    ),
                    "uopz" => format!(
                        "The uopz extension ignores exit calls and may not work with all Shirabe commands.{}Disabling it when using Shirabe is recommended.",
                        PHP_EOL
                    ),
                    other => {
                        return Err(InvalidArgumentException::new(format!(
                            "DiagnoseCommand: Unknown warning type \"{}\". Please report at https://github.com/nsfisis/php-shirabe/issues/new.",
                            other,
                        ))
                        .into());
                    }
                };
                out_fn(&text, "comment", &mut output);
            }
        }

        if display_ini_message {
            out_fn(&ini_message, "comment", &mut output);
        }

        let composer_ipresolve = Platform::get_env("COMPOSER_IPRESOLVE").unwrap_or_default();
        if ["4".to_string(), "6".to_string()].contains(&composer_ipresolve) {
            warnings.insert("ipresolve".to_string(), None);
            out_fn(
                &format!(
                    "The COMPOSER_IPRESOLVE env var is set to {} which may result in network failures below.",
                    Platform::get_env("COMPOSER_IPRESOLVE").unwrap_or_default()
                ),
                "comment",
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
    fn check_connectivity(&self) -> CheckResult {
        // PHP: if (!ini_get('allow_url_fopen')) — a missing setting, "" and "0" are all falsey.
        let allow_url_fopen = shirabe_php_rpc::get_diagnostics().ini_get("allow_url_fopen");
        if !allow_url_fopen.is_some_and(|value| !value.is_empty() && value != "0") {
            return CheckResult::Message(
                "<info>SKIP</> <comment>Because allow_url_fopen is missing.</>".to_string(),
            );
        }

        CheckResult::Ok
    }

    fn check_connectivity_and_composer_network_http_enablement(&self) -> CheckResult {
        let result = self.check_connectivity();
        if !matches!(result, CheckResult::Ok) {
            return result;
        }

        let result = self.check_composer_network_http_enablement();
        if !matches!(result, CheckResult::Ok) {
            return result;
        }

        CheckResult::Ok
    }

    /// Check if Composer network is enabled for HTTP/S
    fn check_composer_network_http_enablement(&self) -> CheckResult {
        if Platform::get_env("COMPOSER_DISABLE_NETWORK")
            .map(|v| !v.is_empty() && v != "0")
            .unwrap_or(false)
        {
            return CheckResult::Message(
                "<info>SKIP</> <comment>Network is disabled by COMPOSER_DISABLE_NETWORK.</>"
                    .to_string(),
            );
        }

        CheckResult::Ok
    }
}

impl Command for DiagnoseCommand {
    fn configure(&self) -> anyhow::Result<()> {
        self.set_name("diagnose")?;
        self.set_description("Diagnoses the system to identify common errors");
        self.set_help(
            "The <info>diagnose</info> command checks common errors to help debugging problems.\n\n\
             The process exit code will be 1 in case of warnings and 2 for errors.\n\n\
             Read more at https://getcomposer.org/doc/03-cli.md#diagnose",
        );
        Ok(())
    }

    fn execute(
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

            let command_event = CommandEvent::new6(
                PluginEvents::COMMAND,
                "diagnose",
                input,
                output,
                vec![],
                IndexMap::new(),
            );
            c.get_event_dispatcher()
                .borrow_mut()
                .dispatch(Some(command_event.get_name()), None);
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
        config_inner.insert("secure-http".to_string(), PhpMixed::Bool(false));
        let mut secure_http_wrap: IndexMap<String, PhpMixed> = IndexMap::new();
        secure_http_wrap.insert("config".to_string(), PhpMixed::Array(config_inner));
        let config = config;
        config
            .borrow_mut()
            .merge(&secure_http_wrap, Config::SOURCE_COMMAND);
        let _ = config.borrow_mut().prohibit_url_by_config(
            "http://repo.packagist.org",
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

        io.write(&format!(
            "Shirabe version: <comment>{}</comment> (based on Composer <comment>{}</comment>)",
            composer::SHIRABE_VERSION,
            composer::VERSION
        ));

        io.write_no_newline("Checking Shirabe and its dependencies for vulnerabilities: ");
        let r = self.check_composer_audit(&config)?;
        self.output_result(r);

        let platform_overrides = config
            .borrow_mut()
            .get("platform")
            .as_array()
            .cloned()
            .unwrap_or_default();
        let platform_overrides_unboxed: indexmap::IndexMap<String, PhpMixed> =
            platform_overrides.into_iter().collect();
        let mut platform_repo =
            PlatformRepository::new(vec![], platform_overrides_unboxed, None, None).unwrap();
        let php_pkg = <PlatformRepository as crate::repository::RepositoryInterface>::find_package(
            &mut platform_repo,
            "php",
            crate::repository::FindPackageConstraint::String("*".to_string()),
        )?
        .unwrap();
        let mut php_version = php_pkg.get_pretty_version();
        if let Some(cp) = php_pkg.as_complete()
            && cp
                .get_description()
                .unwrap_or_default()
                .contains("overridden")
        {
            php_version = format!(
                "{} - {}",
                php_version,
                cp.get_description().unwrap_or_default()
            );
        }

        io.write(&format!("PHP version: <comment>{}</comment>", php_version));

        let diagnostics = shirabe_php_rpc::get_diagnostics();

        if let Some(php_binary) = &diagnostics.php_binary {
            io.write(&format!(
                "PHP binary path: <comment>{}</comment>",
                php_binary
            ));
        }

        io.write(&format!(
            "OpenSSL version: {}",
            match &diagnostics.openssl_version_text {
                Some(text) => format!("<comment>{}</comment>", text),
                None => "<error>missing</error>".to_string(),
            }
        ));
        io.write(&format!("curl version: {}", self.get_curl_version()));

        let finder = ExecutableFinder::new();
        let has_system_unzip = finder.find("unzip", None, &[]).is_some();
        let mut bin_7zip = String::new();
        let has_system_7zip = if finder
            .find("7z", None, &["C:\\Program Files\\7-Zip".to_string()])
            .is_some()
        {
            bin_7zip = "7z".to_string();
            true
        } else if !Platform::is_windows() && finder.find("7zz", None, &[]).is_some() {
            bin_7zip = "7zz".to_string();
            true
        } else if !Platform::is_windows() && finder.find("7za", None, &[]).is_some() {
            bin_7zip = "7za".to_string();
            true
        } else {
            false
        };

        io.write(&format!(
            "zip: {}, {}, {}{}",
            if diagnostics.extension_loaded("zip") {
                "<comment>extension present</comment>"
            } else {
                "<comment>extension not loaded</comment>"
            },
            if has_system_unzip {
                "<comment>unzip present</comment>".to_string()
            } else {
                "<comment>unzip not available</comment>".to_string()
            },
            if has_system_7zip {
                format!("<comment>7-Zip present ({})</comment>", bin_7zip)
            } else {
                "<comment>7-Zip not available</comment>".to_string()
            },
            if (has_system_7zip || has_system_unzip) && !diagnostics.function_exists("proc_open") {
                ", <warning>proc_open is disabled or not present, unzip/7-z will not be usable</warning>"
            } else {
                ""
            }
        ));

        if let Some(ref mut c) = composer {
            let c = crate::composer::composer_full(c);
            io.write(&format!(
                "Active plugins: {}",
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
        let r = self.check_http("http", &config)?;
        self.output_result(r);

        io.write_no_newline("Checking https connectivity to packagist: ");
        let r = self.check_http("https", &config)?;
        self.output_result(r);

        let repositories = config.borrow().get_repositories();
        for repo in repositories {
            let repo_arr = repo.1.as_array().cloned().unwrap_or_default();
            if repo_arr.get("type").and_then(|v| v.as_string()) == Some("composer")
                && repo_arr.get("url").is_some()
            {
                let repo_arr_unboxed: indexmap::IndexMap<String, PhpMixed> = repo_arr
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
                if !url.starts_with("http") {
                    continue;
                }
                if url.starts_with("https://repo.packagist.org") {
                    continue;
                }
                io.write_no_newline(&format!(
                    "Checking connectivity to {}: ",
                    repo_arr
                        .get("url")
                        .and_then(|v| v.as_string())
                        .unwrap_or("")
                ));
                let r = self.check_composer_repo(&url, &config)?;
                self.output_result(r);
            }
        }

        let protos: Vec<&str> = if config.borrow_mut().get("disable-tls").as_bool() == Some(true) {
            vec!["http"]
        } else {
            vec!["http", "https"]
        };
        let proxy_check_result: anyhow::Result<(), anyhow::Error> = (|| -> anyhow::Result<()> {
            for proto in &protos {
                // Compute the proxy under a short-lived lock: `check_http_proxy` below transitively
                // re-enters `ProxyManager::get_instance()` (via HttpDownloader -> CurlDownloader /
                // RemoteFilesystem), and `std::sync::Mutex` is not reentrant, so the guard must not
                // still be held when that call happens.
                let proxy = ProxyManager::get_instance()
                    .as_ref()
                    .unwrap()
                    .get_proxy_for_request(&format!("{}://repo.packagist.org", proto))
                    .map_err(|e| anyhow::anyhow!(e))?;
                if !proxy.get_status(None)?.is_empty() {
                    let r#type = if proxy.is_secure() { "HTTPS" } else { "HTTP" };
                    io.write_no_newline(&format!("Checking {} proxy with {}: ", r#type, proto));
                    let r = self.check_http_proxy(&proxy, proto)?;
                    self.output_result(r);
                }
            }
            Ok(())
        })();
        if let Err(e) = proxy_check_result {
            if let Some(_te) = e.catch::<TransportException>() {
                io.write_no_newline("Checking HTTP proxy: ");
                let status = self.check_connectivity_and_composer_network_http_enablement();
                self.output_result(if matches!(status, CheckResult::Message(_)) {
                    status
                } else {
                    CheckResult::Message(format!(
                        "<error>[{}] {}</error>",
                        AnyThrowable::of(e.as_ref())
                            .expect("PHP reaches this only with a caught \\Throwable")
                            .php_class_name(),
                        e
                    ))
                });
            } else {
                return Err(e);
            }
        }

        let oauth = config
            .borrow_mut()
            .get("github-oauth")
            .as_array()
            .cloned()
            .unwrap_or_default();
        if oauth.len() as i64 > 0 {
            for (domain, token) in &oauth {
                io.write_no_newline(&format!("Checking {} oauth access: ", domain));
                let r = self.check_github_oauth(domain, token.as_string().unwrap_or(""))?;
                self.output_result(r);
            }
        } else {
            io.write_no_newline("Checking github.com rate limit: ");
            match self.get_github_rate_limit("github.com", None) {
                Ok(GithubRateLimit::Unavailable(rate)) => self.output_result(rate),
                Ok(GithubRateLimit::Core { limit, remaining }) => {
                    if 10 > remaining {
                        io.write("<warning>WARNING</warning>");
                        io.write(&format!(
                            "<comment>GitHub has a rate limit on their API. You currently have <options=bold>{}</options=bold> out of <options=bold>{}</options=bold> requests left.\nSee https://developer.github.com/v3/#rate-limiting and also\n    https://getcomposer.org/doc/articles/troubleshooting.md#api-rate-limit-and-oauth-tokens</comment>",
                            remaining, limit,
                        ));
                    } else {
                        self.output_result(CheckResult::Ok);
                    }
                }
                Err(e) => {
                    if let Some(te) = e.catch::<TransportException>() {
                        if te.get_code() == 401 {
                            self.output_result(CheckResult::Message("<comment>The oauth token for github.com seems invalid, run \"shirabe config --global --unset github-oauth.github.com\" to remove it</comment>".to_string()));
                        } else {
                            self.output_result(CheckResult::Message(format!(
                                "<error>[{}] {}</error>",
                                AnyThrowable::of(e.as_ref())
                                    .expect("PHP reaches this only with a caught \\Throwable")
                                    .php_class_name(),
                                e
                            )));
                        }
                    } else {
                        self.output_result(CheckResult::Message(format!(
                            "<error>[{}] {}</error>",
                            AnyThrowable::of(e.as_ref())
                                .expect("PHP reaches this only with a caught \\Throwable")
                                .php_class_name(),
                            e
                        )));
                    }
                }
            }
        }

        io.write_no_newline("Checking disk free space: ");
        let r = self.check_disk_space(&config.borrow());
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
