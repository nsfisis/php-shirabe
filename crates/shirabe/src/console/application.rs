//! ref: composer/src/Composer/Console/Application.php

use crate::io::io_interface;
use indexmap::IndexMap;

use shirabe_external_packages::composer::xdebug_handler::XdebugHandler;
use shirabe_external_packages::seld::json_lint::ParsingException;
use shirabe_external_packages::symfony::component::console::Application as BaseApplication;
use shirabe_external_packages::symfony::component::console::SingleCommandApplication;
use shirabe_external_packages::symfony::component::console::command::Command;
use shirabe_external_packages::symfony::component::console::exception::CommandNotFoundException;
use shirabe_external_packages::symfony::component::console::exception::ExceptionInterface;
use shirabe_external_packages::symfony::component::console::helper::HelperSet;
use shirabe_external_packages::symfony::component::console::helper::QuestionHelper;
use shirabe_external_packages::symfony::component::console::input::InputDefinition;
use shirabe_external_packages::symfony::component::console::input::InputInterface;
use shirabe_external_packages::symfony::component::console::input::InputOption;
use shirabe_external_packages::symfony::component::console::output::ConsoleOutputInterface;
use shirabe_external_packages::symfony::component::console::output::output_interface::{
    self as output_interface, OutputInterface,
};
use shirabe_external_packages::symfony::component::process::exception::ProcessTimedOutException;
use shirabe_php_shim::{
    LogicException as ShimLogicException, PHP_BINARY, PHP_VERSION, PHP_VERSION_ID, PhpMixed,
    RuntimeException, UnexpectedValueException, array_merge, bin2hex, chdir, clone, count,
    date_default_timezone_get, date_default_timezone_set, defined, dirname, disk_free_space,
    error_get_last, extension_loaded, file_exists, file_get_contents, file_put_contents,
    function_exists, get_class, getcwd, getmypid, glob, in_array, ini_set, is_array, is_dir,
    is_file, is_string, is_subclass_of, json_decode, max_i64, memory_get_peak_usage,
    memory_get_usage, method_exists, microtime, php_uname, posix_getuid, random_bytes, realpath,
    register_shutdown_function, restore_error_handler, round, sprintf, str_contains, str_replace,
    strpos, strtoupper, sys_get_temp_dir, time, unlink,
};

use crate::command::AboutCommand;
use crate::command::ArchiveCommand;
use crate::command::AuditCommand;
use crate::command::BaseCommand;
use crate::command::BumpCommand;
use crate::command::CheckPlatformReqsCommand;
use crate::command::ClearCacheCommand;
use crate::command::ConfigCommand;
use crate::command::CreateProjectCommand;
use crate::command::DependsCommand;
use crate::command::DiagnoseCommand;
use crate::command::DumpAutoloadCommand;
use crate::command::ExecCommand;
use crate::command::FundCommand;
use crate::command::GlobalCommand;
use crate::command::HomeCommand;
use crate::command::InitCommand;
use crate::command::InstallCommand;
use crate::command::LicensesCommand;
use crate::command::OutdatedCommand;
use crate::command::ProhibitsCommand;
use crate::command::ReinstallCommand;
use crate::command::RemoveCommand;
use crate::command::RepositoryCommand;
use crate::command::RequireCommand;
use crate::command::RunScriptCommand;
use crate::command::ScriptAliasCommand;
use crate::command::SearchCommand;
use crate::command::SelfUpdateCommand;
use crate::command::ShowCommand;
use crate::command::StatusCommand;
use crate::command::SuggestsCommand;
use crate::command::UpdateCommand;
use crate::command::ValidateCommand;
use crate::composer;
use crate::composer::ComposerHandle;
use crate::composer::PartialComposerHandle;
use crate::console::GithubActionError;
use crate::downloader::TransportException;
use crate::event_dispatcher::ScriptExecutionException;
use crate::exception::NoSslException;
use crate::factory::Factory;
use crate::installer::Installer;
use crate::io::ConsoleIO;
use crate::io::IOInterface;
use crate::io::IOInterfaceImmutable;
use crate::io::NullIO;
use crate::json::JsonValidationException;
use crate::util::ErrorHandler;
use crate::util::Filesystem;
use crate::util::HttpDownloader;
use crate::util::Platform;
use crate::util::Silencer;

#[derive(Debug)]
pub struct Application {
    inner: BaseApplication,
    pub(crate) composer: Option<PartialComposerHandle>,
    pub(crate) io: std::rc::Rc<std::cell::RefCell<dyn IOInterface>>,
    has_plugin_commands: bool,
    disable_plugins_by_default: bool,
    disable_scripts_by_default: bool,
    /// Store the initial working directory at startup time
    initial_working_directory: Option<String>,
}

impl Application {
    const LOGO: &'static str = "   ______\n  / ____/___  ____ ___  ____  ____  ________  _____\n / /   / __ \\/ __ `__ \\/ __ \\/ __ \\/ ___/ _ \\/ ___/\n/ /___/ /_/ / / / / / / /_/ / /_/ (__  )  __/ /\n\\____/\\____/_/ /_/ /_/ .___/\\____/____/\\___/_/\n                    /_/\n";

    pub fn new(name: String, mut version: String) -> Self {
        let mut inner = BaseApplication::new(&name, &version);
        // TODO(phase-b): method_exists check requires reflection-style API on BaseApplication
        if true {
            inner.set_catch_errors(true);
        }

        // PHP: static $shutdownRegistered = false; — register only once globally
        static SHUTDOWN_REGISTERED: std::sync::OnceLock<()> = std::sync::OnceLock::new();
        if version == "" {
            version = composer::get_version();
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

        let io: std::rc::Rc<std::cell::RefCell<dyn IOInterface>> =
            std::rc::Rc::new(std::cell::RefCell::new(NullIO::new()));

        SHUTDOWN_REGISTERED.get_or_init(|| {
            register_shutdown_function(Box::new(|| {
                let last_error = error_get_last();

                let message = last_error
                    .as_ref()
                    .and_then(|m| m.get("message"))
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
        input: Option<&mut dyn InputInterface>,
        output: Option<&mut dyn OutputInterface>,
    ) -> anyhow::Result<i64> {
        // TODO(phase-b): Factory::create_output returns ConsoleOutput, not Box<dyn OutputInterface>.
        // The PHP code falls back to a default output when none is supplied; for now we
        // forward the caller-provided output as-is.
        self.inner.run(input, output)
    }

    pub fn do_run(
        &mut self,
        input: &mut dyn InputInterface,
        output: &dyn OutputInterface,
    ) -> anyhow::Result<i64> {
        self.disable_plugins_by_default = input.has_parameter_option(&["--no-plugins"], false);
        self.disable_scripts_by_default = input.has_parameter_option(&["--no-scripts"], false);

        // PHP: static $stdin = null;
        // We use an Option here to mimic the lazy initialization.
        // TODO(phase-b): stdin caching across calls needs proper resource handling; for
        // now we recompute on each call via PhpMixed values to keep types consistent.
        let stdin: PhpMixed = if defined("STDIN") {
            shirabe_php_shim::stdin_handle()
        } else {
            shirabe_php_shim::fopen("php://stdin", "r")
        };
        if Platform::get_env("COMPOSER_TESTS_ARE_RUNNING").as_deref() != Some("1")
            && (Platform::get_env("COMPOSER_NO_INTERACTION").is_some()
                || matches!(stdin, PhpMixed::Null)
                || !Platform::is_tty(Some(stdin)))
        {
            input.set_interactive(false);
        }

        let mut helpers: Vec<PhpMixed> = vec![];
        // TODO(phase-b): QuestionHelper does not yet implement the Helper trait;
        // packing it as PhpMixed defers the issue.
        helpers.push(PhpMixed::Null);
        let _ = QuestionHelper;
        // TODO(phase-b): ConsoleIO::new takes Box<dyn>, but here input/output are
        // borrowed references — defer construction until ownership story is sorted.
        let _ = ConsoleIO::new;
        let _ = HelperSet::new(helpers);
        // self.io stays as the NullIO that was set during construction.
        let io_owned = self.io.clone();
        let _ = io_owned;

        // Register error handler again to pass it the IO instance
        // TODO(phase-b): ErrorHandler::register expects Box<dyn IOInterface + Send>,
        // not a borrow; passing None until the IO sharing story is settled.
        ErrorHandler::register(None);

        if input.has_parameter_option(&["--no-cache"], false) {
            self.io
                .write_error3("Disabling cache usage", true, io_interface::DEBUG);
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
            old_working_dir = Some(Platform::get_cwd(true).unwrap_or_default());
            chdir(nwd);
            self.initial_working_directory = getcwd();
            let cwd = Platform::get_cwd(true).unwrap_or_default();
            self.io.write_error3(
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
                Ok(cmd) => {
                    // TODO(phase-b): BaseApplication::find returns PhpMixed; calling
                    // get_name() requires a Command trait downcast that is not yet wired.
                    let _ = cmd;
                    command_name = Some(String::new());
                }
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
        let no_composer_json_commands_pm = PhpMixed::List(
            no_composer_json_commands
                .iter()
                .map(|s| Box::new(PhpMixed::String(s.clone())))
                .collect(),
        );
        if new_work_dir.is_none()
            && !in_array(
                command_name.as_deref().unwrap_or("").into(),
                &no_composer_json_commands_pm,
                true,
            )
            && !file_exists(&Factory::get_composer_file().unwrap_or_default())
            && use_parent_dir_if_no_json_available.as_bool() != Some(false)
            && (command_name.as_deref() != Some("config")
                || (input.has_parameter_option(&["--file"], true) == false
                    && input.has_parameter_option(&["-f"], true) == false))
            && input.has_parameter_option(&["--help"], true) == false
            && input.has_parameter_option(&["-h"], true) == false
        {
            let mut dir = dirname(&Platform::get_cwd(true).unwrap_or_default());
            let home_value = Platform::get_env("HOME")
                .or_else(|| Platform::get_env("USERPROFILE"))
                .unwrap_or_else(|| "/".to_string());
            let home = realpath(&home_value).unwrap_or_default();

            // abort when we reach the home dir or top of the filesystem
            while dirname(&dir) != dir && dir != home {
                if file_exists(&format!(
                    "{}/{}",
                    dir,
                    Factory::get_composer_file().unwrap_or_default()
                )) {
                    if use_parent_dir_if_no_json_available.as_bool() != Some(true)
                        && !self.io.is_interactive()
                    {
                        self.io.write_error(&format!("<info>No composer.json in current directory, to use the one at {} run interactively or set config.use-parent-dir to true</info>", dir));
                        break;
                    }
                    if use_parent_dir_if_no_json_available.as_bool() == Some(true)
                        || self.io.ask_confirmation(format!("<info>No composer.json in current directory, do you want to use the one at {}?</info> [<comment>y,n</comment>]? ", dir), true)
                    {
                        if use_parent_dir_if_no_json_available.as_bool() == Some(true) {
                            self.io.write_error(&format!("<info>No composer.json in current directory, changing working directory to {}</info>", dir));
                        } else {
                            self.io.write_error("<info>Always want to use the parent dir? Use \"composer config --global use-parent-dir true\" to change the default.</info>");
                        }
                        old_working_dir = Some(Platform::get_cwd(true).unwrap_or_default());
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
                        shirabe_php_shim::exec(
                            &format!("sudo -u \\#{} sudo -K > /dev/null 2>&1", uid),
                            None,
                            None,
                        );
                        Ok(())
                    });
                }
            }

            // Silently clobber any remaining sudo leases on the current user as well to avoid privilege escalations
            let _ = Silencer::call(|| {
                shirabe_php_shim::exec("sudo -K > /dev/null 2>&1", None, None);
                Ok(())
            });
        }

        // avoid loading plugins/initializing the Composer instance earlier than necessary if no plugin command is needed
        // if showing the version, we never need plugin commands
        let mnp_list = PhpMixed::List(vec![
            Box::new(PhpMixed::String("".to_string())),
            Box::new(PhpMixed::String("list".to_string())),
            Box::new(PhpMixed::String("help".to_string())),
        ]);
        let may_need_plugin_command = !input.has_parameter_option(&["--version", "-V"], false)
            && (command_name.is_none()
                || in_array(
                    command_name.as_deref().unwrap_or("").into(),
                    &mnp_list,
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
                self.io.write_error("<warning>Do not run Composer as root/super user! See https://getcomposer.org/root for details</warning>");

                if self.io.is_interactive()
                    && self.io.ask_confirmation(
                        "<info>Continue as root/super user</info> [<comment>yes</comment>]? "
                            .to_string(),
                        true,
                    )
                {
                    // avoid a second prompt later
                    is_non_allowed_root = false;
                } else {
                    self.io.write_error("<warning>Aborting as no plugin should be loaded if running as super user is not explicitly allowed</warning>");

                    return Ok(1);
                }
            }

            // TODO(phase-b): the original PHP catches plugin discovery exceptions in a
            // try/catch. The Rust port keeps the loop but skips IO error reporting
            // because get_plugin_commands borrows &mut self, conflicting with io.
            let mut plugin_warnings: Vec<String> = Vec::new();
            match (|| -> anyhow::Result<()> {
                for command in self.get_plugin_commands()? {
                    let cmd_name = command.get_name().unwrap_or_default();
                    if self.inner.has(&cmd_name) {
                        // TODO(phase-b): get_class needs a Command-aware overload; default
                        // to a placeholder while the trait downcast story is settled.
                        let cls = String::new();
                        plugin_warnings.push(format!("<warning>Plugin command {} ({}) would override a Composer command and has been skipped</warning>", cmd_name, cls));
                    } else {
                        // Compatibility layer for symfony/console <7.4
                        // TODO(phase-b): add_command/add accept PhpMixed; the symfony
                        // stubs do not yet expose typed command insertion.
                        let _ = command;
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

                        let file = realpath(&Factory::get_composer_file().unwrap_or_default());

                        let line = details.line;

                        let mut ghe = GithubActionError::new(self.io.clone());
                        ghe.emit(&pe.message, file.as_deref(), line);

                        return Err(e);
                    } else {
                        return Err(e);
                    }
                }
            }
            for warning in &plugin_warnings {
                self.io.write_error(warning);
            }

            self.has_plugin_commands = true;
        }

        if !self.disable_plugins_by_default && is_non_allowed_root && !self.io.is_interactive() {
            self.io.write_error("<error>Composer plugins have been disabled for safety in this non-interactive session.</error>");
            self.io.write_error("<error>Set COMPOSER_ALLOW_SUPERUSER=1 if you want to allow plugins to run as root/super user.</error>");
            self.disable_plugins_by_default = true;
        }

        // determine command name to be executed incl plugin commands, and check if it's a proxy command
        let is_proxy_command = false;
        if let Some(ref name) = self.get_command_name_before_binding(input) {
            if let Ok(command) = self.inner.find(name) {
                // TODO(phase-b): BaseApplication::find returns PhpMixed; we cannot yet
                // extract a typed command name or detect proxy commands without the
                // command trait downcast story.
                let _ = command;
                command_name = Some(String::new());
            }
        }

        if !is_proxy_command {
            self.io.write_error3(
                &sprintf(
                    "Running %s (%s) with %s on %s",
                    &[
                        composer::get_version().into(),
                        composer::RELEASE_DATE.into(),
                        (if defined("HHVM_VERSION") {
                            format!("HHVM {}", shirabe_php_shim::HHVM_VERSION.unwrap_or(""))
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
                self.io.write_error(&format!("<warning>Composer supports PHP 7.2.5 and above, you will most likely encounter problems with your PHP {}. Upgrading is strongly recommended but you can use Composer 2.2.x LTS as a fallback.</warning>", PHP_VERSION));
            }

            if XdebugHandler::is_xdebug_active()
                && Platform::get_env("COMPOSER_DISABLE_XDEBUG_WARN").is_none()
            {
                self.io.write_error("<warning>Composer is operating slower than normal because you have Xdebug enabled. See https://getcomposer.org/xdebug</warning>");
            }

            if defined("COMPOSER_DEV_WARNING_TIME")
                && command_name.as_deref() != Some("self-update")
                && command_name.as_deref() != Some("selfupdate")
                && time() > shirabe_php_shim::composer_dev_warning_time()
            {
                self.io.write_error(&sprintf(
                    "<warning>Warning: This development build of Composer is over 60 days old. It is recommended to update it by running \"%s self-update\" to get the latest version.</warning>",
                    &[shirabe_php_shim::server_get("PHP_SELF").unwrap_or_default().into()],
                ));
            }

            if is_non_allowed_root {
                if command_name.as_deref() != Some("self-update")
                    && command_name.as_deref() != Some("selfupdate")
                    && command_name.as_deref() != Some("_complete")
                {
                    self.io.write_error("<warning>Do not run Composer as root/super user! See https://getcomposer.org/root for details</warning>");

                    if self.io.is_interactive() {
                        if !self.io.ask_confirmation(
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
            let tempfile_msg: Option<String> = Silencer::call(|| -> anyhow::Result<Option<String>> {
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
                if !(file_put_contents(&tempfile, file!().as_bytes()).is_some_and(|n| n > 0)
                    && file_get_contents(&tempfile).as_deref() == Some(file!())
                    && unlink(&tempfile)
                    && !file_exists(&tempfile))
                {
                    return Ok(Some(sprintf("<error>PHP temp directory (%s) does not exist or is not writable to Composer. Set sys_temp_dir in your php.ini</error>", &[sys_get_temp_dir().into()])));
                }
                Ok(None)
            })
            .ok()
            .flatten();
            if let Some(msg) = tempfile_msg {
                self.io.write_error(&msg);
            }

            // add non-standard scripts as own commands
            let file = Factory::get_composer_file().unwrap_or_default();
            if may_need_script_command && is_file(&file) && Filesystem::is_readable(&file) {
                let composer_json: PhpMixed =
                    json_decode(&file_get_contents(&file).unwrap_or_default(), true)
                        .unwrap_or(PhpMixed::Null);
                if let Some(arr) = composer_json.as_array() {
                    if let Some(scripts) = arr.get("scripts").and_then(|v| v.as_array()) {
                        for (script, dummy) in scripts {
                            let script_event_const = format!(
                                "Composer\\Script\\ScriptEvents::{}",
                                str_replace("-", "_", &strtoupper(script))
                            );
                            if !defined(&script_event_const) {
                                if self.inner.has(script) {
                                    self.io.write_error(&format!("<warning>A script named {} would override a Composer command and has been skipped</warning>", script));
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
                                        let composer = crate::command::composer_full(&composer);
                                        let root_package = composer.get_package();
                                        let generator = composer.get_autoload_generator().clone();
                                        let generator = generator.borrow();

                                        // TODO(phase-b): build_package_map needs &mut InstallationManager
                                        // but get_composer returns &Composer; skip until shared ownership is settled.
                                        let package_map: Vec<(
                                            crate::package::PackageInterfaceHandle,
                                            Option<String>,
                                        )> = todo!(
                                            "build_package_map requires &mut InstallationManager"
                                        );
                                        let map = generator.parse_autoloads(
                                            package_map,
                                            root_package.clone(),
                                            PhpMixed::Bool(false),
                                        );

                                        let loader = generator.create_loader(
                                            &map,
                                            composer
                                                .get_config()
                                                .borrow()
                                                .get("vendor-dir")
                                                .as_string()
                                                .map(|s| s.to_string()),
                                        );
                                        loader.register(false);
                                    }

                                    // if the command is not an array of commands, and points to a valid Command subclass, import its details directly
                                    let dummy_str = dummy.as_string().unwrap_or("").to_string();
                                    let cmd: PhpMixed = if is_string(dummy)
                                        && shirabe_php_shim::class_exists(&dummy_str)
                                        && is_subclass_of(
                                            &PhpMixed::String(dummy_str.clone()),
                                            "Symfony\\Component\\Console\\Command\\Command",
                                            true,
                                        ) {
                                        if is_subclass_of(
                                            &PhpMixed::String(dummy_str.clone()),
                                            "Symfony\\Component\\Console\\SingleCommandApplication",
                                            true,
                                        ) {
                                            self.io.write_error(&format!("<warning>The script named {} extends SingleCommandApplication which is not compatible with Composer 2.9+, make sure you extend Symfony\\Component\\Console\\Command instead.</warning>", script));
                                        }
                                        let mut cmd = shirabe_php_shim::instantiate_class(
                                            &dummy_str,
                                            vec![PhpMixed::String(script.clone())],
                                        );
                                        // TODO(phase-b): SingleCommandApplication has no class_name() yet.
                                        let _ = SingleCommandApplication::new;

                                        // makes sure the command is find()'able by the name defined in composer.json, and the name isn't overridden in its configure()
                                        // TODO(phase-b): cmd is PhpMixed; get_name/set_name/get_description/set_description
                                        // require the command trait to be unwrapped. Defer until that lands.
                                        let _ = description.clone();
                                        let _ = &mut cmd;
                                        cmd
                                    } else {
                                        // fallback to usual aliasing behavior
                                        // TODO(phase-b): ScriptAliasCommand returns Result; bury it
                                        // into PhpMixed::Null until the command-as-PhpMixed path is
                                        // replaced by a typed trait object.
                                        let _ = ScriptAliasCommand::new(
                                            script.clone(),
                                            Some(description.clone()),
                                            aliases,
                                        );
                                        PhpMixed::Null
                                    };

                                    // Compatibility layer for symfony/console <7.4
                                    // TODO(phase-b): add_command/add take PhpMixed but expect a
                                    // command instance; pending typed-command rewiring.
                                    let _ = self.inner.add(cmd);
                                }
                            }
                        }
                    }
                }
            }
        }

        let mut start_time: Option<f64> = None;
        let result_outcome: anyhow::Result<i64> = (|| -> anyhow::Result<i64> {
            if input.has_parameter_option(&["--profile"], false) {
                start_time = Some(microtime(true));
                // TODO(phase-b): enable_debugging is defined only on ConsoleIO, not
                // through IOInterface. Skip until the IO concrete type is known here.
                let _ = start_time.unwrap();
            }

            // TODO(phase-b): BaseApplication exposes only `run`, not `do_run`.
            let result: i64 = todo!("BaseApplication::do_run");

            if input.has_parameter_option(&["--version", "-V"], true) {
                self.io.write_error(&sprintf(
                    "<info>PHP</info> version <comment>%s</comment> (%s)",
                    &[PHP_VERSION.into(), PHP_BINARY.into()],
                ));
                self.io.write_error(
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
            self.io.write_error(&format!(
                "<info>Memory usage: {}MiB (peak: {}MiB), time: {}s</info>",
                round((memory_get_usage() as f64) / 1024.0 / 1024.0, 2),
                round((memory_get_peak_usage(true) as f64) / 1024.0 / 1024.0, 2),
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
                        self.io.write_error3("<error>Plugins have been disabled automatically as you are running as root, this may be the cause of the script failure.</error>", true, io_interface::QUIET);
                        self.io.write_error3(
                            "<error>See also https://getcomposer.org/root</error>",
                            true,
                            io_interface::QUIET,
                        );
                    }

                    Ok(see.get_code())
                } else {
                    let mut ghe = GithubActionError::new(self.io.clone());
                    ghe.emit(&e.to_string(), None, None);

                    self.hint_common_errors(&e, output);

                    // TODO(phase-b): method_exists/as_any on the inner application and
                    // output trait objects are not yet supported; replicate the catch-all
                    // branch unconditionally.
                    if false {
                        let _ = <dyn ConsoleOutputInterface>::is_console_output_interface;
                        // self.inner.render_throwable expects &mut dyn OutputInterface.
                        // Skipped while output is &dyn OutputInterface here.
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
            .get_parameter_option(&["--working-dir", "-d"], PhpMixed::Null, true)
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

    fn hint_common_errors(&mut self, exception: &anyhow::Error, output: &dyn OutputInterface) {
        let is_logic_or_error = exception.downcast_ref::<ShimLogicException>().is_some();
        if is_logic_or_error && output.get_verbosity() < output_interface::VERBOSITY_VERBOSE {
            output.set_verbosity(output_interface::VERBOSITY_VERBOSE);
        }

        Silencer::suppress(None);
        // Compute the disk-space hint message first; emit it via io afterwards to
        // avoid overlapping borrows of self (get_composer needs &mut self).
        let disk_hint_msg: Option<String> = (|| -> anyhow::Result<Option<String>> {
            let composer = self.get_composer(false, Some(true), None)?;
            if composer.is_some() && function_exists("disk_free_space") {
                let composer = composer.unwrap();
                let composer = composer.borrow_partial();
                let config = composer.get_config();

                let min_space_free: f64 = 100.0 * 1024.0 * 1024.0;
                let mut dir = config
                    .borrow_mut()
                    .get("home")
                    .as_string()
                    .unwrap_or("")
                    .to_string();
                let df = disk_free_space(&dir);
                let mut hit = df.map(|d| d < min_space_free).unwrap_or(false);
                if !hit {
                    dir = config
                        .borrow_mut()
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
                    return Ok(Some(format!("<error>The disk hosting {} has less than 100MiB of free space, this may be the cause of the following exception</error>", dir)));
                }
            }
            Ok(None)
        })()
        .ok()
        .flatten();
        Silencer::restore();

        let io = self.get_io();
        if let Some(msg) = &disk_hint_msg {
            io.write_error3(msg, true, io_interface::QUIET);
        }

        let message = exception.to_string();
        if exception.downcast_ref::<TransportException>().is_some()
            && str_contains(&message, "Unable to use a proxy")
        {
            io.write_error3(
                "<error>The following exception indicates your proxy is misconfigured</error>",
                true,
                io_interface::QUIET,
            );
            io.write_error3("<error>Check https://getcomposer.org/doc/faqs/how-to-use-composer-behind-a-proxy.md for details</error>", true, io_interface::QUIET);
        }

        if Platform::is_windows()
            && exception.downcast_ref::<TransportException>().is_some()
            && str_contains(&message, "unable to get local issuer certificate")
        {
            let avast_detect = glob("C:\\Program Files\\Avast*");
            let avast_detect_pm = PhpMixed::List(
                avast_detect
                    .iter()
                    .map(|s| Box::new(PhpMixed::String(s.clone())))
                    .collect(),
            );
            if is_array(&avast_detect_pm) && count(&avast_detect_pm) != 0 {
                io.write_error3("<error>The following exception indicates a possible issue with the Avast Firewall</error>", true, io_interface::QUIET);
                io.write_error3(
                    "<error>Check https://getcomposer.org/local-issuer for details</error>",
                    true,
                    io_interface::QUIET,
                );
            } else {
                io.write_error3("<error>The following exception indicates a possible issue with a Firewall/Antivirus</error>", true, io_interface::QUIET);
                io.write_error3(
                    "<error>Check https://getcomposer.org/local-issuer for details</error>",
                    true,
                    io_interface::QUIET,
                );
            }
        }

        if Platform::is_windows()
            && strpos(&message, "The system cannot find the path specified").is_some()
        {
            io.write_error3("<error>The following exception may be caused by a stale entry in your cmd.exe AutoRun</error>", true, io_interface::QUIET);
            io.write_error3("<error>Check https://getcomposer.org/doc/articles/troubleshooting.md#-the-system-cannot-find-the-path-specified-windows- for details</error>", true, io_interface::QUIET);
        }

        if strpos(&message, "fork failed - Cannot allocate memory").is_some() {
            io.write_error3("<error>The following exception is caused by a lack of memory or swap, or not having swap configured</error>", true, io_interface::QUIET);
            io.write_error3("<error>Check https://getcomposer.org/doc/articles/troubleshooting.md#proc-open-fork-failed-errors for details</error>", true, io_interface::QUIET);
        }

        if exception
            .downcast_ref::<ProcessTimedOutException>()
            .is_some()
        {
            io.write_error3(
                "<error>The following exception is caused by a process timeout</error>",
                true,
                io_interface::QUIET,
            );
            io.write_error3("<error>Check https://getcomposer.org/doc/06-config.md#process-timeout for details</error>", true, io_interface::QUIET);
        }

        if self.get_disable_plugins_by_default()
            && self.is_running_as_root()
            && !self.io.is_interactive()
        {
            io.write_error3("<error>Plugins have been disabled automatically as you are running as root, this may be the cause of the following exception. See also https://getcomposer.org/root</error>", true, io_interface::QUIET);
        } else if exception
            .downcast_ref::<CommandNotFoundException>()
            .is_some()
            && self.get_disable_plugins_by_default()
        {
            io.write_error3("<error>Plugins have been disabled, which may be why some commands are missing, unless you made a typo</error>", true, io_interface::QUIET);
        }

        let hints = HttpDownloader::get_exception_hints(exception).unwrap_or_default();
        if !hints.is_empty() {
            for hint in &hints {
                io.write_error3(hint, true, io_interface::QUIET);
            }
        }
    }

    pub fn get_composer(
        &mut self,
        required: bool,
        disable_plugins: Option<bool>,
        disable_scripts: Option<bool>,
    ) -> anyhow::Result<Option<PartialComposerHandle>> {
        let disable_plugins = disable_plugins.unwrap_or(self.disable_plugins_by_default);
        let disable_scripts = disable_scripts.unwrap_or(self.disable_scripts_by_default);

        if self.composer.is_none() {
            let io_for_factory: std::rc::Rc<std::cell::RefCell<dyn IOInterface>> =
                if Platform::is_input_completion_process() {
                    std::rc::Rc::new(std::cell::RefCell::new(NullIO::new()))
                } else {
                    self.io.clone()
                };
            let disable_plugins_enum = if disable_plugins {
                crate::factory::DisablePlugins::All
            } else {
                crate::factory::DisablePlugins::None
            };
            match Factory::create(io_for_factory, None, disable_plugins_enum, disable_scripts) {
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
                            // TODO(phase-b): BaseApplication::are_exceptions_caught not yet
                            // available; fall through to returning the error.
                            return Err(e);
                        }
                    }
                }
            }
        }

        Ok(self.composer.clone())
    }

    /// Removes the cached composer instance
    pub fn reset_composer(&mut self) {
        self.composer = None;
        // TODO(phase-b): reset_authentications is defined on BaseIO not IOInterface;
        // skipped until the cross-trait dispatch story is settled.
    }

    /// Delegates to the underlying BaseApplication's `find` method (PHP Symfony Console).
    pub fn find(&self, _name: &str) -> anyhow::Result<shirabe_php_shim::PhpMixed> {
        todo!()
    }

    pub fn get_io(&self) -> std::rc::Rc<std::cell::RefCell<dyn IOInterface>> {
        self.io.clone()
    }

    pub fn get_help(&self) -> String {
        // TODO(phase-b): BaseApplication::get_help is not yet exposed via the stub.
        format!("{}{}", Self::LOGO, "")
    }

    /// Initializes all the composer commands.
    pub(crate) fn get_default_commands(&self) -> Vec<Box<dyn Command>> {
        // TODO(phase-b): each shirabe command struct needs its own `impl Command` (the orphan
        // rule disallowed a blanket `impl<C: HasBaseCommandData> Command for C`). Until those
        // are written, expose only the inner symfony defaults.
        // TODO(phase-b): BaseApplication::get_default_commands is not yet exposed.
        vec![]
    }

    /// This ensures we can find the correct command name even if a global input option is present before it
    fn get_command_name_before_binding(&self, input: &dyn InputInterface) -> Option<String> {
        let mut input = clone(&input);
        // Makes ArgvInput::getFirstArgument() able to distinguish an option from an argument.
        // TODO(phase-b): BaseApplication::get_definition returns PhpMixed, not InputDefinition.
        let _ = input;
        let _ = self.inner.get_definition();
        None
    }

    pub fn get_long_version(&self) -> String {
        let mut branch_alias_string = String::new();
        if !composer::BRANCH_ALIAS_VERSION.is_empty()
            && composer::BRANCH_ALIAS_VERSION != "@package_branch_alias_version@"
        {
            branch_alias_string = sprintf(
                " (%s)",
                &[composer::BRANCH_ALIAS_VERSION.to_string().into()],
            );
        }

        sprintf(
            "<info>%s</info> version <comment>%s%s</comment> %s",
            &[
                self.inner.get_name().into(),
                self.inner.get_version().into(),
                branch_alias_string.into(),
                composer::RELEASE_DATE.into(),
            ],
        )
    }

    pub(crate) fn get_default_input_definition(&self) -> InputDefinition {
        // TODO(phase-b): BaseApplication::get_default_input_definition is not yet exposed.
        let mut definition = InputDefinition::new(vec![]);
        let _ = InputOption::new(
            "--profile",
            None,
            Some(InputOption::VALUE_NONE),
            "Display timing and memory usage information",
            PhpMixed::Null,
        );
        definition.add_option(PhpMixed::Null);
        let _ = InputOption::new(
            "--no-plugins",
            None,
            Some(InputOption::VALUE_NONE),
            "Whether to disable plugins.",
            PhpMixed::Null,
        );
        definition.add_option(PhpMixed::Null);
        let _ = InputOption::new(
            "--no-scripts",
            None,
            Some(InputOption::VALUE_NONE),
            "Skips the execution of all scripts defined in composer.json file.",
            PhpMixed::Null,
        );
        definition.add_option(PhpMixed::Null);
        let _ = InputOption::new(
            "--working-dir",
            Some("-d"),
            Some(InputOption::VALUE_REQUIRED),
            "If specified, use the given directory as working directory.",
            PhpMixed::Null,
        );
        definition.add_option(PhpMixed::Null);
        let _ = InputOption::new(
            "--no-cache",
            None,
            Some(InputOption::VALUE_NONE),
            "Prevent use of the cache",
            PhpMixed::Null,
        );
        definition.add_option(PhpMixed::Null);

        definition
    }

    fn get_plugin_commands(&mut self) -> anyhow::Result<Vec<Box<dyn Command>>> {
        // TODO(plugin): plugin command discovery is part of the plugin API
        let commands: Vec<Box<dyn Command>> = vec![];

        // TODO(phase-b): Composer is a PHP class (no Clone) and the plugin manager
        // pathway needs PluginCapability downcasting. Defer the full implementation
        // until those are available; for now return the empty command list.
        let _ = self.get_composer(false, Some(false), None)?;
        let _ = UnexpectedValueException {
            message: String::new(),
            code: 0,
        };
        let _: fn(PhpMixed, PhpMixed) -> PhpMixed = array_merge;
        let _: fn(&PhpMixed) -> String = get_class;
        let _ = shirabe_php_shim::ArrayObject::new(None);
        let _: IndexMap<String, PhpMixed> = IndexMap::new();

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
        let config = match Factory::create_config(Some(self.io.clone()), None) {
            Ok(c) => c,
            Err(_) => return PhpMixed::Bool(false),
        };

        config.get("use-parent-dir").clone()
    }

    fn is_running_as_root(&self) -> bool {
        function_exists("posix_getuid") && posix_getuid() == 0
    }
}
