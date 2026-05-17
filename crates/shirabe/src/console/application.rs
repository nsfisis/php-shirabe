//! ref: composer/src/Composer/Console/Application.php

use crate::io::io_interface;
use indexmap::IndexMap;

use shirabe_external_packages::composer::xdebug_handler::xdebug_handler::XdebugHandler;
use shirabe_external_packages::seld::json_lint::parsing_exception::ParsingException;
use shirabe_external_packages::symfony::component::console::application::Application as BaseApplication;
use shirabe_external_packages::symfony::component::console::command::command::Command;
use shirabe_external_packages::symfony::component::console::exception::command_not_found_exception::CommandNotFoundException;
use shirabe_external_packages::symfony::component::console::exception::exception_interface::ExceptionInterface;
use shirabe_external_packages::symfony::component::console::helper::helper_set::HelperSet;
use shirabe_external_packages::symfony::component::console::helper::question_helper::QuestionHelper;
use shirabe_external_packages::symfony::component::console::input::input_definition::InputDefinition;
use shirabe_external_packages::symfony::component::console::input::input_interface::InputInterface;
use shirabe_external_packages::symfony::component::console::input::input_option::InputOption;
use shirabe_external_packages::symfony::component::console::output::console_output_interface::ConsoleOutputInterface;
use shirabe_external_packages::symfony::component::console::output::output_interface::OutputInterface;
use shirabe_external_packages::symfony::component::console::single_command_application::SingleCommandApplication;
use shirabe_external_packages::symfony::component::process::exception::process_timed_out_exception::ProcessTimedOutException;
use shirabe_php_shim::{
    array_merge, bin2hex, chdir, clone, count, date_default_timezone_get,
    date_default_timezone_set, defined, dirname, disk_free_space, error_get_last,
    extension_loaded, file_exists, file_get_contents, file_put_contents, function_exists, get_class,
    getcwd, getmypid, glob, ini_set, in_array, is_array, is_dir, is_file, is_string,
    is_subclass_of, json_decode, max_i64, memory_get_peak_usage, memory_get_usage, microtime,
    method_exists, php_uname, posix_getuid, random_bytes, realpath, register_shutdown_function,
    restore_error_handler, round, sprintf, str_contains, str_replace, strpos, strtoupper,
    sys_get_temp_dir, time, unlink, PhpMixed, RuntimeException, UnexpectedValueException,
    LogicException as ShimLogicException,
    PHP_BINARY, PHP_VERSION, PHP_VERSION_ID,
};

use crate::command::about_command::AboutCommand;
use crate::command::archive_command::ArchiveCommand;
use crate::command::audit_command::AuditCommand;
use crate::command::base_command::BaseCommand;
use crate::command::bump_command::BumpCommand;
use crate::command::check_platform_reqs_command::CheckPlatformReqsCommand;
use crate::command::clear_cache_command::ClearCacheCommand;
use crate::command::config_command::ConfigCommand;
use crate::command::create_project_command::CreateProjectCommand;
use crate::command::depends_command::DependsCommand;
use crate::command::diagnose_command::DiagnoseCommand;
use crate::command::dump_autoload_command::DumpAutoloadCommand;
use crate::command::exec_command::ExecCommand;
use crate::command::fund_command::FundCommand;
use crate::command::global_command::GlobalCommand;
use crate::command::home_command::HomeCommand;
use crate::command::init_command::InitCommand;
use crate::command::install_command::InstallCommand;
use crate::command::licenses_command::LicensesCommand;
use crate::command::outdated_command::OutdatedCommand;
use crate::command::prohibits_command::ProhibitsCommand;
use crate::command::reinstall_command::ReinstallCommand;
use crate::command::remove_command::RemoveCommand;
use crate::command::repository_command::RepositoryCommand;
use crate::command::require_command::RequireCommand;
use crate::command::run_script_command::RunScriptCommand;
use crate::command::script_alias_command::ScriptAliasCommand;
use crate::command::search_command::SearchCommand;
use crate::command::self_update_command::SelfUpdateCommand;
use crate::command::show_command::ShowCommand;
use crate::command::status_command::StatusCommand;
use crate::command::suggests_command::SuggestsCommand;
use crate::command::update_command::UpdateCommand;
use crate::command::validate_command::ValidateCommand;
use crate::composer::Composer;
use crate::console::github_action_error::GithubActionError;
use crate::downloader::transport_exception::TransportException;
use crate::event_dispatcher::script_execution_exception::ScriptExecutionException;
use crate::exception::no_ssl_exception::NoSslException;
use crate::factory::Factory;
use crate::installer::Installer;
use crate::io::console_io::ConsoleIO;
use crate::io::io_interface::IOInterface;
use crate::io::null_io::NullIO;
use crate::json::json_validation_exception::JsonValidationException;
use crate::util::error_handler::ErrorHandler;
use crate::util::filesystem::Filesystem;
use crate::util::http_downloader::HttpDownloader;
use crate::util::platform::Platform;
use crate::util::silencer::Silencer;

#[derive(Debug)]
pub struct Application {
    inner: BaseApplication,
    pub(crate) composer: Option<Composer>,
    pub(crate) io: Box<dyn IOInterface>,
    has_plugin_commands: bool,
    disable_plugins_by_default: bool,
    disable_scripts_by_default: bool,
    /// Store the initial working directory at startup time
    initial_working_directory: Option<String>,
}

impl Application {
    const LOGO: &'static str = "   ______\n  / ____/___  ____ ___  ____  ____  ________  _____\n / /   / __ \\/ __ `__ \\/ __ \\/ __ \\/ ___/ _ \\/ ___/\n/ /___/ /_/ / / / / / / /_/ / /_/ (__  )  __/ /\n\\____/\\____/_/ /_/ /_/ .___/\\____/____/\\___/_/\n                    /_/\n";

    pub fn new(name: String, mut version: String) -> Self {
        let mut inner = BaseApplication::new(name.clone(), version.clone());
        if method_exists(&inner, "setCatchErrors") {
            inner.set_catch_errors(true);
        }

        // PHP: static $shutdownRegistered = false; — register only once globally
        static SHUTDOWN_REGISTERED: std::sync::OnceLock<()> = std::sync::OnceLock::new();
        if version == "" {
            version = Composer::get_version();
        }
        if function_exists("ini_set") && extension_loaded("xdebug") {
            ini_set("xdebug.show_exception_trace", "0");
            ini_set("xdebug.scream", "0");
        }

        if function_exists("date_default_timezone_set")
            && function_exists("date_default_timezone_get")
        {
            let tz = Silencer::call(|| Ok(date_default_timezone_get())).unwrap_or_default();
            date_default_timezone_set(&tz);
        }

        let io: Box<dyn IOInterface> = Box::new(NullIO::new());

        SHUTDOWN_REGISTERED.get_or_init(|| {
            register_shutdown_function(Box::new(|| {
                let last_error = error_get_last();

                let message = last_error
                    .get("message")
                    .and_then(|v| v.as_string())
                    .unwrap_or("");
                if !message.is_empty()
                    && (strpos(message, "Allowed memory").is_some()
                        || strpos(message, "exceeded memory").is_some())
                {
                    println!("\nCheck https://getcomposer.org/doc/articles/troubleshooting.md#memory-limit-errors for more info on how to handle out of memory errors.");
                }
            }));
        });

        let initial_working_directory = getcwd();

        // PHP: parent::__construct($name, $version);
        // BaseApplication is mock-imported; assume new(name, version) above also recorded the version.
        let _ = (name, version);

        Self {
            inner,
            composer: None,
            io,
            has_plugin_commands: false,
            disable_plugins_by_default: false,
            disable_scripts_by_default: false,
            initial_working_directory,
        }
    }

    pub fn run(
        &mut self,
        input: Option<&dyn InputInterface>,
        output: Option<&dyn OutputInterface>,
    ) -> anyhow::Result<i64> {
        let output_owned: Box<dyn OutputInterface>;
        let output_ref: &dyn OutputInterface = if let Some(o) = output {
            o
        } else {
            output_owned = Factory::create_output();
            &*output_owned
        };

        self.inner.run(input, Some(output_ref))
    }

    pub fn do_run(
        &mut self,
        input: &mut dyn InputInterface,
        output: &dyn OutputInterface,
    ) -> anyhow::Result<i64> {
        self.disable_plugins_by_default = input.has_parameter_option("--no-plugins", false);
        self.disable_scripts_by_default = input.has_parameter_option("--no-scripts", false);

        // PHP: static $stdin = null;
        // We use an Option here to mimic the lazy initialization.
        static STDIN: std::sync::OnceLock<Option<shirabe_php_shim::PhpResource>> =
            std::sync::OnceLock::new();
        let stdin = STDIN.get_or_init(|| {
            if defined("STDIN") {
                Some(shirabe_php_shim::stdin_handle())
            } else {
                shirabe_php_shim::fopen("php://stdin", "r")
            }
        });
        if Platform::get_env("COMPOSER_TESTS_ARE_RUNNING").as_deref() != Some("1")
            && (Platform::get_env("COMPOSER_NO_INTERACTION").is_some()
                || stdin.is_none()
                || !Platform::is_tty(stdin.as_ref().unwrap()))
        {
            input.set_interactive(false);
        }

        let mut helpers: Vec<
            Box<dyn shirabe_external_packages::symfony::component::console::helper::helper::Helper>,
        > = vec![];
        helpers.push(Box::new(QuestionHelper::new()));
        let console_io = ConsoleIO::new(input, output, HelperSet::new(helpers));
        self.io = Box::new(console_io);
        let io = &mut *self.io;

        // Register error handler again to pass it the IO instance
        ErrorHandler::register(Some(io));

        if input.has_parameter_option("--no-cache", false) {
            io.write_error("Disabling cache usage", true, io_interface::DEBUG);
            Platform::put_env(
                "COMPOSER_CACHE_DIR",
                if Platform::is_windows() {
                    "nul"
                } else {
                    "/dev/null"
                },
            );
        }

        // switch working dir
        let new_work_dir = self.get_new_working_dir(input)?;
        let mut old_working_dir: Option<String> = None;
        if let Some(ref nwd) = new_work_dir {
            old_working_dir = Some(Platform::get_cwd_real(true));
            chdir(nwd);
            self.initial_working_directory = getcwd();
            let cwd = Platform::get_cwd_real(true);
            io.write_error(
                &format!(
                    "Changed CWD to {}",
                    if !cwd.is_empty() {
                        cwd.clone()
                    } else {
                        nwd.clone()
                    }
                ),
                true,
                io_interface::DEBUG,
            );
        }

        // determine command name to be executed without including plugin commands
        let mut command_name: Option<String> = Some(String::new());
        let raw_command_name = self.get_command_name_before_binding(input);
        if let Some(ref raw) = raw_command_name {
            match self.inner.find(raw) {
                Ok(cmd) => command_name = Some(cmd.get_name()),
                Err(e) => {
                    if e.downcast_ref::<CommandNotFoundException>().is_some() {
                        // we'll check command validity again later after plugins are loaded
                        command_name = None;
                    }
                    // PHP also catches \InvalidArgumentException here without action
                }
            }
        }

        // prompt user for dir change if no composer.json is present in current dir
        let no_composer_json_commands = vec![
            "".to_string(),
            "list".to_string(),
            "init".to_string(),
            "about".to_string(),
            "help".to_string(),
            "diagnose".to_string(),
            "self-update".to_string(),
            "global".to_string(),
            "create-project".to_string(),
            "outdated".to_string(),
        ];
        let use_parent_dir_if_no_json_available = self.get_use_parent_dir_config_value();
        if new_work_dir.is_none()
            && !in_array(
                command_name.as_deref().unwrap_or(""),
                &no_composer_json_commands,
                true,
            )
            && !file_exists(&Factory::get_composer_file())
            && use_parent_dir_if_no_json_available.as_bool() != Some(false)
            && (command_name.as_deref() != Some("config")
                || (input.has_parameter_option("--file", true) == false
                    && input.has_parameter_option("-f", true) == false))
            && input.has_parameter_option("--help", true) == false
            && input.has_parameter_option("-h", true) == false
        {
            let mut dir = dirname(&Platform::get_cwd_real(true));
            let home_value = Platform::get_env("HOME")
                .or_else(|| Platform::get_env("USERPROFILE"))
                .unwrap_or_else(|| "/".to_string());
            let home = realpath(&home_value).unwrap_or_default();

            // abort when we reach the home dir or top of the filesystem
            while dirname(&dir) != dir && dir != home {
                if file_exists(&format!("{}/{}", dir, Factory::get_composer_file())) {
                    if use_parent_dir_if_no_json_available.as_bool() != Some(true)
                        && !io.is_interactive()
                    {
                        io.write_error(&format!("<info>No composer.json in current directory, to use the one at {} run interactively or set config.use-parent-dir to true</info>", dir));
                        break;
                    }
                    if use_parent_dir_if_no_json_available.as_bool() == Some(true)
                        || io.ask_confirmation(format!("<info>No composer.json in current directory, do you want to use the one at {}?</info> [<comment>y,n</comment>]? ", dir), true)
                    {
                        if use_parent_dir_if_no_json_available.as_bool() == Some(true) {
                            io.write_error(&format!("<info>No composer.json in current directory, changing working directory to {}</info>", dir));
                        } else {
                            io.write_error("<info>Always want to use the parent dir? Use \"composer config --global use-parent-dir true\" to change the default.</info>");
                        }
                        old_working_dir = Some(Platform::get_cwd_real(true));
                        chdir(&dir);
                    }
                    break;
                }
                dir = dirname(&dir);
            }
            drop((dir, home));
        }

        let needs_sudo_check = !Platform::is_windows()
            && function_exists("exec")
            && Platform::get_env("COMPOSER_ALLOW_SUPERUSER").is_none()
            && !Platform::is_docker();
        let mut is_non_allowed_root = false;

        // Clobber sudo credentials if COMPOSER_ALLOW_SUPERUSER is not set before loading plugins
        if needs_sudo_check {
            is_non_allowed_root = self.is_running_as_root();

            if is_non_allowed_root {
                let uid: i64 = Platform::get_env("SUDO_UID")
                    .map(|v| v.parse().unwrap_or(0))
                    .unwrap_or(0);
                if uid != 0 {
                    // Silently clobber any sudo credentials on the invoking user to avoid privilege escalations later on
                    // ref. https://github.com/composer/composer/issues/5119
                    let _ = Silencer::call(|| {
                        shirabe_php_shim::exec(&format!(
                            "sudo -u \\#{} sudo -K > /dev/null 2>&1",
                            uid
                        ));
                        Ok(())
                    });
                }
            }

            // Silently clobber any remaining sudo leases on the current user as well to avoid privilege escalations
            let _ = Silencer::call(|| {
                shirabe_php_shim::exec("sudo -K > /dev/null 2>&1");
                Ok(())
            });
        }

        // avoid loading plugins/initializing the Composer instance earlier than necessary if no plugin command is needed
        // if showing the version, we never need plugin commands
        let may_need_plugin_command = !input
            .has_parameter_option_array(&vec!["--version".to_string(), "-V".to_string()], false)
            && (command_name.is_none()
                || in_array(
                    command_name.as_deref().unwrap_or(""),
                    &vec!["".to_string(), "list".to_string(), "help".to_string()],
                    true,
                )
                || (command_name.as_deref() == Some("_complete") && !is_non_allowed_root));

        let may_need_script_command = may_need_plugin_command
            || command_name.as_deref() == Some("run-script")
            || raw_command_name != command_name;

        if may_need_plugin_command && !self.disable_plugins_by_default && !self.has_plugin_commands
        {
            // at this point plugins are needed, so if we are running as root and it is not allowed we need to prompt
            // if interactive, and abort otherwise
            if is_non_allowed_root {
                io.write_error("<warning>Do not run Composer as root/super user! See https://getcomposer.org/root for details</warning>");

                if io.is_interactive()
                    && io.ask_confirmation(
                        "<info>Continue as root/super user</info> [<comment>yes</comment>]? "
                            .to_string(),
                        true,
                    )
                {
                    // avoid a second prompt later
                    is_non_allowed_root = false;
                } else {
                    io.write_error("<warning>Aborting as no plugin should be loaded if running as super user is not explicitly allowed</warning>");

                    return Ok(1);
                }
            }

            match (|| -> anyhow::Result<()> {
                for command in self.get_plugin_commands()? {
                    if self.inner.has(&command.get_name()) {
                        io.write_error(&format!("<warning>Plugin command {} ({}) would override a Composer command and has been skipped</warning>", command.get_name(), get_class(&*command)));
                    } else {
                        // Compatibility layer for symfony/console <7.4
                        if method_exists(&self.inner, "addCommand") {
                            self.inner.add_command(command);
                        } else {
                            self.inner.add(command);
                        }
                    }
                }
                Ok(())
            })() {
                Ok(_) => {}
                Err(e) => {
                    if e.downcast_ref::<NoSslException>().is_some() {
                        // suppress these as they are not relevant at this point
                    } else if let Some(pe) = e.downcast_ref::<ParsingException>() {
                        let details = pe.get_details();

                        let file = realpath(&Factory::get_composer_file());

                        let mut line: Option<i64> = None;
                        if !details.is_empty() {
                            if let Some(l) = details.get("line").and_then(|v| v.as_int()) {
                                line = Some(l);
                            }
                        }

                        let mut ghe = GithubActionError::new(self.io.clone_box());
                        ghe.emit(&pe.get_message(), file.as_deref(), line);

                        return Err(e);
                    } else {
                        return Err(e);
                    }
                }
            }

            self.has_plugin_commands = true;
        }

        if !self.disable_plugins_by_default && is_non_allowed_root && !io.is_interactive() {
            io.write_error("<error>Composer plugins have been disabled for safety in this non-interactive session.</error>");
            io.write_error("<error>Set COMPOSER_ALLOW_SUPERUSER=1 if you want to allow plugins to run as root/super user.</error>");
            self.disable_plugins_by_default = true;
        }

        // determine command name to be executed incl plugin commands, and check if it's a proxy command
        let mut is_proxy_command = false;
        if let Some(ref name) = self.get_command_name_before_binding(input) {
            if let Ok(command) = self.inner.find(name) {
                command_name = Some(command.get_name());
                is_proxy_command = command
                    .as_any()
                    .downcast_ref::<BaseCommand>()
                    .map(|bc| bc.is_proxy_command())
                    .unwrap_or(false);
            }
        }

        if !is_proxy_command {
            io.write_error(
                &sprintf(
                    "Running %s (%s) with %s on %s",
                    &[
                        Composer::get_version().into(),
                        Composer::RELEASE_DATE.into(),
                        (if defined("HHVM_VERSION") {
                            format!("HHVM {}", shirabe_php_shim::HHVM_VERSION)
                        } else {
                            format!("PHP {}", PHP_VERSION)
                        })
                        .into(),
                        (if function_exists("php_uname") {
                            format!("{} / {}", php_uname("s"), php_uname("r"))
                        } else {
                            "Unknown OS".to_string()
                        })
                        .into(),
                    ],
                ),
                true,
                io_interface::DEBUG,
            );

            if PHP_VERSION_ID < 70205 {
                io.write_error(&format!("<warning>Composer supports PHP 7.2.5 and above, you will most likely encounter problems with your PHP {}. Upgrading is strongly recommended but you can use Composer 2.2.x LTS as a fallback.</warning>", PHP_VERSION));
            }

            if XdebugHandler::is_xdebug_active()
                && Platform::get_env("COMPOSER_DISABLE_XDEBUG_WARN").is_none()
            {
                io.write_error("<warning>Composer is operating slower than normal because you have Xdebug enabled. See https://getcomposer.org/xdebug</warning>");
            }

            if defined("COMPOSER_DEV_WARNING_TIME")
                && command_name.as_deref() != Some("self-update")
                && command_name.as_deref() != Some("selfupdate")
                && time() > shirabe_php_shim::composer_dev_warning_time()
            {
                io.write_error(&sprintf(
                    "<warning>Warning: This development build of Composer is over 60 days old. It is recommended to update it by running \"%s self-update\" to get the latest version.</warning>",
                    &[shirabe_php_shim::server_get("PHP_SELF").unwrap_or_default().into()],
                ));
            }

            if is_non_allowed_root {
                if command_name.as_deref() != Some("self-update")
                    && command_name.as_deref() != Some("selfupdate")
                    && command_name.as_deref() != Some("_complete")
                {
                    io.write_error("<warning>Do not run Composer as root/super user! See https://getcomposer.org/root for details</warning>");

                    if io.is_interactive() {
                        if !io.ask_confirmation(
                            "<info>Continue as root/super user</info> [<comment>yes</comment>]? "
                                .to_string(),
                            true,
                        ) {
                            return Ok(1);
                        }
                    }
                }
            }

            // Check system temp folder for usability as it can cause weird runtime issues otherwise
            let _ = Silencer::call(|| {
                let pid = if function_exists("getmypid") {
                    format!("{}-", getmypid())
                } else {
                    String::new()
                };
                let tempfile = format!(
                    "{}/temp-{}{}",
                    sys_get_temp_dir(),
                    pid,
                    bin2hex(&random_bytes(5))
                );
                if !(file_put_contents(&tempfile, file!()) > 0
                    && file_get_contents(&tempfile).as_deref() == Some(file!())
                    && unlink(&tempfile)
                    && !file_exists(&tempfile))
                {
                    io.write_error(&sprintf("<error>PHP temp directory (%s) does not exist or is not writable to Composer. Set sys_temp_dir in your php.ini</error>", &[sys_get_temp_dir().into()]));
                }
                Ok(())
            });

            // add non-standard scripts as own commands
            let file = Factory::get_composer_file();
            if may_need_script_command && is_file(&file) && Filesystem::is_readable(&file) {
                let composer_json =
                    json_decode(&file_get_contents(&file).unwrap_or_default(), true);
                if let Some(arr) = composer_json.as_array() {
                    if let Some(scripts) = arr.get("scripts").and_then(|v| v.as_array()) {
                        for (script, dummy) in scripts {
                            let script_event_const = format!(
                                "Composer\\Script\\ScriptEvents::{}",
                                str_replace("-", "_", &strtoupper(script))
                            );
                            if !defined(&script_event_const) {
                                if self.inner.has(script) {
                                    io.write_error(&format!("<warning>A script named {} would override a Composer command and has been skipped</warning>", script));
                                } else {
                                    let mut description = format!(
                                        "Runs the {} script as defined in composer.json",
                                        script
                                    );

                                    if let Some(desc) = arr
                                        .get("scripts-descriptions")
                                        .and_then(|v| v.as_array())
                                        .and_then(|a| a.get(script))
                                        .and_then(|v| v.as_string())
                                    {
                                        description = desc.to_string();
                                    }

                                    let aliases: Vec<String> = arr
                                        .get("scripts-aliases")
                                        .and_then(|v| v.as_array())
                                        .and_then(|a| a.get(script))
                                        .and_then(|v| v.as_list())
                                        .map(|l| {
                                            l.iter()
                                                .filter_map(|v| {
                                                    v.as_string().map(|s| s.to_string())
                                                })
                                                .collect()
                                        })
                                        .unwrap_or_default();

                                    if let Some(composer) = self.get_composer(false, None, None)? {
                                        let root_package = composer.get_package();
                                        let generator = composer.get_autoload_generator();

                                        let package_map = generator.build_package_map(
                                            composer.get_installation_manager(),
                                            &*root_package,
                                            vec![],
                                        )?;
                                        let map = generator.parse_autoloads(
                                            &package_map,
                                            &*root_package,
                                            PhpMixed::Bool(false),
                                        );

                                        let loader = generator.create_loader(
                                            &map,
                                            composer
                                                .get_config()
                                                .get("vendor-dir")
                                                .as_string()
                                                .map(|s| s.to_string()),
                                        );
                                        loader.register(false);
                                    }

                                    // if the command is not an array of commands, and points to a valid Command subclass, import its details directly
                                    let dummy_str = dummy.as_string().unwrap_or("");
                                    let cmd: Box<dyn Command> = if is_string(dummy)
                                        && shirabe_php_shim::class_exists(dummy_str)
                                        && is_subclass_of(
                                            dummy_str,
                                            "Symfony\\Component\\Console\\Command\\Command",
                                        ) {
                                        if is_subclass_of(
                                            dummy_str,
                                            "Symfony\\Component\\Console\\SingleCommandApplication",
                                        ) {
                                            io.write_error(&format!("<warning>The script named {} extends SingleCommandApplication which is not compatible with Composer 2.9+, make sure you extend Symfony\\Component\\Console\\Command instead.</warning>", script));
                                        }
                                        let mut cmd = shirabe_php_shim::instantiate_class::<
                                            Box<dyn Command>,
                                        >(
                                            dummy_str,
                                            vec![PhpMixed::String(script.clone())],
                                        );
                                        let _ = SingleCommandApplication::class_name();

                                        // makes sure the command is find()'able by the name defined in composer.json, and the name isn't overridden in its configure()
                                        let name = cmd.get_name();
                                        if !name.is_empty() && name != *script {
                                            io.write_error(&format!("<warning>The script named {} in composer.json has a mismatched name in its class definition. For consistency, either use the same name, or do not define one inside the class.</warning>", script));
                                            cmd.set_name(script);
                                        }

                                        if cmd.get_description().is_empty()
                                            && is_string(&PhpMixed::String(description.clone()))
                                        {
                                            cmd.set_description(&description);
                                        }
                                        cmd
                                    } else {
                                        // fallback to usual aliasing behavior
                                        Box::new(ScriptAliasCommand::new(
                                            script.clone(),
                                            description.clone(),
                                            aliases,
                                        ))
                                    };

                                    // Compatibility layer for symfony/console <7.4
                                    if method_exists(&self.inner, "addCommand") {
                                        self.inner.add_command(cmd);
                                    } else {
                                        self.inner.add(cmd);
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }

        let mut start_time: Option<f64> = None;
        let result_outcome: anyhow::Result<i64> = (|| -> anyhow::Result<i64> {
            if input.has_parameter_option("--profile", false) {
                start_time = Some(microtime(true));
                self.io.enable_debugging(start_time.unwrap());
            }

            let result = self.inner.do_run(input, output)?;

            if input
                .has_parameter_option_array(&vec!["--version".to_string(), "-V".to_string()], true)
            {
                io.write_error(&sprintf(
                    "<info>PHP</info> version <comment>%s</comment> (%s)",
                    &[PHP_VERSION.into(), PHP_BINARY.into()],
                ));
                io.write_error(
                    "Run the \"diagnose\" command to get more detailed diagnostics output.",
                );
            }

            Ok(result)
        })();

        // chdir back to oldWorkingDir if set — runs regardless of result
        if let Some(ref owd) = old_working_dir {
            if !owd.is_empty() {
                let owd = owd.clone();
                let _ = Silencer::call(|| {
                    chdir(&owd);
                    Ok(())
                });
            }
        }

        if let Some(st) = start_time {
            io.write_error(&format!(
                "<info>Memory usage: {}MiB (peak: {}MiB), time: {}s</info>",
                round((memory_get_usage() as f64) / 1024.0 / 1024.0, 2),
                round((memory_get_peak_usage() as f64) / 1024.0 / 1024.0, 2),
                round(microtime(true) - st, 2)
            ));
        }

        let outcome = match result_outcome {
            Ok(r) => Ok(r),
            Err(e) => {
                if let Some(see) = e.downcast_ref::<ScriptExecutionException>() {
                    if self.get_disable_plugins_by_default()
                        && self.is_running_as_root()
                        && !self.io.is_interactive()
                    {
                        io.write_error("<error>Plugins have been disabled automatically as you are running as root, this may be the cause of the script failure.</error>", true, io_interface::QUIET);
                        io.write_error(
                            "<error>See also https://getcomposer.org/root</error>",
                            true,
                            io_interface::QUIET,
                        );
                    }

                    Ok(see.get_code())
                } else {
                    let mut ghe = GithubActionError::new(self.io.clone_box());
                    ghe.emit(&e.to_string(), None, None);

                    self.hint_common_errors(&e, output);

                    if !method_exists(&self.inner, "setCatchErrors") {
                        if let Some(coi) =
                            output.as_any().downcast_ref::<dyn ConsoleOutputInterface>()
                        {
                            self.inner.render_throwable(&e, coi.get_error_output());
                        } else {
                            self.inner.render_throwable(&e, output);
                        }

                        let code = e
                            .downcast_ref::<RuntimeException>()
                            .map(|r| r.code)
                            .unwrap_or(0);
                        return Ok(max_i64(1, code));
                    }

                    // override TransportException's code for the purpose of parent::run() using it as process exit code
                    // as http error codes are all beyond the 255 range of permitted exit codes
                    if e.downcast_ref::<TransportException>().is_some() {
                        // PHP: ReflectionProperty $reflProp = new \ReflectionProperty($e, 'code');
                        //      $reflProp->setValue($e, Installer::ERROR_TRANSPORT_EXCEPTION);
                        // TODO: reflection-based mutation of the existing exception is not portable;
                        // we surface the rewritten code via a fresh TransportException at the call site.
                        let _ = Installer::ERROR_TRANSPORT_EXCEPTION;
                    }

                    Err(e)
                }
            }
        };

        restore_error_handler();

        outcome
    }

    fn get_new_working_dir(&self, input: &dyn InputInterface) -> anyhow::Result<Option<String>> {
        let working_dir = input
            .get_parameter_option(
                &vec!["--working-dir".to_string(), "-d".to_string()],
                None,
                true,
            )
            .as_string()
            .map(|s| s.to_string());
        if let Some(ref wd) = working_dir {
            if !is_dir(wd) {
                return Err(RuntimeException {
                    message: format!(
                        "Invalid working directory specified, {} does not exist.",
                        wd
                    ),
                    code: 0,
                }
                .into());
            }
        }

        Ok(working_dir)
    }

    fn hint_common_errors(&self, exception: &anyhow::Error, output: &dyn OutputInterface) {
        let io = self.get_io();

        let is_logic_or_error = exception.downcast_ref::<ShimLogicException>().is_some();
        if is_logic_or_error && output.get_verbosity() < OutputInterface::VERBOSITY_VERBOSE {
            output.set_verbosity(OutputInterface::VERBOSITY_VERBOSE);
        }

        Silencer::suppress();
        let _ = (|| -> anyhow::Result<()> {
            let composer = self.get_composer(false, Some(true), None)?;
            if composer.is_some() && function_exists("disk_free_space") {
                let composer = composer.unwrap();
                let config = composer.get_config();

                let min_space_free: f64 = 100.0 * 1024.0 * 1024.0;
                let mut dir = config.get("home").as_string().unwrap_or("").to_string();
                let df = disk_free_space(&dir);
                let mut hit = df.map(|d| d < min_space_free).unwrap_or(false);
                if !hit {
                    dir = config
                        .get("vendor-dir")
                        .as_string()
                        .unwrap_or("")
                        .to_string();
                    let df = disk_free_space(&dir);
                    hit = df.map(|d| d < min_space_free).unwrap_or(false);
                }
                if !hit {
                    dir = sys_get_temp_dir();
                    let df = disk_free_space(&dir);
                    hit = df.map(|d| d < min_space_free).unwrap_or(false);
                }
                if hit {
                    io.write_error(&format!("<error>The disk hosting {} has less than 100MiB of free space, this may be the cause of the following exception</error>", dir), true, io_interface::QUIET);
                }
            }
            Ok(())
        })();
        Silencer::restore();

        let message = exception.to_string();
        if exception.downcast_ref::<TransportException>().is_some()
            && str_contains(&message, "Unable to use a proxy")
        {
            io.write_error(
                "<error>The following exception indicates your proxy is misconfigured</error>",
                true,
                io_interface::QUIET,
            );
            io.write_error("<error>Check https://getcomposer.org/doc/faqs/how-to-use-composer-behind-a-proxy.md for details</error>", true, io_interface::QUIET);
        }

        if Platform::is_windows()
            && exception.downcast_ref::<TransportException>().is_some()
            && str_contains(&message, "unable to get local issuer certificate")
        {
            let avast_detect = glob("C:\\Program Files\\Avast*");
            if is_array(&PhpMixed::List(
                avast_detect
                    .iter()
                    .map(|s| Box::new(PhpMixed::String(s.clone())))
                    .collect(),
            )) && count(&avast_detect) != 0
            {
                io.write_error("<error>The following exception indicates a possible issue with the Avast Firewall</error>", true, io_interface::QUIET);
                io.write_error(
                    "<error>Check https://getcomposer.org/local-issuer for details</error>",
                    true,
                    io_interface::QUIET,
                );
            } else {
                io.write_error("<error>The following exception indicates a possible issue with a Firewall/Antivirus</error>", true, io_interface::QUIET);
                io.write_error(
                    "<error>Check https://getcomposer.org/local-issuer for details</error>",
                    true,
                    io_interface::QUIET,
                );
            }
        }

        if Platform::is_windows()
            && strpos(&message, "The system cannot find the path specified").is_some()
        {
            io.write_error("<error>The following exception may be caused by a stale entry in your cmd.exe AutoRun</error>", true, io_interface::QUIET);
            io.write_error("<error>Check https://getcomposer.org/doc/articles/troubleshooting.md#-the-system-cannot-find-the-path-specified-windows- for details</error>", true, io_interface::QUIET);
        }

        if strpos(&message, "fork failed - Cannot allocate memory").is_some() {
            io.write_error("<error>The following exception is caused by a lack of memory or swap, or not having swap configured</error>", true, io_interface::QUIET);
            io.write_error("<error>Check https://getcomposer.org/doc/articles/troubleshooting.md#proc-open-fork-failed-errors for details</error>", true, io_interface::QUIET);
        }

        if exception
            .downcast_ref::<ProcessTimedOutException>()
            .is_some()
        {
            io.write_error(
                "<error>The following exception is caused by a process timeout</error>",
                true,
                io_interface::QUIET,
            );
            io.write_error("<error>Check https://getcomposer.org/doc/06-config.md#process-timeout for details</error>", true, io_interface::QUIET);
        }

        if self.get_disable_plugins_by_default()
            && self.is_running_as_root()
            && !self.io.is_interactive()
        {
            io.write_error("<error>Plugins have been disabled automatically as you are running as root, this may be the cause of the following exception. See also https://getcomposer.org/root</error>", true, io_interface::QUIET);
        } else if exception
            .downcast_ref::<CommandNotFoundException>()
            .is_some()
            && self.get_disable_plugins_by_default()
        {
            io.write_error("<error>Plugins have been disabled, which may be why some commands are missing, unless you made a typo</error>", true, io_interface::QUIET);
        }

        let hints = HttpDownloader::get_exception_hints_from_error(exception);
        if !hints.is_empty() && count(&hints) > 0 {
            for hint in &hints {
                io.write_error(hint, true, io_interface::QUIET);
            }
        }
    }

    pub fn get_composer(
        &mut self,
        required: bool,
        disable_plugins: Option<bool>,
        disable_scripts: Option<bool>,
    ) -> anyhow::Result<Option<&Composer>> {
        let disable_plugins = disable_plugins.unwrap_or(self.disable_plugins_by_default);
        let disable_scripts = disable_scripts.unwrap_or(self.disable_scripts_by_default);

        if self.composer.is_none() {
            let io_for_factory: Box<dyn IOInterface> = if Platform::is_input_completion_process() {
                Box::new(NullIO::new())
            } else {
                self.io.clone_box()
            };
            match Factory::create(io_for_factory, None, disable_plugins, disable_scripts) {
                Ok(c) => self.composer = Some(c),
                Err(e) => {
                    if e.downcast_ref::<JsonValidationException>().is_some()
                        || e.downcast_ref::<RuntimeException>().is_some()
                    {
                        if required {
                            return Err(e);
                        }
                    } else {
                        if required {
                            self.io.write_error(&e.to_string());
                            if self.inner.are_exceptions_caught() {
                                std::process::exit(1);
                            }
                            return Err(e);
                        }
                    }
                }
            }
        }

        Ok(self.composer.as_ref())
    }

    /// Removes the cached composer instance
    pub fn reset_composer(&mut self) {
        self.composer = None;
        if method_exists(&*self.io, "resetAuthentications") {
            self.io.reset_authentications();
        }
    }

    pub fn get_io(&self) -> &dyn IOInterface {
        &*self.io
    }

    pub fn get_help(&self) -> String {
        format!("{}{}", Self::LOGO, self.inner.get_help())
    }

    /// Initializes all the composer commands.
    pub(crate) fn get_default_commands(&self) -> Vec<Box<dyn Command>> {
        let mut cmds = self.inner.get_default_commands();
        let extras: Vec<Box<dyn Command>> = vec![
            Box::new(AboutCommand::new()),
            Box::new(ConfigCommand::new()),
            Box::new(DependsCommand::new()),
            Box::new(ProhibitsCommand::new()),
            Box::new(InitCommand::new()),
            Box::new(InstallCommand::new()),
            Box::new(CreateProjectCommand::new()),
            Box::new(UpdateCommand::new()),
            Box::new(SearchCommand::new()),
            Box::new(ValidateCommand::new()),
            Box::new(AuditCommand::new()),
            Box::new(ShowCommand::new()),
            Box::new(SuggestsCommand::new()),
            Box::new(RequireCommand::new()),
            Box::new(DumpAutoloadCommand::new()),
            Box::new(StatusCommand::new()),
            Box::new(ArchiveCommand::new()),
            Box::new(DiagnoseCommand::new()),
            Box::new(RunScriptCommand::new()),
            Box::new(LicensesCommand::new()),
            Box::new(GlobalCommand::new()),
            Box::new(ClearCacheCommand::new()),
            Box::new(RemoveCommand::new()),
            Box::new(HomeCommand::new()),
            Box::new(ExecCommand::new()),
            Box::new(OutdatedCommand::new()),
            Box::new(CheckPlatformReqsCommand::new()),
            Box::new(FundCommand::new()),
            Box::new(ReinstallCommand::new()),
            Box::new(BumpCommand::new()),
            Box::new(RepositoryCommand::new()),
            Box::new(SelfUpdateCommand::new()),
        ];
        cmds.extend(extras);
        cmds
    }

    /// This ensures we can find the correct command name even if a global input option is present before it
    fn get_command_name_before_binding(&self, input: &dyn InputInterface) -> Option<String> {
        let mut input = clone(&input);
        // Makes ArgvInput::getFirstArgument() able to distinguish an option from an argument.
        let _ = input.bind(&self.inner.get_definition());

        input.get_first_argument()
    }

    pub fn get_long_version(&self) -> String {
        let mut branch_alias_string = String::new();
        if !Composer::BRANCH_ALIAS_VERSION.is_empty()
            && Composer::BRANCH_ALIAS_VERSION != "@package_branch_alias_version@"
        {
            branch_alias_string = sprintf(
                " (%s)",
                &[Composer::BRANCH_ALIAS_VERSION.to_string().into()],
            );
        }

        sprintf(
            "<info>%s</info> version <comment>%s%s</comment> %s",
            &[
                self.inner.get_name().into(),
                self.inner.get_version().into(),
                branch_alias_string.into(),
                Composer::RELEASE_DATE.into(),
            ],
        )
    }

    pub(crate) fn get_default_input_definition(&self) -> InputDefinition {
        let mut definition = self.inner.get_default_input_definition();
        definition.add_option(InputOption::new(
            "--profile",
            None,
            Some(InputOption::VALUE_NONE),
            "Display timing and memory usage information",
            None,
            vec![],
        ));
        definition.add_option(InputOption::new(
            "--no-plugins",
            None,
            Some(InputOption::VALUE_NONE),
            "Whether to disable plugins.",
            None,
            vec![],
        ));
        definition.add_option(InputOption::new(
            "--no-scripts",
            None,
            Some(InputOption::VALUE_NONE),
            "Skips the execution of all scripts defined in composer.json file.",
            None,
            vec![],
        ));
        definition.add_option(InputOption::new(
            "--working-dir",
            Some("-d"),
            Some(InputOption::VALUE_REQUIRED),
            "If specified, use the given directory as working directory.",
            None,
            vec![],
        ));
        definition.add_option(InputOption::new(
            "--no-cache",
            None,
            Some(InputOption::VALUE_NONE),
            "Prevent use of the cache",
            None,
            vec![],
        ));

        definition
    }

    fn get_plugin_commands(&mut self) -> anyhow::Result<Vec<Box<dyn Command>>> {
        // TODO(plugin): plugin command discovery is part of the plugin API
        let mut commands: Vec<Box<dyn Command>> = vec![];

        let composer = self.get_composer(false, Some(false), None)?.cloned();
        let composer = match composer {
            Some(c) => Some(c),
            None => Factory::create_global(
                &*self.io,
                self.disable_plugins_by_default,
                self.disable_scripts_by_default,
            ),
        };

        if let Some(composer) = composer {
            let pm = composer.get_plugin_manager();
            let mut ctor_args: IndexMap<String, PhpMixed> = IndexMap::new();
            ctor_args.insert(
                "composer".to_string(),
                PhpMixed::Object(shirabe_php_shim::ArrayObject::new()),
            );
            ctor_args.insert(
                "io".to_string(),
                PhpMixed::Object(shirabe_php_shim::ArrayObject::new()),
            );
            for capability in pm
                .get_plugin_capabilities("Composer\\Plugin\\Capability\\CommandProvider", ctor_args)
            {
                let new_commands = capability.get_commands();
                for command in &new_commands {
                    if command.as_any().downcast_ref::<BaseCommand>().is_none() {
                        return Err(UnexpectedValueException {
                            message: format!("Plugin capability {} returned an invalid value, we expected an array of Composer\\Command\\BaseCommand objects", get_class(&*capability)),
                            code: 0,
                        }
                        .into());
                    }
                }
                commands = array_merge(commands, new_commands);
            }
        }

        Ok(commands)
    }

    /// Get the working directory at startup time
    pub fn get_initial_working_directory(&self) -> Option<String> {
        self.initial_working_directory.clone()
    }

    pub fn get_disable_plugins_by_default(&self) -> bool {
        self.disable_plugins_by_default
    }

    pub fn get_disable_scripts_by_default(&self) -> bool {
        self.disable_scripts_by_default
    }

    fn get_use_parent_dir_config_value(&self) -> PhpMixed {
        let config = match Factory::create_config(Some(&*self.io)) {
            Ok(c) => c,
            Err(_) => return PhpMixed::Bool(false),
        };

        config.get("use-parent-dir").clone()
    }

    fn is_running_as_root(&self) -> bool {
        function_exists("posix_getuid") && posix_getuid() == 0
    }
}
