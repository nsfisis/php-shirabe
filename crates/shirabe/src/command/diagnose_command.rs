//! ref: composer/src/Composer/Command/DiagnoseCommand.php

use indexmap::IndexMap;

use shirabe_external_packages::composer::pcre::preg::Preg;
use shirabe_external_packages::composer::xdebug_handler::xdebug_handler::XdebugHandler;
use shirabe_external_packages::symfony::component::console::input::input_interface::InputInterface;
use shirabe_external_packages::symfony::component::console::output::output_interface::OutputInterface;
use shirabe_external_packages::symfony::component::process::executable_finder::ExecutableFinder;
use shirabe_php_shim::{
    CURL_HTTP_VERSION_2_0, CURL_VERSION_HTTP2, CURL_VERSION_HTTP3, CURL_VERSION_ZSTD,
    FILTER_VALIDATE_BOOLEAN, INFO_GENERAL, InvalidArgumentException, OPENSSL_VERSION_NUMBER,
    OPENSSL_VERSION_TEXT, PHP_BINARY, PHP_EOL, PHP_VERSION, PHP_VERSION_ID,
    PHP_WINDOWS_VERSION_BUILD, PhpMixed, RuntimeException, count, curl_version, defined,
    disk_free_space, extension_loaded, file_exists, filter_var, function_exists, get_class, hash,
    implode, ini_get, ioncube_loader_iversion, ioncube_loader_version, is_array, is_string, key,
    max_i64, ob_get_clean, ob_start, phpinfo, reset, rtrim, sprintf, str_contains, str_replace,
    str_starts_with, strpos, strstr, strtolower, trim, version_compare,
};

use crate::advisory::auditor::Auditor;
use crate::command::base_command::BaseCommand;
use crate::composer::Composer;
use crate::config::Config;
use crate::downloader::transport_exception::TransportException;
use crate::factory::Factory;
use crate::io::buffer_io::BufferIO;
use crate::io::null_io::NullIO;
use crate::json::json_file::JsonFile;
use crate::json::json_validation_exception::JsonValidationException;
use crate::package::complete_package_interface::CompletePackageInterface;
use crate::package::locker::Locker;
use crate::package::root_package::RootPackage;
use crate::package::version::version_parser::VersionParser;
use crate::plugin::command_event::CommandEvent;
use crate::plugin::plugin_events::PluginEvents;
use crate::repository::composer_repository::ComposerRepository;
use crate::repository::filesystem_repository::FilesystemRepository;
use crate::repository::platform_repository::PlatformRepository;
use crate::repository::repository_set::RepositorySet;
use crate::self_update::keys::Keys;
use crate::self_update::versions::Versions;
use crate::util::config_validator::ConfigValidator;
use crate::util::git::Git;
use crate::util::http::proxy_manager::ProxyManager;
use crate::util::http::request_proxy::RequestProxy;
use crate::util::http_downloader::HttpDownloader;
use crate::util::ini_helper::IniHelper;
use crate::util::platform::Platform;
use crate::util::process_executor::ProcessExecutor;

#[derive(Debug)]
pub struct DiagnoseCommand {
    inner: BaseCommand,
    pub(crate) http_downloader: Option<HttpDownloader>,
    pub(crate) process: Option<ProcessExecutor>,
    pub(crate) exit_code: i64,
}

impl DiagnoseCommand {
    pub(crate) fn configure(&mut self) {
        self.inner
            .set_name("diagnose")
            .set_description("Diagnoses the system to identify common errors")
            .set_help(
                "The <info>diagnose</info> command checks common errors to help debugging problems.\n\n\
                 The process exit code will be 1 in case of warnings and 2 for errors.\n\n\
                 Read more at https://getcomposer.org/doc/03-cli.md#diagnose",
            );
    }

    pub(crate) fn execute(
        &mut self,
        input: &dyn InputInterface,
        output: &dyn OutputInterface,
    ) -> anyhow::Result<i64> {
        let composer = self.inner.try_composer();
        let io = self.inner.get_io();

        let config: Config;
        if let Some(ref c) = composer {
            config = c.get_config().clone();

            let command_event = CommandEvent::new(
                PluginEvents::COMMAND,
                "diagnose",
                input,
                output,
                vec![],
                IndexMap::new(),
            );
            c.get_event_dispatcher()
                .dispatch(command_event.get_name(), &command_event);
            self.process = Some(
                c.get_loop()
                    .get_process_executor()
                    .unwrap_or_else(|| ProcessExecutor::new(Some(io.clone_box()))),
            );
        } else {
            config = Factory::create_config(None)?;

            self.process = Some(ProcessExecutor::new(Some(io.clone_box())));
        }

        let mut secure_http_wrap: IndexMap<String, Box<PhpMixed>> = IndexMap::new();
        let mut config_inner: IndexMap<String, Box<PhpMixed>> = IndexMap::new();
        config_inner.insert("secure-http".to_string(), Box::new(PhpMixed::Bool(false)));
        secure_http_wrap.insert(
            "config".to_string(),
            Box::new(PhpMixed::Array(config_inner)),
        );
        let mut config = config;
        config.merge(PhpMixed::Array(secure_http_wrap), Config::SOURCE_COMMAND);
        config.prohibit_url_by_config("http://repo.packagist.org", &NullIO::new());

        self.http_downloader = Some(Factory::create_http_downloader(io, &config)?);

        if strpos(file!(), "phar:") == Some(0) {
            io.write_no_newline("Checking pubkeys: ");
            let r = self.check_pub_keys(&config);
            self.output_result(r);

            io.write_no_newline("Checking Composer version: ");
            let r = self.check_version(&config)?;
            self.output_result(r);
        }

        io.write(&format!(
            "Composer version: <comment>{}</comment>",
            Composer::get_version()
        ));

        io.write_no_newline("Checking Composer and its dependencies for vulnerabilities: ");
        let r = self.check_composer_audit(&config)?;
        self.output_result(r);

        let platform_overrides = config
            .get("platform")
            .as_array()
            .cloned()
            .unwrap_or_default();
        let platform_repo = PlatformRepository::new(vec![], platform_overrides);
        let php_pkg = platform_repo.find_package("php", "*").unwrap();
        let mut php_version = php_pkg.get_pretty_version().to_string();
        if let Some(cp) = php_pkg.as_complete_package_interface() {
            if str_contains(&cp.get_description().unwrap_or_default(), "overridden") {
                php_version = format!(
                    "{} - {}",
                    php_version,
                    cp.get_description().unwrap_or_default()
                );
            }
        }

        io.write(&format!("PHP version: <comment>{}</comment>", php_version));

        if defined("PHP_BINARY") {
            io.write(&format!(
                "PHP binary path: <comment>{}</comment>",
                PHP_BINARY
            ));
        }

        io.write(&format!(
            "OpenSSL version: {}",
            if defined("OPENSSL_VERSION_TEXT") {
                format!("<comment>{}</comment>", OPENSSL_VERSION_TEXT)
            } else {
                "<error>missing</error>".to_string()
            }
        ));
        io.write(&format!("curl version: {}", self.get_curl_version()));

        let finder = ExecutableFinder::new();
        let has_system_unzip = finder.find("unzip", None, vec![]).is_some();
        let mut bin_7zip = String::new();
        let has_system_7zip = if finder
            .find("7z", None, vec!["C:\\Program Files\\7-Zip".to_string()])
            .is_some()
        {
            bin_7zip = "7z".to_string();
            true
        } else if !Platform::is_windows() && finder.find("7zz", None, vec![]).is_some() {
            bin_7zip = "7zz".to_string();
            true
        } else if !Platform::is_windows() && finder.find("7za", None, vec![]).is_some() {
            bin_7zip = "7za".to_string();
            true
        } else {
            false
        };

        io.write(&format!(
            "zip: {}, {}, {}{}",
            if extension_loaded("zip") {
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
            if (has_system_7zip || has_system_unzip) && !function_exists("proc_open") {
                ", <warning>proc_open is disabled or not present, unzip/7-z will not be usable</warning>"
            } else {
                ""
            }
        ));

        if let Some(ref c) = composer {
            io.write(&format!(
                "Active plugins: {}",
                implode(", ", &c.get_plugin_manager().get_registered_plugins())
            ));

            io.write_no_newline("Checking composer.json: ");
            let r = self.check_composer_schema()?;
            self.output_result(r);

            if c.get_locker().is_locked() {
                io.write_no_newline("Checking composer.lock: ");
                let r = self.check_composer_lock_schema(c.get_locker())?;
                self.output_result(r);
            }
        }

        io.write_no_newline("Checking platform settings: ");
        let r = self.check_platform()?;
        self.output_result(r);

        io.write_no_newline("Checking git settings: ");
        let r = self.check_git();
        self.output_result(PhpMixed::String(r));

        io.write_no_newline("Checking http connectivity to packagist: ");
        let r = self.check_http("http", &config)?;
        self.output_result(r);

        io.write_no_newline("Checking https connectivity to packagist: ");
        let r = self.check_http("https", &config)?;
        self.output_result(r);

        for repo in config.get_repositories() {
            let repo_arr = repo.as_array().cloned().unwrap_or_default();
            if repo_arr.get("type").and_then(|v| v.as_string()) == Some("composer")
                && repo_arr.get("url").is_some()
            {
                let composer_repo = ComposerRepository::new(
                    PhpMixed::Array(repo_arr.clone()),
                    self.inner.get_io().clone_box(),
                    config.clone(),
                    self.http_downloader.clone().unwrap(),
                );
                // PHP: ReflectionMethod($composerRepo, 'getPackagesJsonUrl')
                // We surface the same internal call by directly invoking the equivalent method.
                // TODO(plugin): support reflection-based access if plugin code requires it.
                let url = composer_repo.get_packages_json_url();
                if !str_starts_with(&url, "http") {
                    continue;
                }
                if str_starts_with(&url, "https://repo.packagist.org") {
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

        let proxy_manager = ProxyManager::get_instance();
        let protos: Vec<&str> = if config.get("disable-tls").as_bool() == Some(true) {
            vec!["http"]
        } else {
            vec!["http", "https"]
        };
        let proxy_check_result: Result<(), anyhow::Error> = (|| -> anyhow::Result<()> {
            for proto in &protos {
                let proxy =
                    proxy_manager.get_proxy_for_request(&format!("{}://repo.packagist.org", proto));
                if !proxy.get_status().is_empty() {
                    let r#type = if proxy.is_secure() { "HTTPS" } else { "HTTP" };
                    io.write_no_newline(&format!("Checking {} proxy with {}: ", r#type, proto));
                    let r = self.check_http_proxy(&proxy, proto)?;
                    self.output_result(r);
                }
            }
            Ok(())
        })();
        if let Err(e) = proxy_check_result {
            if let Some(_te) = e.downcast_ref::<TransportException>() {
                io.write_no_newline("Checking HTTP proxy: ");
                let status = self.check_connectivity_and_composer_network_http_enablement();
                self.output_result(if is_string(&status) {
                    status
                } else {
                    PhpMixed::String(format!(
                        "<error>[{}] {}</error>",
                        get_class(&e),
                        e.to_string()
                    ))
                });
            } else {
                return Err(e);
            }
        }

        let oauth = config
            .get("github-oauth")
            .as_array()
            .cloned()
            .unwrap_or_default();
        if count(&oauth) > 0 {
            for (domain, token) in &oauth {
                io.write_no_newline(&format!("Checking {} oauth access: ", domain));
                let r = self.check_github_oauth(domain, token.as_string().unwrap_or(""))?;
                self.output_result(r);
            }
        } else {
            io.write_no_newline("Checking github.com rate limit: ");
            match self.get_github_rate_limit("github.com", None) {
                Ok(rate) => {
                    if !is_array(&rate) {
                        self.output_result(rate);
                    } else if let Some(arr) = rate.as_array() {
                        let remaining = arr.get("remaining").and_then(|v| v.as_int()).unwrap_or(0);
                        let limit = arr.get("limit").and_then(|v| v.as_int()).unwrap_or(0);
                        if 10 > remaining {
                            io.write("<warning>WARNING</warning>");
                            io.write(&sprintf(
                                "<comment>GitHub has a rate limit on their API. You currently have <options=bold>%u</options=bold> out of <options=bold>%u</options=bold> requests left.\nSee https://developer.github.com/v3/#rate-limiting and also\n    https://getcomposer.org/doc/articles/troubleshooting.md#api-rate-limit-and-oauth-tokens</comment>",
                                &[remaining.into(), limit.into()],
                            ));
                        } else {
                            self.output_result(PhpMixed::Bool(true));
                        }
                    }
                }
                Err(e) => {
                    if let Some(te) = e.downcast_ref::<TransportException>() {
                        if te.get_code() == 401 {
                            self.output_result(PhpMixed::String("<comment>The oauth token for github.com seems invalid, run \"composer config --global --unset github-oauth.github.com\" to remove it</comment>".to_string()));
                        } else {
                            self.output_result(PhpMixed::String(format!(
                                "<error>[{}] {}</error>",
                                get_class(&e),
                                e.to_string()
                            )));
                        }
                    } else {
                        self.output_result(PhpMixed::String(format!(
                            "<error>[{}] {}</error>",
                            get_class(&e),
                            e.to_string()
                        )));
                    }
                }
            }
        }

        io.write_no_newline("Checking disk free space: ");
        let r = self.check_disk_space(&config);
        self.output_result(r);

        Ok(self.exit_code)
    }

    fn check_composer_schema(&self) -> anyhow::Result<PhpMixed> {
        let validator = ConfigValidator::new(self.inner.get_io().clone_box());
        let (errors, _, warnings) = validator.validate(&Factory::get_composer_file());

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

            return Ok(PhpMixed::String(rtrim(&output, " \t\n\r\0\u{0B}")));
        }

        Ok(PhpMixed::Bool(true))
    }

    fn check_composer_lock_schema(&self, locker: &Locker) -> anyhow::Result<PhpMixed> {
        let json = locker.get_json_file();

        match json.validate_schema(JsonFile::LOCK_SCHEMA) {
            Ok(_) => {}
            Err(e) => {
                if let Some(jve) = e.downcast_ref::<JsonValidationException>() {
                    let mut output = String::new();
                    for error in jve.get_errors() {
                        output.push_str(&format!("<error>{}</error>{}", error, PHP_EOL));
                    }

                    return Ok(PhpMixed::String(trim(&output, " \t\n\r\0\u{0B}")));
                }
                return Err(e);
            }
        }

        Ok(PhpMixed::Bool(true))
    }

    fn check_git(&mut self) -> String {
        if !function_exists("proc_open") {
            return "<comment>proc_open is not available, git cannot be used</comment>".to_string();
        }

        let mut output = String::new();
        self.process.as_mut().unwrap().execute(
            &vec![
                "git".to_string(),
                "config".to_string(),
                "color.ui".to_string(),
            ],
            &mut output,
        );
        if strtolower(&trim(&output, " \t\n\r\0\u{0B}")) == "always" {
            return "<comment>Your git color.ui setting is set to always, this is known to create issues. Use \"git config --global color.ui true\" to set it correctly.</comment>".to_string();
        }

        let git_version = Git::get_version(self.process.as_ref().unwrap());
        let git_version = match git_version {
            Some(v) => v,
            None => return "<comment>No git process found</>".to_string(),
        };

        if version_compare("2.24.0", &git_version, ">") {
            return format!(
                "<warning>Your git version ({}) is too old and possibly will cause issues. Please upgrade to git 2.24 or above</>",
                git_version
            );
        }

        format!("<info>OK</> <comment>git version {}</>", git_version)
    }

    fn check_http(&mut self, proto: &str, config: &Config) -> anyhow::Result<PhpMixed> {
        let result = self.check_connectivity_and_composer_network_http_enablement();
        if result.as_bool() != Some(true) {
            return Ok(result);
        }

        let mut result_list: Vec<Box<PhpMixed>> = vec![];
        let mut tls_warning: Option<String> = None;
        if proto == "https" && config.get("disable-tls").as_bool() == Some(true) {
            tls_warning = Some("<warning>Composer is configured to disable SSL/TLS protection. This will leave remote HTTPS requests vulnerable to Man-In-The-Middle attacks.</warning>".to_string());
        }

        match self.http_downloader.as_mut().unwrap().get(
            &format!("{}://repo.packagist.org/packages.json", proto),
            IndexMap::new(),
        ) {
            Ok(_) => {}
            Err(e) => {
                if let Some(te) = e.downcast_ref::<TransportException>() {
                    let hints = HttpDownloader::get_exception_hints(te);
                    if !hints.is_empty() && count(&hints) > 0 {
                        for hint in hints {
                            result_list.push(Box::new(PhpMixed::String(hint)));
                        }
                    }

                    result_list.push(Box::new(PhpMixed::String(format!(
                        "<error>[{}] {}</error>",
                        get_class(te),
                        te.get_message()
                    ))));
                } else {
                    return Err(e);
                }
            }
        }

        if let Some(w) = tls_warning {
            result_list.push(Box::new(PhpMixed::String(w)));
        }

        if count(&result_list) > 0 {
            return Ok(PhpMixed::List(result_list));
        }

        Ok(PhpMixed::Bool(true))
    }

    fn check_composer_repo(&mut self, url: &str, config: &Config) -> anyhow::Result<PhpMixed> {
        let result = self.check_connectivity_and_composer_network_http_enablement();
        if result.as_bool() != Some(true) {
            return Ok(result);
        }

        let mut result_list: Vec<Box<PhpMixed>> = vec![];
        let mut tls_warning: Option<String> = None;
        if str_starts_with(url, "https://") && config.get("disable-tls").as_bool() == Some(true) {
            tls_warning = Some("<warning>Composer is configured to disable SSL/TLS protection. This will leave remote HTTPS requests vulnerable to Man-In-The-Middle attacks.</warning>".to_string());
        }

        match self
            .http_downloader
            .as_mut()
            .unwrap()
            .get(url, IndexMap::new())
        {
            Ok(_) => {}
            Err(e) => {
                if let Some(te) = e.downcast_ref::<TransportException>() {
                    let hints = HttpDownloader::get_exception_hints(te);
                    if !hints.is_empty() && count(&hints) > 0 {
                        for hint in hints {
                            result_list.push(Box::new(PhpMixed::String(hint)));
                        }
                    }

                    result_list.push(Box::new(PhpMixed::String(format!(
                        "<error>[{}] {}</error>",
                        get_class(te),
                        te.get_message()
                    ))));
                } else {
                    return Err(e);
                }
            }
        }

        if let Some(w) = tls_warning {
            result_list.push(Box::new(PhpMixed::String(w)));
        }

        if count(&result_list) > 0 {
            return Ok(PhpMixed::List(result_list));
        }

        Ok(PhpMixed::Bool(true))
    }

    fn check_http_proxy(
        &mut self,
        proxy: &RequestProxy,
        protocol: &str,
    ) -> anyhow::Result<PhpMixed> {
        let result = self.check_connectivity_and_composer_network_http_enablement();
        if result.as_bool() != Some(true) {
            return Ok(result);
        }

        let proxy_status = proxy.get_status();

        if proxy.is_excluded_by_no_proxy() {
            return Ok(PhpMixed::String(format!(
                "<info>SKIP</> <comment>Because repo.packagist.org is {}</>",
                proxy_status
            )));
        }

        let json = self
            .http_downloader
            .as_mut()
            .unwrap()
            .get(
                &format!("{}://repo.packagist.org/packages.json", protocol),
                IndexMap::new(),
            )?
            .decode_json()?;
        if let Some(provider_includes) = json.as_array().and_then(|a| a.get("provider-includes")) {
            let mut hash_val = reset(&provider_includes.as_array().cloned().unwrap_or_default());
            hash_val = hash_val
                .as_array()
                .and_then(|a| a.get("sha256"))
                .map(|v| (**v).clone())
                .unwrap_or(PhpMixed::Null);
            let path = str_replace(
                "%hash%",
                hash_val.as_string().unwrap_or(""),
                &key(&provider_includes.as_array().cloned().unwrap_or_default())
                    .unwrap_or_default(),
            );
            let provider = self
                .http_downloader
                .as_mut()
                .unwrap()
                .get(
                    &format!("{}://repo.packagist.org/{}", protocol, path),
                    IndexMap::new(),
                )?
                .get_body();

            if hash("sha256", &provider) != hash_val.as_string().unwrap_or("") {
                return Ok(PhpMixed::String(format!(
                    "<warning>It seems that your proxy ({}) is modifying {} traffic on the fly</>",
                    proxy_status, protocol
                )));
            }
        }

        Ok(PhpMixed::String(format!(
            "<info>OK</> <comment>{}</>",
            proxy_status
        )))
    }

    fn check_github_oauth(&mut self, domain: &str, token: &str) -> anyhow::Result<PhpMixed> {
        let result = self.check_connectivity_and_composer_network_http_enablement();
        if result.as_bool() != Some(true) {
            return Ok(result);
        }

        self.inner.get_io().set_authentication(
            domain.to_string(),
            token.to_string(),
            Some("x-oauth-basic".to_string()),
        );
        let url = if domain == "github.com" {
            format!("https://api.{}/", domain)
        } else {
            format!("https://{}/api/v3/", domain)
        };

        let mut opts: IndexMap<String, Box<PhpMixed>> = IndexMap::new();
        opts.insert(
            "retry-auth-failure".to_string(),
            Box::new(PhpMixed::Bool(false)),
        );

        match self.http_downloader.as_mut().unwrap().get(&url, opts) {
            Ok(response) => {
                let expiration = response.get_header("github-authentication-token-expiration");

                if expiration.is_none() {
                    return Ok(PhpMixed::String(
                        "<info>OK</> <comment>does not expire</>".to_string(),
                    ));
                }

                Ok(PhpMixed::String(format!(
                    "<info>OK</> <comment>expires on {}</>",
                    expiration.unwrap()
                )))
            }
            Err(e) => {
                if let Some(te) = e.downcast_ref::<TransportException>() {
                    if te.get_code() == 401 {
                        return Ok(PhpMixed::String(format!(
                            "<comment>The oauth token for {} seems invalid, run \"composer config --global --unset github-oauth.{}\" to remove it</comment>",
                            domain, domain
                        )));
                    }
                }
                Ok(PhpMixed::String(format!(
                    "<error>[{}] {}</error>",
                    get_class(&e),
                    e.to_string()
                )))
            }
        }
    }

    fn get_github_rate_limit(
        &mut self,
        domain: &str,
        token: Option<&str>,
    ) -> anyhow::Result<PhpMixed> {
        let result = self.check_connectivity_and_composer_network_http_enablement();
        if result.as_bool() != Some(true) {
            return Ok(result);
        }

        if let Some(t) = token {
            self.inner.get_io().set_authentication(
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
        let mut opts: IndexMap<String, Box<PhpMixed>> = IndexMap::new();
        opts.insert(
            "retry-auth-failure".to_string(),
            Box::new(PhpMixed::Bool(false)),
        );
        let data = self
            .http_downloader
            .as_mut()
            .unwrap()
            .get(&url, opts)?
            .decode_json()?;

        Ok(data
            .as_array()
            .and_then(|a| a.get("resources"))
            .and_then(|v| v.as_array())
            .and_then(|a| a.get("core"))
            .map(|v| (**v).clone())
            .unwrap_or(PhpMixed::Null))
    }

    fn check_disk_space(&self, config: &Config) -> PhpMixed {
        if !function_exists("disk_free_space") {
            return PhpMixed::Bool(true);
        }

        let min_space_free = 1024 * 1024;
        let home_dir = config.get("home").as_string().unwrap_or("").to_string();
        let vendor_dir = config
            .get("vendor-dir")
            .as_string()
            .unwrap_or("")
            .to_string();
        let mut dir = home_dir.clone();
        let df_home = disk_free_space(&home_dir);
        if df_home.map(|d| d < min_space_free).unwrap_or(false) {
            return PhpMixed::String(format!("<error>The disk hosting {} is full</error>", dir));
        }
        dir = vendor_dir.clone();
        let df_vendor = disk_free_space(&vendor_dir);
        if df_vendor.map(|d| d < min_space_free).unwrap_or(false) {
            return PhpMixed::String(format!("<error>The disk hosting {} is full</error>", dir));
        }

        PhpMixed::Bool(true)
    }

    fn check_pub_keys(&self, config: &Config) -> PhpMixed {
        let home = config.get("home").as_string().unwrap_or("").to_string();
        let mut errors: Vec<Box<PhpMixed>> = vec![];
        let io = self.inner.get_io();

        if file_exists(&format!("{}/keys.tags.pub", home))
            && file_exists(&format!("{}/keys.dev.pub", home))
        {
            io.write("");
        }

        if file_exists(&format!("{}/keys.tags.pub", home)) {
            io.write(&format!(
                "Tags Public Key Fingerprint: {}",
                Keys::fingerprint(&format!("{}/keys.tags.pub", home))
            ));
        } else {
            errors.push(Box::new(PhpMixed::String(
                "<error>Missing pubkey for tags verification</error>".to_string(),
            )));
        }

        if file_exists(&format!("{}/keys.dev.pub", home)) {
            io.write(&format!(
                "Dev Public Key Fingerprint: {}",
                Keys::fingerprint(&format!("{}/keys.dev.pub", home))
            ));
        } else {
            errors.push(Box::new(PhpMixed::String(
                "<error>Missing pubkey for dev verification</error>".to_string(),
            )));
        }

        if !errors.is_empty() {
            errors.push(Box::new(PhpMixed::String(
                "<error>Run composer self-update --update-keys to set them up</error>".to_string(),
            )));
        }

        if !errors.is_empty() {
            PhpMixed::List(errors)
        } else {
            PhpMixed::Bool(true)
        }
    }

    fn check_version(&mut self, config: &Config) -> anyhow::Result<PhpMixed> {
        let result = self.check_connectivity_and_composer_network_http_enablement();
        if result.as_bool() != Some(true) {
            return Ok(result);
        }

        let versions_util = Versions::new(config.clone(), self.http_downloader.clone().unwrap());
        let latest = match versions_util.get_latest() {
            Ok(l) => l,
            Err(e) => {
                return Ok(PhpMixed::String(format!(
                    "<error>[{}] {}</error>",
                    get_class(&e),
                    e.to_string()
                )));
            }
        };

        let latest_version = latest
            .as_array()
            .and_then(|a| a.get("version"))
            .and_then(|v| v.as_string())
            .unwrap_or("")
            .to_string();
        if Composer::VERSION != latest_version && Composer::VERSION != "@package_version@" {
            return Ok(PhpMixed::String(format!(
                "<comment>You are not running the latest {} version, run `composer self-update` to update ({} => {})</comment>",
                versions_util.get_channel(),
                Composer::VERSION,
                latest_version
            )));
        }

        Ok(PhpMixed::Bool(true))
    }

    fn check_composer_audit(&mut self, config: &Config) -> anyhow::Result<PhpMixed> {
        let result = self.check_connectivity_and_composer_network_http_enablement();
        if result.as_bool() != Some(true) {
            return Ok(result);
        }

        let auditor = Auditor::new();
        let mut repo_set = RepositorySet::new(
            "stable".to_string(),
            IndexMap::new(),
            vec![],
            IndexMap::new(),
            IndexMap::new(),
            IndexMap::new(),
        );
        // PHP: __DIR__ . '/../../../vendor/composer/installed.json'
        let installed_json = JsonFile::new(
            "composer/src/Composer/Command/../../../vendor/composer/installed.json".to_string(),
            None,
            None,
        );
        if !installed_json.exists() {
            return Ok(PhpMixed::String("<warning>Could not find Composer's installed.json, this must be a non-standard Composer installation.</>".to_string()));
        }

        let local_repo = FilesystemRepository::new(installed_json, false, None);
        let version = Composer::get_version();
        let mut packages = local_repo.get_canonical_packages();
        if version != "@package_version@" {
            let version_parser = VersionParser::new();
            let normalized_version = version_parser.normalize(&version, None)?;
            let root_pkg = RootPackage::new(
                "composer/composer".to_string(),
                normalized_version,
                version.clone(),
            );
            packages.push(Box::new(root_pkg));
        }
        let mut repo_config: IndexMap<String, Box<PhpMixed>> = IndexMap::new();
        repo_config.insert(
            "type".to_string(),
            Box::new(PhpMixed::String("composer".to_string())),
        );
        repo_config.insert(
            "url".to_string(),
            Box::new(PhpMixed::String("https://packagist.org".to_string())),
        );
        repo_set.add_repository(Box::new(ComposerRepository::new(
            PhpMixed::Array(repo_config),
            Box::new(NullIO::new()),
            config.clone(),
            self.http_downloader.clone().unwrap(),
        )));

        let io = BufferIO::new();
        let result = match auditor.audit(
            &io,
            &repo_set,
            &packages,
            Auditor::FORMAT_TABLE,
            true,
            &IndexMap::new(),
            Auditor::ABANDONED_IGNORE,
            &IndexMap::new(),
            false,
            &IndexMap::new(),
        ) {
            Ok(r) => r,
            Err(e) => {
                return Ok(PhpMixed::String(format!(
                    "<highlight>Failed performing audit: {}</>",
                    e.to_string()
                )));
            }
        };

        if result > 0 {
            return Ok(PhpMixed::String(format!(
                "<highlight>Audit found some issues:</>{}{}",
                PHP_EOL,
                io.get_output()
            )));
        }

        Ok(PhpMixed::Bool(true))
    }

    fn get_curl_version(&self) -> String {
        if extension_loaded("curl") {
            if !HttpDownloader::is_curl_enabled() {
                return "<error>disabled via disable_functions, using php streams fallback, which reduces performance</error>".to_string();
            }

            let version = curl_version();
            let version_arr = version.as_array().cloned().unwrap_or_default();
            let libz_version = version_arr
                .get("libz_version")
                .and_then(|v| v.as_string())
                .filter(|s| !s.is_empty())
                .unwrap_or("missing")
                .to_string();
            let brotli_version = version_arr
                .get("brotli_version")
                .and_then(|v| v.as_string())
                .filter(|s| !s.is_empty())
                .unwrap_or("missing")
                .to_string();
            let ssl_version = version_arr
                .get("ssl_version")
                .and_then(|v| v.as_string())
                .filter(|s| !s.is_empty())
                .unwrap_or("missing")
                .to_string();
            let features = version_arr
                .get("features")
                .and_then(|v| v.as_int())
                .unwrap_or(0);
            let has_zstd = features != 0
                && defined("CURL_VERSION_ZSTD")
                && 0 != (features & CURL_VERSION_ZSTD);
            let mut http_versions = "1.0, 1.1".to_string();
            if features != 0
                && defined("CURL_VERSION_HTTP2")
                && defined("CURL_HTTP_VERSION_2_0")
                && (CURL_VERSION_HTTP2 & features) != 0
            {
                http_versions.push_str(", 2");
            }
            if features != 0
                && defined("CURL_VERSION_HTTP3")
                && (features & CURL_VERSION_HTTP3) != 0
            {
                http_versions.push_str(", 3");
            }

            let curl_version_str = version_arr
                .get("version")
                .and_then(|v| v.as_string())
                .unwrap_or("")
                .to_string();
            return format!(
                "<comment>{}</comment> libz <comment>{}</comment> brotli <comment>{}</comment> zstd <comment>{}</comment> ssl <comment>{}</comment> HTTP <comment>{}</comment>",
                curl_version_str,
                libz_version,
                brotli_version,
                if has_zstd { "supported" } else { "missing" },
                ssl_version,
                http_versions
            );
        }

        "<error>missing, using php streams fallback, which reduces performance</error>".to_string()
    }

    fn output_result(&mut self, result: PhpMixed) {
        let io = self.inner.get_io();
        if result.as_bool() == Some(true) {
            io.write("<info>OK</info>");

            return;
        }

        let mut had_error = false;
        let mut had_warning = false;
        let mut result = result;
        // PHP: $result instanceof \Exception → already converted to string at call sites here
        if !result.as_bool().unwrap_or(true) && !result.is_string() && !is_array(&result) {
            // falsey results should be considered as an error, even if there is nothing to output
            had_error = true;
        } else {
            let result_list: Vec<PhpMixed> = match &result {
                PhpMixed::List(l) => l.iter().map(|b| (**b).clone()).collect(),
                other => vec![other.clone()],
            };
            for message in &result_list {
                let s = message.as_string().unwrap_or("");
                if strpos(s, "<error>").is_some() {
                    had_error = true;
                } else if strpos(s, "<warning>").is_some() {
                    had_warning = true;
                }
            }
            // re-wrap so the final output loop works the same
            result = PhpMixed::List(result_list.into_iter().map(Box::new).collect());
        }

        if had_error {
            io.write("<error>FAIL</error>");
            self.exit_code = max_i64(self.exit_code, 2);
        } else if had_warning {
            io.write("<warning>WARNING</warning>");
            self.exit_code = max_i64(self.exit_code, 1);
        }

        if !result.as_bool().unwrap_or(false) {
            // PHP: if ($result) — falsey skips; this branch matches truthy
        }
        if let Some(list) = result.as_list() {
            for message in list {
                io.write(&trim(message.as_string().unwrap_or(""), " \t\n\r\0\u{0B}"));
            }
        }
    }

    fn check_platform(&mut self) -> anyhow::Result<PhpMixed> {
        let mut output = String::new();
        let mut display_ini_message = false;

        let mut ini_message = format!("{}{}{}", PHP_EOL, PHP_EOL, IniHelper::get_message());
        ini_message.push_str(&format!("{}If you can not modify the ini file, you can also run `php -d option=value` to modify ini values on the fly. You can use -d multiple times.", PHP_EOL));

        let mut errors: IndexMap<String, PhpMixed> = IndexMap::new();
        let mut warnings: IndexMap<String, PhpMixed> = IndexMap::new();

        if !function_exists("json_decode") {
            errors.insert("json".to_string(), PhpMixed::Bool(true));
        }

        if !extension_loaded("Phar") {
            errors.insert("phar".to_string(), PhpMixed::Bool(true));
        }

        if !extension_loaded("filter") {
            errors.insert("filter".to_string(), PhpMixed::Bool(true));
        }

        if !extension_loaded("hash") {
            errors.insert("hash".to_string(), PhpMixed::Bool(true));
        }

        if !extension_loaded("iconv") && !extension_loaded("mbstring") {
            errors.insert("iconv_mbstring".to_string(), PhpMixed::Bool(true));
        }

        if !filter_var(&ini_get("allow_url_fopen"), FILTER_VALIDATE_BOOLEAN)
            .as_bool()
            .unwrap_or(false)
        {
            errors.insert("allow_url_fopen".to_string(), PhpMixed::Bool(true));
        }

        if extension_loaded("ionCube Loader") && ioncube_loader_iversion() < 40009 {
            errors.insert(
                "ioncube".to_string(),
                PhpMixed::String(ioncube_loader_version()),
            );
        }

        if PHP_VERSION_ID < 70205 {
            errors.insert("php".to_string(), PhpMixed::String(PHP_VERSION.to_string()));
        }

        if !extension_loaded("openssl") {
            errors.insert("openssl".to_string(), PhpMixed::Bool(true));
        }

        if extension_loaded("openssl") && OPENSSL_VERSION_NUMBER < 0x1000100f {
            warnings.insert("openssl_version".to_string(), PhpMixed::Bool(true));
        }

        if !defined("HHVM_VERSION")
            && !extension_loaded("apcu")
            && filter_var(&ini_get("apc.enable_cli"), FILTER_VALIDATE_BOOLEAN)
                .as_bool()
                .unwrap_or(false)
        {
            warnings.insert("apc_cli".to_string(), PhpMixed::Bool(true));
        }

        if !extension_loaded("zlib") {
            warnings.insert("zlib".to_string(), PhpMixed::Bool(true));
        }

        ob_start();
        phpinfo(INFO_GENERAL);
        let phpinfo_str = ob_get_clean();
        let mut phpinfo_match: Vec<String> = vec![];
        if phpinfo_str.is_some()
            && Preg::is_match_strict_groups(
                "{Configure Command(?: *</td><td class=\"v\">| *=> *)(.*?)(?:</td>|$)}m",
                phpinfo_str.as_ref().unwrap(),
                Some(&mut phpinfo_match),
            )
            .unwrap_or(false)
        {
            let configure = &phpinfo_match[1];

            if str_contains(configure, "--enable-sigchild") {
                warnings.insert("sigchild".to_string(), PhpMixed::Bool(true));
            }

            if str_contains(configure, "--with-curlwrappers") {
                warnings.insert("curlwrappers".to_string(), PhpMixed::Bool(true));
            }
        }

        if filter_var(&ini_get("xdebug.profiler_enabled"), FILTER_VALIDATE_BOOLEAN)
            .as_bool()
            .unwrap_or(false)
        {
            warnings.insert("xdebug_profile".to_string(), PhpMixed::Bool(true));
        } else if XdebugHandler::is_xdebug_active() {
            warnings.insert("xdebug_loaded".to_string(), PhpMixed::Bool(true));
        }

        if defined("PHP_WINDOWS_VERSION_BUILD")
            && (version_compare(PHP_VERSION, "7.2.23", "<")
                || (version_compare(PHP_VERSION, "7.3.0", ">=")
                    && version_compare(PHP_VERSION, "7.3.10", "<")))
        {
            let _ = PHP_WINDOWS_VERSION_BUILD;
            warnings.insert(
                "onedrive".to_string(),
                PhpMixed::String(PHP_VERSION.to_string()),
            );
        }

        if extension_loaded("uopz")
            && !(filter_var(&ini_get("uopz.disable"), FILTER_VALIDATE_BOOLEAN)
                .as_bool()
                .unwrap_or(false)
                || filter_var(&ini_get("uopz.exit"), FILTER_VALIDATE_BOOLEAN)
                    .as_bool()
                    .unwrap_or(false))
        {
            warnings.insert("uopz".to_string(), PhpMixed::Bool(true));
        }

        let mut out_fn = |msg: &str, style: &str, output: &mut String| {
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
                        current.as_string().unwrap_or("")
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
                            current.as_string().unwrap_or(""),
                            PHP_EOL,
                            PHP_EOL
                        )
                    }
                    "openssl" => format!(
                        "{}The openssl extension is missing, which means that secure HTTPS transfers are impossible.{}If possible you should enable it or recompile php with --with-openssl",
                        PHP_EOL, PHP_EOL
                    ),
                    other => {
                        return Err(InvalidArgumentException {
                            message: sprintf(
                                "DiagnoseCommand: Unknown error type \"%s\". Please report at https://github.com/composer/composer/issues/new.",
                                &[other.to_string().into()],
                            ),
                            code: 0,
                        }
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
                            "The zlib extension is not loaded, this can slow down Composer a lot.{}If possible, enable it or recompile php with --with-zlib{}",
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
                        let openssl_trimmed =
                            trim(&strstr(OPENSSL_VERSION_TEXT, " ", false), " \t\n\r\0\u{0B}");
                        let mut openssl_version = strstr(&openssl_trimmed, " ", true);
                        if openssl_version.is_empty() {
                            openssl_version = OPENSSL_VERSION_TEXT.to_string();
                        }

                        format!(
                            "The OpenSSL library ({}) used by PHP does not support TLSv1.2 or TLSv1.1.{}If possible you should upgrade OpenSSL to version 1.0.1 or above.",
                            openssl_version, PHP_EOL
                        )
                    }
                    "xdebug_loaded" => format!(
                        "The xdebug extension is loaded, this can slow down Composer a little.{} Disabling it when using Composer is recommended.",
                        PHP_EOL
                    ),
                    "xdebug_profile" => {
                        display_ini_message = true;
                        format!(
                            "The xdebug.profiler_enabled setting is enabled, this can slow down Composer a lot.{}Add the following to the end of your `php.ini` to disable it:{}  xdebug.profiler_enabled = 0",
                            PHP_EOL, PHP_EOL
                        )
                    }
                    "onedrive" => format!(
                        "The Windows OneDrive folder is not supported on PHP versions below 7.2.23 and 7.3.10.{}Upgrade your PHP ({}) to use this location with Composer.{}",
                        PHP_EOL,
                        current.as_string().unwrap_or(""),
                        PHP_EOL
                    ),
                    "uopz" => format!(
                        "The uopz extension ignores exit calls and may not work with all Composer commands.{}Disabling it when using Composer is recommended.",
                        PHP_EOL
                    ),
                    other => {
                        return Err(InvalidArgumentException {
                            message: sprintf(
                                "DiagnoseCommand: Unknown warning type \"%s\". Please report at https://github.com/composer/composer/issues/new.",
                                &[other.to_string().into()],
                            ),
                            code: 0,
                        }
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
        if vec!["4".to_string(), "6".to_string()].contains(&composer_ipresolve) {
            warnings.insert("ipresolve".to_string(), PhpMixed::Bool(true));
            out_fn(
                &format!(
                    "The COMPOSER_IPRESOLVE env var is set to {} which may result in network failures below.",
                    Platform::get_env("COMPOSER_IPRESOLVE").unwrap_or_default()
                ),
                "comment",
                &mut output,
            );
        }

        Ok(if count(&warnings) == 0 && count(&errors) == 0 {
            PhpMixed::Bool(true)
        } else {
            PhpMixed::String(output)
        })
    }

    /// Check if allow_url_fopen is ON
    fn check_connectivity(&self) -> PhpMixed {
        if !ini_get("allow_url_fopen").parse::<bool>().unwrap_or(false)
            && ini_get("allow_url_fopen") != "1"
        {
            return PhpMixed::String(
                "<info>SKIP</> <comment>Because allow_url_fopen is missing.</>".to_string(),
            );
        }

        PhpMixed::Bool(true)
    }

    fn check_connectivity_and_composer_network_http_enablement(&self) -> PhpMixed {
        let result = self.check_connectivity();
        if result.as_bool() != Some(true) {
            return result;
        }

        let result = self.check_composer_network_http_enablement();
        if result.as_bool() != Some(true) {
            return result;
        }

        PhpMixed::Bool(true)
    }

    /// Check if Composer network is enabled for HTTP/S
    fn check_composer_network_http_enablement(&self) -> PhpMixed {
        if Platform::get_env("COMPOSER_DISABLE_NETWORK")
            .map(|v| !v.is_empty() && v != "0")
            .unwrap_or(false)
        {
            return PhpMixed::String(
                "<info>SKIP</> <comment>Network is disabled by COMPOSER_DISABLE_NETWORK.</>"
                    .to_string(),
            );
        }

        PhpMixed::Bool(true)
    }
}
