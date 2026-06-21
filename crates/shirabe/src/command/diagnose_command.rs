//! ref: composer/src/Composer/Command/DiagnoseCommand.php

use anyhow::Result;
use indexmap::IndexMap;

use shirabe_external_packages::composer::pcre::{CaptureKey, Preg};
use shirabe_external_packages::composer::xdebug_handler::XdebugHandler;
use shirabe_external_packages::symfony::console::command::command::Command;
use shirabe_external_packages::symfony::console::input::InputInterface;
use shirabe_external_packages::symfony::console::output::OutputInterface;
use shirabe_external_packages::symfony::process::ExecutableFinder;
use shirabe_php_shim::{
    CURL_HTTP_VERSION_2_0, CURL_VERSION_HTTP2, CURL_VERSION_HTTP3, CURL_VERSION_ZSTD, INFO_GENERAL,
    InvalidArgumentException, OPENSSL_VERSION_NUMBER, OPENSSL_VERSION_TEXT, PHP_BINARY, PHP_EOL,
    PHP_VERSION, PHP_VERSION_ID, PHP_WINDOWS_VERSION_BUILD, PhpMixed, RuntimeException, count,
    curl_version, defined, disk_free_space, extension_loaded, file_exists, filter_var_boolean,
    function_exists, get_class, get_class_err, hash, implode, ini_get, ioncube_loader_iversion,
    ioncube_loader_version, is_array, is_string, key, ob_get_clean, ob_start, phpinfo, reset,
    rtrim, sprintf, str_contains, str_replace, str_starts_with, strpos, strstr, strtolower, trim,
    version_compare,
};
use std::cell::RefCell;
use std::rc::Rc;

use crate::advisory::AuditConfig;
use crate::advisory::Auditor;
use crate::command::base_command::base_command_initialize;
use crate::command::{BaseCommand, BaseCommandData};
use crate::composer;
use crate::composer::ComposerHandle;
use crate::composer::PartialComposerHandle;
use crate::config::Config;
use crate::downloader::TransportException;
use crate::factory::Factory;
use crate::filter::platform_requirement_filter::PlatformRequirementFilterInterface;
use crate::io::BufferIO;
use crate::io::IOInterface;
use crate::io::IOInterfaceImmutable;
use crate::io::NullIO;
use crate::json::JsonFile;
use crate::json::JsonValidationException;
use crate::package::CompletePackageInterface;
use crate::package::Locker;
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

#[derive(Debug)]
pub struct DiagnoseCommand {
    base_command_data: BaseCommandData,

    pub(crate) http_downloader: Option<std::rc::Rc<std::cell::RefCell<HttpDownloader>>>,
    pub(crate) process: Option<std::rc::Rc<std::cell::RefCell<ProcessExecutor>>>,
    pub(crate) exit_code: i64,
}

impl Default for DiagnoseCommand {
    fn default() -> Self {
        Self::new()
    }
}

impl DiagnoseCommand {
    pub fn new() -> Self {
        let mut command = DiagnoseCommand {
            base_command_data: BaseCommandData::new(None),
            http_downloader: None,
            process: None,
            exit_code: 0,
        };
        command
            .configure()
            .expect("DiagnoseCommand::configure uses static, valid metadata");
        command
    }
}

impl Command for DiagnoseCommand {
    fn configure(&mut self) -> anyhow::Result<()> {
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
        &mut self,
        input: std::rc::Rc<std::cell::RefCell<dyn InputInterface>>,
        output: std::rc::Rc<std::cell::RefCell<dyn OutputInterface>>,
    ) -> anyhow::Result<i64> {
        let mut composer = self.try_composer(None, None);
        let io: std::rc::Rc<std::cell::RefCell<dyn IOInterface>> = self.get_io().clone();

        let config: std::rc::Rc<std::cell::RefCell<Config>>;
        if let Some(ref mut c) = composer {
            let c = crate::command::composer_full(c);
            config = c.get_config().clone();

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
            self.process = Some(
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

            self.process = Some(std::rc::Rc::new(std::cell::RefCell::new(
                ProcessExecutor::new(Some(io.clone())),
            )));
        }
        let mut config_inner = IndexMap::new();
        config_inner.insert("secure-http".to_string(), PhpMixed::Bool(false));
        let mut secure_http_wrap: IndexMap<String, PhpMixed> = IndexMap::new();
        secure_http_wrap.insert("config".to_string(), PhpMixed::Array(config_inner));
        let mut config = config;
        config
            .borrow_mut()
            .merge(&secure_http_wrap, Config::SOURCE_COMMAND);
        let _ = config.borrow_mut().prohibit_url_by_config(
            "http://repo.packagist.org",
            Some(std::rc::Rc::new(std::cell::RefCell::new(NullIO::new()))),
            &IndexMap::new(),
        );

        self.http_downloader = Some(std::rc::Rc::new(std::cell::RefCell::new(
            Factory::create_http_downloader(io.clone(), &config, indexmap::IndexMap::new())?,
        )));

        if strpos(file!(), "phar:") == Some(0) {
            io.write_no_newline("Checking pubkeys: ");
            let r = self.check_pub_keys(&config.borrow())?;
            self.output_result(r);

            io.write_no_newline("Checking Composer version: ");
            let r = self.check_version(&config)?;
            self.output_result(r);
        }

        io.write(&format!(
            "Composer version: <comment>{}</comment>",
            composer::get_version()
        ));

        io.write_no_newline("Checking Composer and its dependencies for vulnerabilities: ");
        let r = self.check_composer_audit(&config.borrow())?;
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
            PlatformRepository::new(vec![], platform_overrides_unboxed).unwrap();
        let php_pkg = <PlatformRepository as crate::repository::RepositoryInterface>::find_package(
            &mut platform_repo,
            "php",
            crate::repository::FindPackageConstraint::String("*".to_string()),
        )?
        .unwrap();
        let mut php_version = php_pkg.get_pretty_version().to_string();
        if let Some(cp) = php_pkg.as_complete()
            && str_contains(&cp.get_description().unwrap_or_default(), "overridden")
        {
            php_version = format!(
                "{} - {}",
                php_version,
                cp.get_description().unwrap_or_default()
            );
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

        if let Some(ref mut c) = composer {
            let mut c = crate::command::composer_full_mut(c);
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
                let r = self.check_composer_lock_schema(&locker)?;
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
        let r = self.check_http("http", &config.borrow())?;
        self.output_result(r);

        io.write_no_newline("Checking https connectivity to packagist: ");
        let r = self.check_http("https", &config.borrow())?;
        self.output_result(r);

        for repo in config.borrow().get_repositories() {
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
                    self.http_downloader.clone().unwrap(),
                    None,
                )
                .unwrap();
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
                let r = self.check_composer_repo(&url, &config.borrow())?;
                self.output_result(r);
            }
        }

        let proxy_manager = ProxyManager::get_instance();
        let protos: Vec<&str> = if config.borrow_mut().get("disable-tls").as_bool() == Some(true) {
            vec!["http"]
        } else {
            vec!["http", "https"]
        };
        let proxy_check_result: Result<(), anyhow::Error> = (|| -> anyhow::Result<()> {
            for proto in &protos {
                let proxy = proxy_manager
                    .lock()
                    .unwrap()
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
            if let Some(_te) = e.downcast_ref::<TransportException>() {
                io.write_no_newline("Checking HTTP proxy: ");
                let status = self.check_connectivity_and_composer_network_http_enablement();
                self.output_result(if is_string(&status) {
                    status
                } else {
                    PhpMixed::String(format!("<error>[{}] {}</error>", get_class_err(&e), e))
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
                Ok(rate) => {
                    if !is_array(&rate) {
                        self.output_result(rate);
                    } else if let Some(arr) = rate.as_array() {
                        let remaining = arr.get("remaining").and_then(|v| v.as_int()).unwrap_or(0);
                        let limit = arr.get("limit").and_then(|v| v.as_int()).unwrap_or(0);
                        if 10 > remaining {
                            io.write("<warning>WARNING</warning>");
                            io.write(&format!(
                                "<comment>GitHub has a rate limit on their API. You currently have <options=bold>{}</options=bold> out of <options=bold>{}</options=bold> requests left.\nSee https://developer.github.com/v3/#rate-limiting and also\n    https://getcomposer.org/doc/articles/troubleshooting.md#api-rate-limit-and-oauth-tokens</comment>",
                                remaining, limit,
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
                                get_class_err(&e),
                                e
                            )));
                        }
                    } else {
                        self.output_result(PhpMixed::String(format!(
                            "<error>[{}] {}</error>",
                            get_class_err(&e),
                            e
                        )));
                    }
                }
            }
        }

        io.write_no_newline("Checking disk free space: ");
        let r = self.check_disk_space(&config.borrow());
        self.output_result(r);

        Ok(self.exit_code)
    }

    fn initialize(
        &mut self,
        input: Rc<RefCell<dyn InputInterface>>,
        output: Rc<RefCell<dyn OutputInterface>>,
    ) -> anyhow::Result<()> {
        base_command_initialize(self, input, output)
    }

    shirabe_external_packages::delegate_command_trait_impls_to_inner!(base_command_data);
}

impl BaseCommand for DiagnoseCommand {
    fn command_data_mut(
        &mut self,
    ) -> &mut shirabe_external_packages::symfony::console::command::command::CommandData {
        self.base_command_data.command_data_mut()
    }

    crate::delegate_base_command_trait_impls_to_inner!(base_command_data);
}

impl DiagnoseCommand {
    fn check_composer_schema(&mut self) -> anyhow::Result<PhpMixed> {
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

            return Ok(PhpMixed::String(rtrim(&output, Some(" \t\n\r\0\u{0B}"))));
        }

        Ok(PhpMixed::Bool(true))
    }

    fn check_composer_lock_schema(&self, locker: &Locker) -> anyhow::Result<PhpMixed> {
        let json = locker.get_json_file();

        match json.validate_schema(JsonFile::LOCK_SCHEMA, None) {
            Ok(_) => {}
            Err(e) => {
                if let Some(jve) = e.downcast_ref::<JsonValidationException>() {
                    let mut output = String::new();
                    for error in jve.get_errors() {
                        output.push_str(&format!("<error>{}</error>{}", error, PHP_EOL));
                    }

                    return Ok(PhpMixed::String(trim(&output, Some(" \t\n\r\0\u{0B}"))));
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
        let _ = self.process.as_mut().unwrap().borrow_mut().execute(
            vec![
                "git".to_string(),
                "config".to_string(),
                "color.ui".to_string(),
            ],
            &mut output,
            (),
        );
        if strtolower(&trim(&output, Some(" \t\n\r\0\u{0B}"))) == "always" {
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

        let mut result_list: Vec<PhpMixed> = vec![];
        let mut tls_warning: Option<String> = None;
        if proto == "https" && config.get("disable-tls").as_bool() == Some(true) {
            tls_warning = Some("<warning>Composer is configured to disable SSL/TLS protection. This will leave remote HTTPS requests vulnerable to Man-In-The-Middle attacks.</warning>".to_string());
        }

        match self.http_downloader.as_ref().unwrap().borrow_mut().get(
            &format!("{}://repo.packagist.org/packages.json", proto),
            IndexMap::new(),
        ) {
            Ok(_) => {}
            Err(e) => {
                if let Some(te) = e.downcast_ref::<TransportException>() {
                    let hints = HttpDownloader::get_exception_hints(&e).unwrap_or_default();
                    if !hints.is_empty() {
                        for hint in hints {
                            result_list.push(PhpMixed::String(hint));
                        }
                    }

                    result_list.push(PhpMixed::String(format!(
                        "<error>[{}] {}</error>",
                        std::any::type_name_of_val(te),
                        te.message
                    )));
                } else {
                    return Err(e);
                }
            }
        }

        if let Some(w) = tls_warning {
            result_list.push(PhpMixed::String(w));
        }

        if !result_list.is_empty() {
            return Ok(PhpMixed::List(result_list));
        }

        Ok(PhpMixed::Bool(true))
    }

    fn check_composer_repo(&mut self, url: &str, config: &Config) -> anyhow::Result<PhpMixed> {
        let result = self.check_connectivity_and_composer_network_http_enablement();
        if result.as_bool() != Some(true) {
            return Ok(result);
        }

        let mut result_list: Vec<PhpMixed> = vec![];
        let mut tls_warning: Option<String> = None;
        if str_starts_with(url, "https://") && config.get("disable-tls").as_bool() == Some(true) {
            tls_warning = Some("<warning>Composer is configured to disable SSL/TLS protection. This will leave remote HTTPS requests vulnerable to Man-In-The-Middle attacks.</warning>".to_string());
        }

        match self
            .http_downloader
            .as_ref()
            .unwrap()
            .borrow_mut()
            .get(url, IndexMap::new())
        {
            Ok(_) => {}
            Err(e) => {
                if let Some(te) = e.downcast_ref::<TransportException>() {
                    let hints = HttpDownloader::get_exception_hints(&e).unwrap_or_default();
                    if !hints.is_empty() {
                        for hint in hints {
                            result_list.push(PhpMixed::String(hint));
                        }
                    }

                    result_list.push(PhpMixed::String(format!(
                        "<error>[{}] {}</error>",
                        std::any::type_name_of_val(te),
                        te.message
                    )));
                } else {
                    return Err(e);
                }
            }
        }

        if let Some(w) = tls_warning {
            result_list.push(PhpMixed::String(w));
        }

        if !result_list.is_empty() {
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

        let proxy_status = proxy.get_status(None).unwrap_or_default();

        if proxy.is_excluded_by_no_proxy() {
            return Ok(PhpMixed::String(format!(
                "<info>SKIP</> <comment>Because repo.packagist.org is {}</>",
                proxy_status
            )));
        }

        let json = self
            .http_downloader
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
                .map(|v| v.clone())
                .unwrap_or(PhpMixed::Null);
            let hash_val = first
                .as_array()
                .and_then(|a| a.get("sha256"))
                .map(|v| v.clone())
                .unwrap_or(PhpMixed::Null);
            let path = str_replace(
                "%hash%",
                hash_val.as_string().unwrap_or(""),
                &key(provider_includes
                    .as_array()
                    .cloned()
                    .unwrap_or_default()
                    .into())
                .unwrap_or_default(),
            );
            let response = self.http_downloader.as_ref().unwrap().borrow_mut().get(
                &format!("{}://repo.packagist.org/{}", protocol, path),
                IndexMap::new(),
            )?;
            let provider = response.get_body().unwrap_or_default().to_string();

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
            .as_ref()
            .unwrap()
            .borrow_mut()
            .get(&url, opts)
        {
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
                if let Some(te) = e.downcast_ref::<TransportException>()
                    && te.get_code() == 401
                {
                    return Ok(PhpMixed::String(format!(
                        "<comment>The oauth token for {} seems invalid, run \"composer config --global --unset github-oauth.{}\" to remove it</comment>",
                        domain, domain
                    )));
                }
                Ok(PhpMixed::String(format!(
                    "<error>[{}] {}</error>",
                    get_class_err(&e),
                    e
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
            .map(|v| v.clone())
            .unwrap_or(PhpMixed::Null))
    }

    fn check_disk_space(&self, config: &Config) -> PhpMixed {
        if !function_exists("disk_free_space") {
            return PhpMixed::Bool(true);
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
            return PhpMixed::String(format!("<error>The disk hosting {} is full</error>", dir));
        }
        dir = vendor_dir.clone();
        let df_vendor = disk_free_space(&vendor_dir);
        if df_vendor.map(|d| d < min_space_free).unwrap_or(false) {
            return PhpMixed::String(format!("<error>The disk hosting {} is full</error>", dir));
        }

        PhpMixed::Bool(true)
    }

    fn check_pub_keys(&mut self, config: &Config) -> anyhow::Result<PhpMixed> {
        let home = config.get("home").as_string().unwrap_or("").to_string();
        let mut errors: Vec<PhpMixed> = vec![];
        let io = self.get_io();

        if file_exists(&format!("{}/keys.tags.pub", home))
            && file_exists(&format!("{}/keys.dev.pub", home))
        {
            io.write("");
        }

        if file_exists(&format!("{}/keys.tags.pub", home)) {
            io.write(&format!(
                "Tags Public Key Fingerprint: {}",
                Keys::fingerprint(&format!("{}/keys.tags.pub", home))?
            ));
        } else {
            errors.push(PhpMixed::String(
                "<error>Missing pubkey for tags verification</error>".to_string(),
            ));
        }

        if file_exists(&format!("{}/keys.dev.pub", home)) {
            io.write(&format!(
                "Dev Public Key Fingerprint: {}",
                Keys::fingerprint(&format!("{}/keys.dev.pub", home))?
            ));
        } else {
            errors.push(PhpMixed::String(
                "<error>Missing pubkey for dev verification</error>".to_string(),
            ));
        }

        if !errors.is_empty() {
            errors.push(PhpMixed::String(
                "<error>Run composer self-update --update-keys to set them up</error>".to_string(),
            ));
        }

        Ok(if !errors.is_empty() {
            PhpMixed::List(errors)
        } else {
            PhpMixed::Bool(true)
        })
    }

    fn check_version(
        &mut self,
        config: &std::rc::Rc<std::cell::RefCell<Config>>,
    ) -> anyhow::Result<PhpMixed> {
        let result = self.check_connectivity_and_composer_network_http_enablement();
        if result.as_bool() != Some(true) {
            return Ok(result);
        }

        let mut versions_util =
            Versions::new(config.clone(), self.http_downloader.clone().unwrap());
        let latest = match versions_util.get_latest(None) {
            Ok(Ok(l)) => l,
            Ok(Err(e)) => {
                return Ok(PhpMixed::String(format!(
                    "<error>[{}] {}</error>",
                    "UnexpectedValueException", e.message
                )));
            }
            Err(e) => {
                return Ok(PhpMixed::String(format!(
                    "<error>[{}] {}</error>",
                    get_class_err(&e),
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
            return Ok(PhpMixed::String(format!(
                "<comment>You are not running the latest {} version, run `composer self-update` to update ({} => {})</comment>",
                versions_util.get_channel()?,
                composer::VERSION,
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

        let auditor = Auditor;
        let mut repo_set = RepositorySet::new(
            "stable",
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
        )?;
        if !installed_json.exists() {
            return Ok(PhpMixed::String("<warning>Could not find Composer's installed.json, this must be a non-standard Composer installation.</>".to_string()));
        }

        let local_repo = FilesystemRepository::new(installed_json, false, None, None)?;
        let version = composer::get_version();
        let mut packages = local_repo.inner.get_canonical_packages();
        if version != "@package_version@" {
            let version_parser = VersionParser::new();
            let normalized_version = version_parser.normalize(&version, None)?;
            let root_pkg = RootPackage::new(
                "composer/composer".to_string(),
                normalized_version,
                version.clone(),
            );
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
                config,
                self.http_downloader.clone().unwrap(),
                None,
            )?);
        repo_set.add_repository(composer_repo_as_repo)?;

        let mut io = BufferIO::new(String::new(), 0, None)?;
        let result = match auditor.audit(
            &mut io,
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
                return Ok(PhpMixed::String(format!(
                    "<highlight>Failed performing audit: {}</>",
                    e
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
            let version_arr = version.unwrap_or_default();
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
        let prev_exit_code = self.exit_code;
        let io = self.get_io();
        if result.as_bool() == Some(true) {
            io.write("<info>OK</info>");

            return;
        }

        let mut had_error = false;
        let mut had_warning = false;
        let mut result = result;
        // PHP: $result instanceof \Exception → already converted to string at call sites here
        if !result.as_bool().unwrap_or(true) && result.as_string().is_none() && !is_array(&result) {
            // falsey results should be considered as an error, even if there is nothing to output
            had_error = true;
        } else {
            let result_list: Vec<PhpMixed> = match &result {
                PhpMixed::List(l) => l.clone(),
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
            result = PhpMixed::List(result_list);
        }

        if had_error {
            io.write("<error>FAIL</error>");
        } else if had_warning {
            io.write("<warning>WARNING</warning>");
        }

        if !result.as_bool().unwrap_or(false) {
            // PHP: if ($result) — falsey skips; this branch matches truthy
        }
        if let Some(list) = result.as_list() {
            for message in list {
                io.write(&trim(
                    message.as_string().unwrap_or(""),
                    Some(" \t\n\r\0\u{0B}"),
                ));
            }
        }
        // Apply exit code updates after io borrow ends
        if had_error {
            self.exit_code = prev_exit_code.max(2);
        } else if had_warning {
            self.exit_code = prev_exit_code.max(1);
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

        if !filter_var_boolean(ini_get("allow_url_fopen").as_deref().unwrap_or("")) {
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
            && filter_var_boolean(ini_get("apc.enable_cli").as_deref().unwrap_or(""))
        {
            warnings.insert("apc_cli".to_string(), PhpMixed::Bool(true));
        }

        if !extension_loaded("zlib") {
            warnings.insert("zlib".to_string(), PhpMixed::Bool(true));
        }

        ob_start();
        phpinfo(INFO_GENERAL);
        let phpinfo_str = ob_get_clean();
        let mut phpinfo_match: IndexMap<CaptureKey, String> = IndexMap::new();
        if phpinfo_str.is_some()
            && Preg::is_match3(
                "{Configure Command(?: *</td><td class=\"v\">| *=> *)(.*?)(?:</td>|$)}m",
                phpinfo_str.as_ref().unwrap(),
                Some(&mut phpinfo_match),
            )
        {
            let configure = phpinfo_match
                .get(&CaptureKey::ByIndex(1))
                .cloned()
                .unwrap_or_default();
            let configure = configure.as_str();

            if str_contains(configure, "--enable-sigchild") {
                warnings.insert("sigchild".to_string(), PhpMixed::Bool(true));
            }

            if str_contains(configure, "--with-curlwrappers") {
                warnings.insert("curlwrappers".to_string(), PhpMixed::Bool(true));
            }
        }

        if filter_var_boolean(ini_get("xdebug.profiler_enabled").as_deref().unwrap_or("")) {
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
            && !(filter_var_boolean(ini_get("uopz.disable").as_deref().unwrap_or(""))
                || filter_var_boolean(ini_get("uopz.exit").as_deref().unwrap_or("")))
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
                            message: format!(
                                "DiagnoseCommand: Unknown error type \"{}\". Please report at https://github.com/composer/composer/issues/new.",
                                other,
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
                        let openssl_trimmed = trim(
                            &strstr(OPENSSL_VERSION_TEXT, " ").unwrap_or_default(),
                            Some(" \t\n\r\0\u{0B}"),
                        );
                        let mut openssl_version = strstr(&openssl_trimmed, " ").unwrap_or_default();
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
                            message: format!(
                                "DiagnoseCommand: Unknown warning type \"{}\". Please report at https://github.com/composer/composer/issues/new.",
                                other,
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
        if ["4".to_string(), "6".to_string()].contains(&composer_ipresolve) {
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

        Ok(if warnings.is_empty() && errors.is_empty() {
            PhpMixed::Bool(true)
        } else {
            PhpMixed::String(output)
        })
    }

    /// Check if allow_url_fopen is ON
    fn check_connectivity(&self) -> PhpMixed {
        if !ini_get("allow_url_fopen")
            .as_deref()
            .and_then(|s| s.parse::<bool>().ok())
            .unwrap_or(false)
            && ini_get("allow_url_fopen").as_deref() != Some("1")
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
