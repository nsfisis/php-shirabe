//! ref: composer/src/Composer/Console/Application.php
//! ref: composer/vendor/symfony/console/Application.php

use crate::io::io_interface;
use indexmap::IndexMap;

use shirabe_external_packages::composer::xdebug_handler::XdebugHandler;
use shirabe_external_packages::seld::json_lint::ParsingException;
use shirabe_external_packages::symfony::console::application::Application as BaseApplication;
use shirabe_external_packages::symfony::console::command::Command as SymfonyCommand;
use shirabe_external_packages::symfony::console::command::help_command::HelpCommand;
use shirabe_external_packages::symfony::console::command::lazy_command::LazyCommand;
use shirabe_external_packages::symfony::console::command::signalable_command_interface::SignalableCommandInterface;
use shirabe_external_packages::symfony::console::command_loader::command_loader_interface::CommandLoaderInterface;
use shirabe_external_packages::symfony::console::completion::completion_input::CompletionInput;
use shirabe_external_packages::symfony::console::completion::completion_suggestions::CompletionSuggestions;
use shirabe_external_packages::symfony::console::exception::CommandNotFoundException;
use shirabe_external_packages::symfony::console::exception::ExceptionInterface;
use shirabe_external_packages::symfony::console::exception::invalid_argument_exception::InvalidArgumentException as ConsoleInvalidArgumentException;
use shirabe_external_packages::symfony::console::exception::invalid_option_exception::InvalidOptionException;
use shirabe_external_packages::symfony::console::exception::logic_exception::LogicException as ConsoleLogicException;
use shirabe_external_packages::symfony::console::exception::missing_input_exception::MissingInputException;
use shirabe_external_packages::symfony::console::exception::namespace_not_found_exception::NamespaceNotFoundException;
use shirabe_external_packages::symfony::console::exception::runtime_exception::RuntimeException as ConsoleRuntimeException;
use shirabe_external_packages::symfony::console::formatter::output_formatter::OutputFormatter;
use shirabe_external_packages::symfony::console::helper::HelperInterface;
use shirabe_external_packages::symfony::console::helper::HelperSet;
use shirabe_external_packages::symfony::console::helper::HelperSetKey;
use shirabe_external_packages::symfony::console::helper::QuestionHelper;
use shirabe_external_packages::symfony::console::helper::debug_formatter_helper::DebugFormatterHelper;
use shirabe_external_packages::symfony::console::helper::formatter_helper::{
    FormatBlockMessages, FormatterHelper,
};
use shirabe_external_packages::symfony::console::helper::helper::Helper;
use shirabe_external_packages::symfony::console::helper::process_helper::ProcessHelper;
use shirabe_external_packages::symfony::console::input::InputDefinition;
use shirabe_external_packages::symfony::console::input::InputInterface;
use shirabe_external_packages::symfony::console::input::InputOption;
use shirabe_external_packages::symfony::console::input::argv_input::ArgvInput;
use shirabe_external_packages::symfony::console::input::array_input::ArrayInput;
use shirabe_external_packages::symfony::console::input::input_argument::InputArgument;
use shirabe_external_packages::symfony::console::input::input_aware_interface::InputAwareInterface;
use shirabe_external_packages::symfony::console::output::ConsoleOutputInterface;
use shirabe_external_packages::symfony::console::output::console_output::ConsoleOutput;
use shirabe_external_packages::symfony::console::output::output_interface::{
    self as output_interface, OutputInterface,
};
use shirabe_external_packages::symfony::console::signal_registry::signal_registry::SignalRegistry;
use shirabe_external_packages::symfony::console::style::style_interface::StyleInterface;
use shirabe_external_packages::symfony::console::style::symfony_style::SymfonyStyle;
use shirabe_external_packages::symfony::console::terminal::Terminal;
use shirabe_external_packages::symfony::process::exception::ProcessTimedOutException;
use shirabe_php_shim::{
    LogicException as ShimLogicException, PHP_BINARY, PHP_VERSION, PHP_VERSION_ID, PhpMixed,
    RuntimeException, UnexpectedValueException, array_merge, bin2hex, chdir, count,
    date_default_timezone_get, date_default_timezone_set, defined, dirname, disk_free_space,
    error_get_last, extension_loaded, file_exists, file_get_contents, file_put_contents,
    function_exists, get_class, getcwd, getmypid, glob, in_array, ini_set, is_array, is_dir,
    is_file, is_string, is_subclass_of, json_decode, memory_get_peak_usage, memory_get_usage,
    method_exists, microtime, php_uname, posix_getuid, random_bytes, realpath,
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

/// The PHP `Composer\Console\Application` and `Symfony\Component\Console\Application` are
/// flattened into a single struct. Methods that are overridden by subclass and called via
/// `parent::` are prefixed by `base_`.
#[derive(Debug)]
pub struct Application {
    commands: IndexMap<String, std::rc::Rc<std::cell::RefCell<dyn SymfonyCommand>>>,
    want_helps: bool,
    running_command: Option<std::rc::Rc<std::cell::RefCell<dyn SymfonyCommand>>>,
    name: String,
    version: String,
    command_loader: Option<Box<dyn CommandLoaderInterface>>,
    catch_exceptions: bool,
    definition: Option<std::rc::Rc<std::cell::RefCell<InputDefinition>>>,
    helper_set: Option<std::rc::Rc<std::cell::RefCell<HelperSet>>>,
    terminal: Terminal,
    default_command: String,
    single_command: bool,
    initialized: bool,
    signal_registry: Option<SignalRegistry>,
    signals_to_dispatch_event: Vec<i64>,

    pub(crate) composer: Option<PartialComposerHandle>,
    pub(crate) io: std::rc::Rc<std::cell::RefCell<dyn IOInterface>>,
    has_plugin_commands: bool,
    disable_plugins_by_default: bool,
    disable_scripts_by_default: bool,
    /// Store the initial working directory at startup time
    initial_working_directory: Option<String>,
    /// Self-reference used to hand commands their owning application (PHP's `$command->setApplication($this)`).
    /// Set by `new_shared`; the run flow threads the upgraded `Rc` so command callbacks never
    /// re-borrow the application while it is already borrowed.
    me: std::rc::Weak<std::cell::RefCell<Application>>,
}

impl Application {
    const LOGO: &'static str = "   ______\n  / ____/___  ____ ___  ____  ____  ________  _____\n / /   / __ \\/ __ `__ \\/ __ \\/ __ \\/ ___/ _ \\/ ___/\n/ /___/ /_/ / / / / / / /_/ / /_/ (__  )  __/ /\n\\____/\\____/_/ /_/ /_/ .___/\\____/____/\\___/_/\n                    /_/\n";

    pub fn new(name: String, mut version: String) -> Self {
        static SHUTDOWN_REGISTERED: std::sync::OnceLock<()> = std::sync::OnceLock::new();
        if version.is_empty() {
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

        let mut this = Self {
            commands: IndexMap::new(),
            want_helps: false,
            running_command: None,
            name,
            version,
            command_loader: None,
            catch_exceptions: true,
            definition: None,
            helper_set: None,
            terminal: Terminal::new(),
            default_command: "list".to_string(),
            single_command: false,
            initialized: false,
            signal_registry: None,
            signals_to_dispatch_event: Vec::new(),
            composer: None,
            io,
            has_plugin_commands: false,
            disable_plugins_by_default: false,
            disable_scripts_by_default: false,
            initial_working_directory,
            me: std::rc::Weak::new(),
        };
        if defined("SIGINT") && SignalRegistry::is_supported() {
            this.signal_registry = Some(SignalRegistry::new());
            this.signals_to_dispatch_event = vec![
                shirabe_php_shim::SIGINT,
                shirabe_php_shim::SIGTERM,
                shirabe_php_shim::SIGUSR1,
                shirabe_php_shim::SIGUSR2,
            ];
        }
        this
    }

    /// Builds the application inside a shared `Rc<RefCell<…>>` and registers the default commands.
    ///
    /// Commands hold a back-pointer to their application, which shares this very `RefCell`. So the
    /// whole run flow is driven through the `Rc` (never a long-lived `borrow_mut`), and command
    /// registration happens here — before any borrow is taken — so `set_application` can borrow the
    /// application without a re-entrant conflict.
    pub fn new_shared(
        name: String,
        version: String,
    ) -> anyhow::Result<std::rc::Rc<std::cell::RefCell<Application>>> {
        let application =
            std::rc::Rc::new(std::cell::RefCell::new(Application::new(name, version)));
        application.borrow_mut().me = std::rc::Rc::downgrade(&application);
        Application::init_shared(&application)?;
        Ok(application)
    }

    /// Returns the shared handle to this application set up by `new_shared`. Proxy commands use it to
    /// re-enter the run flow (PHP's `$this->getApplication()->run(...)`).
    pub fn shared(&self) -> std::rc::Rc<std::cell::RefCell<Application>> {
        self.me
            .upgrade()
            .expect("Application must be constructed through new_shared")
    }

    /// Registers the default commands on a shared application (PHP's lazy `init()`), executed once
    /// with no application borrow held so each `set_application` can borrow back safely.
    fn init_shared(
        application: &std::rc::Rc<std::cell::RefCell<Application>>,
    ) -> anyhow::Result<()> {
        {
            let mut app = application.borrow_mut();
            if app.initialized {
                return Ok(());
            }
            app.initialized = true;
        }

        let commands = application.borrow().get_default_commands();
        for command in commands {
            Application::add_shared(application, command)?;
        }

        Ok(())
    }

    /// Adds a command object to a shared application (PHP's `add()`), without holding an application
    /// borrow while calling into the command, so `set_application` can borrow back safely.
    pub fn add_shared(
        application: &std::rc::Rc<std::cell::RefCell<Application>>,
        command: std::rc::Rc<std::cell::RefCell<dyn SymfonyCommand>>,
    ) -> anyhow::Result<Option<std::rc::Rc<std::cell::RefCell<dyn SymfonyCommand>>>> {
        command.borrow_mut().set_application(Some(
            application.clone() as std::rc::Rc<std::cell::RefCell<dyn BaseApplication>>
        ));

        if !command.borrow().is_enabled() {
            command.borrow_mut().set_application(None);

            return Ok(None);
        }

        // if (!$command instanceof LazyCommand) { $command->getDefinition(); }
        command.borrow().get_definition();

        if command.borrow().get_name().is_none() {
            return Err(ConsoleLogicException(shirabe_php_shim::LogicException {
                message: format!(
                    "The command defined in \"{}\" cannot have an empty name.",
                    PhpMixed::from(shirabe_php_shim::get_debug_type_obj(&command)),
                ),
                code: 0,
            })
            .into());
        }

        let name = command.borrow().get_name().unwrap();
        application
            .borrow_mut()
            .commands
            .insert(name, command.clone());

        for alias in command.borrow().get_aliases() {
            application
                .borrow_mut()
                .commands
                .insert(alias, command.clone());
        }

        Ok(Some(command))
    }

    pub fn run(
        application: &std::rc::Rc<std::cell::RefCell<Application>>,
        input: Option<std::rc::Rc<std::cell::RefCell<dyn InputInterface>>>,
        output: Option<std::rc::Rc<std::cell::RefCell<dyn OutputInterface>>>,
    ) -> anyhow::Result<i32> {
        let output = match output {
            Some(output) => Some(output),
            None => Some(
                std::rc::Rc::new(std::cell::RefCell::new(Factory::create_output()))
                    as std::rc::Rc<std::cell::RefCell<dyn OutputInterface>>,
            ),
        };

        Application::base_run(application, input, output)
    }

    pub fn do_run(
        application: &std::rc::Rc<std::cell::RefCell<Application>>,
        input: std::rc::Rc<std::cell::RefCell<dyn InputInterface>>,
        output: std::rc::Rc<std::cell::RefCell<dyn OutputInterface>>,
    ) -> anyhow::Result<i32> {
        application.borrow_mut().disable_plugins_by_default = input
            .borrow()
            .has_parameter_option(PhpMixed::from(vec!["--no-plugins"]), false);
        application.borrow_mut().disable_scripts_by_default = input
            .borrow()
            .has_parameter_option(PhpMixed::from(vec!["--no-scripts"]), false);

        let stdin = shirabe_php_shim::STDIN;
        if Platform::get_env("COMPOSER_TESTS_ARE_RUNNING").as_deref() != Some("1")
            && (Platform::get_env("COMPOSER_NO_INTERACTION").is_some()
                || !Platform::is_tty(Some(stdin)))
        {
            input.borrow_mut().set_interactive(false);
        }

        let mut helpers: IndexMap<
            HelperSetKey,
            std::rc::Rc<std::cell::RefCell<dyn HelperInterface>>,
        > = IndexMap::new();
        helpers.insert(
            HelperSetKey::Int(0),
            std::rc::Rc::new(std::cell::RefCell::new(QuestionHelper::default())),
        );
        let helper_set = std::rc::Rc::new(std::cell::RefCell::new(HelperSet::default()));
        HelperSet::new(&helper_set, helpers);
        application.borrow_mut().io = std::rc::Rc::new(std::cell::RefCell::new(ConsoleIO::new(
            input.clone(),
            output.clone(),
            helper_set.borrow().clone(),
        )));
        // Cache the IO so the rest of the flow does not re-borrow the application; command callbacks
        // (e.g. mergeApplicationDefinition) need the application's RefCell free.
        let io = application.borrow().io.clone();

        // Register error handler again to pass it the IO instance
        ErrorHandler::register(Some(io.clone()));

        if input
            .borrow()
            .has_parameter_option(PhpMixed::from(vec!["--no-cache"]), false)
        {
            io.write_error3("Disabling cache usage", true, io_interface::DEBUG);
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
        let new_work_dir = application.borrow().get_new_working_dir(input.clone())?;
        let mut old_working_dir: Option<String> = None;
        if let Some(ref nwd) = new_work_dir {
            old_working_dir = Some(Platform::get_cwd(true).unwrap_or_default());
            chdir(nwd);
            application.borrow_mut().initial_working_directory = getcwd();
            let cwd = Platform::get_cwd(true).unwrap_or_default();
            io.write_error3(
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
        let raw_command_name = application
            .borrow_mut()
            .get_command_name_before_binding(input.clone())?;
        if let Some(ref raw) = raw_command_name {
            let find_result = application.borrow_mut().find(raw);
            match find_result {
                Ok(cmd) => {
                    command_name = cmd.borrow().get_name();
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
        let use_parent_dir_if_no_json_available =
            application.borrow().get_use_parent_dir_config_value();
        let no_composer_json_commands_pm = PhpMixed::List(
            no_composer_json_commands
                .iter()
                .map(|s| PhpMixed::String(s.clone()))
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
                || (!input
                    .borrow()
                    .has_parameter_option(PhpMixed::from(vec!["--file"]), true)
                    && !input
                        .borrow()
                        .has_parameter_option(PhpMixed::from(vec!["-f"]), true)))
            && !input
                .borrow()
                .has_parameter_option(PhpMixed::from(vec!["--help"]), true)
            && !input
                .borrow()
                .has_parameter_option(PhpMixed::from(vec!["-h"]), true)
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
            is_non_allowed_root = application.borrow().is_running_as_root();

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
            PhpMixed::String("".to_string()),
            PhpMixed::String("list".to_string()),
            PhpMixed::String("help".to_string()),
        ]);
        let may_need_plugin_command = !input
            .borrow()
            .has_parameter_option(PhpMixed::from(vec!["--version", "-V"]), false)
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

        if may_need_plugin_command
            && !application.borrow().disable_plugins_by_default
            && !application.borrow().has_plugin_commands
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

            // TODO(phase-b): the original PHP catches plugin discovery exceptions in a
            // try/catch. The Rust port keeps the loop but skips IO error reporting
            // because get_plugin_commands borrows &mut self, conflicting with io.
            let mut plugin_warnings: Vec<String> = Vec::new();
            match (|| -> anyhow::Result<()> {
                let plugin_commands = application.borrow_mut().get_plugin_commands()?;
                for command in plugin_commands {
                    let cmd_name = command.get_name().unwrap_or_default();
                    if application.borrow_mut().has(&cmd_name) {
                        // TODO(plugin): PHP uses get_class($command) for the skipped-command class
                        // name. Plugin command discovery (get_plugin_commands) is unimplemented, so
                        // this loop never runs; wire the concrete class name with the plugin API.
                        let cls = String::new();
                        plugin_warnings.push(format!("<warning>Plugin command {} ({}) would override a Composer command and has been skipped</warning>", cmd_name, cls));
                    } else {
                        // Compatibility layer for symfony/console <7.4
                        // TODO(phase-c): registering a plugin command needs the Symfony
                        // Application's typed add()/addCommand(); the external-package stub keeps
                        // its registry as PhpMixed/todo!() per the "Symfony stays todo!()" policy.
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

                        let mut ghe = GithubActionError::new(io.clone());
                        ghe.emit(&pe.message, file.as_deref(), line);

                        return Err(e);
                    } else {
                        return Err(e);
                    }
                }
            }
            for warning in &plugin_warnings {
                io.write_error(warning);
            }

            application.borrow_mut().has_plugin_commands = true;
        }

        if !application.borrow().disable_plugins_by_default
            && is_non_allowed_root
            && !io.is_interactive()
        {
            io.write_error("<error>Composer plugins have been disabled for safety in this non-interactive session.</error>");
            io.write_error("<error>Set COMPOSER_ALLOW_SUPERUSER=1 if you want to allow plugins to run as root/super user.</error>");
            application.borrow_mut().disable_plugins_by_default = true;
        }

        // determine command name to be executed incl plugin commands, and check if it's a proxy command
        let mut is_proxy_command = false;
        let resolved_command_name = application
            .borrow_mut()
            .get_command_name_before_binding(input.clone())?;
        if let Some(ref name) = resolved_command_name {
            let find_result = application.borrow_mut().find(name);
            if let Ok(command) = find_result {
                command_name = command.borrow().get_name();
                // PHP: $command instanceof Command\BaseCommand && $command->isProxyCommand().
                // The Symfony `Command` trait exposes `is_proxy_command` (default false) so the
                // `dyn SymfonyCommand` registry can answer this; Composer proxy commands override it.
                is_proxy_command = command.borrow().is_proxy_command();
            }
        }

        if !is_proxy_command {
            io.write_error3(
                &format!(
                    "Running {} ({}) with {} on {}",
                    composer::get_version(),
                    composer::RELEASE_DATE,
                    (if defined("HHVM_VERSION") {
                        format!("HHVM {}", shirabe_php_shim::HHVM_VERSION.unwrap_or(""))
                    } else {
                        format!("PHP {}", PHP_VERSION)
                    }),
                    (if function_exists("php_uname") {
                        format!("{} / {}", php_uname("s"), php_uname("r"))
                    } else {
                        "Unknown OS".to_string()
                    }),
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
                io.write_error(&format!(
                    "<warning>Warning: This development build of Composer is over 60 days old. It is recommended to update it by running \"{} self-update\" to get the latest version.</warning>",
                    shirabe_php_shim::server_get("PHP_SELF").unwrap_or_default(),
                ));
            }

            if is_non_allowed_root
                && command_name.as_deref() != Some("self-update")
                && command_name.as_deref() != Some("selfupdate")
                && command_name.as_deref() != Some("_complete")
            {
                io.write_error("<warning>Do not run Composer as root/super user! See https://getcomposer.org/root for details</warning>");

                if io.is_interactive()
                    && !io.ask_confirmation(
                        "<info>Continue as root/super user</info> [<comment>yes</comment>]? "
                            .to_string(),
                        true,
                    )
                {
                    return Ok(1);
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
                    return Ok(Some(format!("<error>PHP temp directory ({}) does not exist or is not writable to Composer. Set sys_temp_dir in your php.ini</error>", sys_get_temp_dir())));
                }
                Ok(None)
            })
            .ok()
            .flatten();
            if let Some(msg) = tempfile_msg {
                io.write_error(&msg);
            }

            // add non-standard scripts as own commands
            let file = Factory::get_composer_file().unwrap_or_default();
            if may_need_script_command && is_file(&file) && Filesystem::is_readable(&file) {
                let composer_json: PhpMixed =
                    json_decode(&file_get_contents(&file).unwrap_or_default(), true)
                        .unwrap_or(PhpMixed::Null);
                if let Some(arr) = composer_json.as_array()
                    && let Some(scripts) = arr.get("scripts").and_then(|v| v.as_array())
                {
                    for (script, dummy) in scripts {
                        let script_event_const = format!(
                            "Composer\\Script\\ScriptEvents::{}",
                            str_replace("-", "_", &strtoupper(script))
                        );
                        if !defined(&script_event_const) {
                            if application.borrow_mut().has(script) {
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
                                            .filter_map(|v| v.as_string().map(|s| s.to_string()))
                                            .collect()
                                    })
                                    .unwrap_or_default();

                                let composer_opt =
                                    application.borrow_mut().get_composer(false, None, None)?;
                                if let Some(composer) = composer_opt {
                                    let composer = crate::command::composer_full(&composer);
                                    let root_package = composer.get_package();
                                    let generator = composer.get_autoload_generator().clone();
                                    let generator = generator.borrow();

                                    let installation_manager = composer.get_installation_manager();
                                    let package_map = generator.build_package_map(
                                        &mut installation_manager.borrow_mut(),
                                        root_package.clone(),
                                        vec![],
                                    )?;
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

                                // if the command is not an array of commands, and points to a valid SymfonyCommand subclass, import its details directly
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
                                        io.write_error(&format!("<warning>The script named {} extends SingleCommandApplication which is not compatible with Composer 2.9+, make sure you extend Symfony\\Component\\Console\\Command instead.</warning>", script));
                                    }
                                    let mut cmd = shirabe_php_shim::instantiate_class(
                                        &dummy_str,
                                        vec![PhpMixed::String(script.clone())],
                                    );
                                    // TODO(phase-c): the script's command class is built by
                                    // reflection (instantiate_class) and stays PhpMixed; the
                                    // SingleCommandApplication / SymfonyCommand typed registry it
                                    // belongs to is an external-package todo!() stub.
                                    // let _ = SingleCommandApplication::new;

                                    // makes sure the command is find()'able by the name defined in composer.json, and the name isn't overridden in its configure()
                                    // TODO(phase-c): cmd is the PhpMixed result of reflection
                                    // instantiation; reading/overriding its
                                    // name/description requires the typed SymfonyCommand model that
                                    // the Symfony stub does not yet provide.
                                    let _ = description.clone();
                                    let _ = &mut cmd;
                                    cmd
                                } else {
                                    // fallback to usual aliasing behavior
                                    // TODO(phase-c): ScriptAliasCommand is a typed BaseCommand
                                    // but this code path stores commands as PhpMixed; it can
                                    // only be carried as a typed trait object once the Symfony
                                    // command registry is modelled.
                                    let _ = ScriptAliasCommand::new(
                                        script.clone(),
                                        Some(description.clone()),
                                        aliases,
                                    );
                                    PhpMixed::Null
                                };

                                // Compatibility layer for symfony/console <7.4
                                // TODO(phase-c): Application::add() takes Rc<RefCell<dyn
                                // SymfonyCommand>>
                                // but `cmd` here is the PhpMixed result of reflection-based
                                // plugin command instantiation; registering it as a typed
                                // command instance is blocked on the Symfony command-registry
                                // model (external-package todo!() stub).
                                let _ = &cmd;
                                todo!(
                                    "plugin: register reflection-instantiated command on Application::add"
                                );
                            }
                        }
                    }
                }
            }
        }

        let mut start_time: Option<f64> = None;
        let result_outcome: anyhow::Result<i32> = (|| -> anyhow::Result<i32> {
            if input
                .borrow()
                .has_parameter_option(PhpMixed::from(vec!["--profile"]), false)
            {
                start_time = Some(microtime());
                // PHP: $this->io->enableDebugging($startTime).
                // TODO(phase-c): enableDebugging exists only on ConsoleIO, not on IOInterface,
                // and self.io is still the NullIO because the ConsoleIO construction above is
                // deferred (Symfony HelperSet/Helper modelling). Once self.io is the real
                // ConsoleIO this becomes a concrete-type call on it.
                let _ = start_time.unwrap();
            }

            let result = Application::base_do_run(application, input.clone(), output.clone())?;

            if input
                .borrow()
                .has_parameter_option(PhpMixed::from(vec!["--version", "-V"]), true)
            {
                io.write_error(&format!(
                    "<info>PHP</info> version <comment>{}</comment> ({})",
                    PHP_VERSION, PHP_BINARY,
                ));
                io.write_error(
                    "Run the \"diagnose\" command to get more detailed diagnostics output.",
                );
            }

            // chdir back to oldWorkingDir if set
            if let Some(ref owd) = old_working_dir
                && !owd.is_empty()
            {
                let owd = owd.clone();
                let _ = Silencer::call(|| {
                    chdir(&owd);
                    Ok(())
                });
            }

            if let Some(st) = start_time {
                io.write_error(&format!(
                    "<info>Memory usage: {}MiB (peak: {}MiB), time: {}s</info>",
                    round((memory_get_usage() as f64) / 1024.0 / 1024.0, 2),
                    round((memory_get_peak_usage(false) as f64) / 1024.0 / 1024.0, 2),
                    round(microtime() - st, 2)
                ));
            }

            Ok(result)
        })();

        let outcome = match result_outcome {
            Ok(r) => Ok(r),
            Err(e) => {
                // PHP's `exit` bypasses parent::doRun()'s catch entirely; re-raise it untouched so
                // the GitHub Actions annotation and error hints below are skipped.
                if e.downcast_ref::<shirabe_php_shim::ExitException>()
                    .is_some()
                {
                    return Err(e);
                }
                if let Some(see) = e.downcast_ref::<ScriptExecutionException>() {
                    if application.borrow().get_disable_plugins_by_default()
                        && application.borrow().is_running_as_root()
                        && !io.is_interactive()
                    {
                        io.write_error3("<error>Plugins have been disabled automatically as you are running as root, this may be the cause of the script failure.</error>", true, io_interface::QUIET);
                        io.write_error3(
                            "<error>See also https://getcomposer.org/root</error>",
                            true,
                            io_interface::QUIET,
                        );
                    }

                    Ok(see.get_code() as i32)
                } else {
                    let mut ghe = GithubActionError::new(io.clone());
                    ghe.emit(&e.to_string(), None, None);

                    application.borrow_mut().hint_common_errors(&e, output);

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

    fn get_new_working_dir(
        &self,
        input: std::rc::Rc<std::cell::RefCell<dyn InputInterface>>,
    ) -> anyhow::Result<Option<String>> {
        let working_dir = input
            .borrow()
            .get_parameter_option(
                PhpMixed::from(vec!["--working-dir", "-d"]),
                PhpMixed::Null,
                true,
            )
            .as_string()
            .map(|s| s.to_string());
        if let Some(ref wd) = working_dir
            && !is_dir(wd)
        {
            return Err(RuntimeException {
                message: format!(
                    "Invalid working directory specified, {} does not exist.",
                    wd
                ),
                code: 0,
            }
            .into());
        }

        Ok(working_dir)
    }

    fn hint_common_errors(
        &mut self,
        exception: &anyhow::Error,
        output: std::rc::Rc<std::cell::RefCell<dyn OutputInterface>>,
    ) {
        let is_logic_or_error = exception.downcast_ref::<ShimLogicException>().is_some();
        if is_logic_or_error
            && output.borrow().get_verbosity() < output_interface::VERBOSITY_VERBOSE
        {
            output
                .borrow_mut()
                .set_verbosity(output_interface::VERBOSITY_VERBOSE);
        }

        Silencer::suppress(None);
        // Compute the disk-space hint message first; emit it via io afterwards to
        // avoid overlapping borrows of self (get_composer needs &mut self).
        let disk_hint_msg: Option<String> = (|| -> anyhow::Result<Option<String>> {
            let composer = self.get_composer(false, Some(true), None)?;
            if let Some(composer) = composer && function_exists("disk_free_space") {
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
                    .map(|s| PhpMixed::String(s.clone()))
                    .collect(),
            );
            if is_array(&avast_detect_pm) && !avast_detect.is_empty() {
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
                Ok(c) => self.composer = Some(c.upcast()),
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
                            if self.are_exceptions_caught() {
                                // PHP calls `exit(1)` here, terminating before parent::run() can
                                // re-render the exception. Propagate it as an ExitException so the
                                // top-level handler turns it into exit code 1 without re-rendering.
                                return Err(shirabe_php_shim::ExitException { code: 1 }.into());
                            }
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
        let io = self.get_io();
        if let Some(base_io) = io.borrow_mut().as_base_io_mut() {
            base_io.reset_authentications();
        }
    }

    pub fn get_io(&self) -> std::rc::Rc<std::cell::RefCell<dyn IOInterface>> {
        self.io.clone()
    }

    pub fn get_help(&self) -> String {
        format!("{}{}", Self::LOGO, self.base_get_help())
    }

    /// Initializes all the composer commands.
    pub(crate) fn get_default_commands(
        &self,
    ) -> Vec<std::rc::Rc<std::cell::RefCell<dyn SymfonyCommand>>> {
        let mut commands = self.base_get_default_commands();
        let composer_commands: Vec<std::rc::Rc<std::cell::RefCell<dyn SymfonyCommand>>> = vec![
            std::rc::Rc::new(std::cell::RefCell::new(AboutCommand::new())),
            std::rc::Rc::new(std::cell::RefCell::new(ConfigCommand::new())),
            std::rc::Rc::new(std::cell::RefCell::new(DependsCommand::new())),
            std::rc::Rc::new(std::cell::RefCell::new(ProhibitsCommand::new())),
            std::rc::Rc::new(std::cell::RefCell::new(InitCommand::new())),
            std::rc::Rc::new(std::cell::RefCell::new(InstallCommand::new())),
            std::rc::Rc::new(std::cell::RefCell::new(CreateProjectCommand::new())),
            std::rc::Rc::new(std::cell::RefCell::new(UpdateCommand::new())),
            std::rc::Rc::new(std::cell::RefCell::new(SearchCommand::new())),
            std::rc::Rc::new(std::cell::RefCell::new(ValidateCommand::new())),
            std::rc::Rc::new(std::cell::RefCell::new(AuditCommand::new())),
            std::rc::Rc::new(std::cell::RefCell::new(ShowCommand::new())),
            std::rc::Rc::new(std::cell::RefCell::new(SuggestsCommand::new())),
            std::rc::Rc::new(std::cell::RefCell::new(RequireCommand::new())),
            std::rc::Rc::new(std::cell::RefCell::new(DumpAutoloadCommand::new())),
            std::rc::Rc::new(std::cell::RefCell::new(StatusCommand::new())),
            std::rc::Rc::new(std::cell::RefCell::new(ArchiveCommand::new())),
            std::rc::Rc::new(std::cell::RefCell::new(DiagnoseCommand::new())),
            std::rc::Rc::new(std::cell::RefCell::new(RunScriptCommand::new())),
            std::rc::Rc::new(std::cell::RefCell::new(LicensesCommand::new())),
            std::rc::Rc::new(std::cell::RefCell::new(GlobalCommand::new())),
            std::rc::Rc::new(std::cell::RefCell::new(ClearCacheCommand::new())),
            std::rc::Rc::new(std::cell::RefCell::new(RemoveCommand::new())),
            std::rc::Rc::new(std::cell::RefCell::new(HomeCommand::new())),
            std::rc::Rc::new(std::cell::RefCell::new(ExecCommand::new())),
            std::rc::Rc::new(std::cell::RefCell::new(OutdatedCommand::new())),
            std::rc::Rc::new(std::cell::RefCell::new(CheckPlatformReqsCommand::new())),
            std::rc::Rc::new(std::cell::RefCell::new(FundCommand::new())),
            std::rc::Rc::new(std::cell::RefCell::new(ReinstallCommand::new())),
            std::rc::Rc::new(std::cell::RefCell::new(BumpCommand::new())),
            std::rc::Rc::new(std::cell::RefCell::new(RepositoryCommand::new())),
            std::rc::Rc::new(std::cell::RefCell::new(SelfUpdateCommand::new())),
        ];
        commands.extend(composer_commands);
        commands
    }

    /// This ensures we can find the correct command name even if a global input option is present before it
    ///
    /// e.g. "composer -d foo bar" should detect bar as the command name, and not foo
    fn get_command_name_before_binding(
        &mut self,
        input: std::rc::Rc<std::cell::RefCell<dyn InputInterface>>,
    ) -> anyhow::Result<Option<String>> {
        let input = input.borrow().dup();
        // Makes ArgvInput::getFirstArgument() able to distinguish an option from an argument.
        match input.borrow_mut().bind(&self.get_definition().borrow()) {
            Ok(()) => {}
            Err(e) => {
                // Errors must be ignored, full binding/validation happens later when the command is known.
                if !is_exception_interface(&e) {
                    return Err(e);
                }
            }
        }

        Ok(input.borrow().get_first_argument())
    }

    pub fn get_long_version(&self) -> String {
        let mut branch_alias_string = String::new();
        if !composer::BRANCH_ALIAS_VERSION.is_empty()
            && composer::BRANCH_ALIAS_VERSION != "@package_branch_alias_version@"
        {
            branch_alias_string = format!(" ({})", composer::BRANCH_ALIAS_VERSION,);
        }

        format!(
            "<info>{}</info> version <comment>{}{}</comment> {}",
            self.get_name(),
            self.get_version(),
            branch_alias_string,
            composer::RELEASE_DATE,
        )
    }

    pub(crate) fn get_default_input_definition(&self) -> anyhow::Result<InputDefinition> {
        let mut definition = self.base_get_default_input_definition();
        definition.add_option(InputOption::new(
            "--profile",
            PhpMixed::Null,
            Some(InputOption::VALUE_NONE),
            "Display timing and memory usage information".to_string(),
            PhpMixed::Null,
        )?)?;
        definition.add_option(InputOption::new(
            "--no-plugins",
            PhpMixed::Null,
            Some(InputOption::VALUE_NONE),
            "Whether to disable plugins.".to_string(),
            PhpMixed::Null,
        )?)?;
        definition.add_option(InputOption::new(
            "--no-scripts",
            PhpMixed::Null,
            Some(InputOption::VALUE_NONE),
            "Skips the execution of all scripts defined in composer.json file.".to_string(),
            PhpMixed::Null,
        )?)?;
        definition.add_option(InputOption::new(
            "--working-dir",
            PhpMixed::from("-d"),
            Some(InputOption::VALUE_REQUIRED),
            "If specified, use the given directory as working directory.".to_string(),
            PhpMixed::Null,
        )?)?;
        definition.add_option(InputOption::new(
            "--no-cache",
            PhpMixed::Null,
            Some(InputOption::VALUE_NONE),
            "Prevent use of the cache".to_string(),
            PhpMixed::Null,
        )?)?;

        Ok(definition)
    }

    fn get_plugin_commands(&mut self) -> anyhow::Result<Vec<Box<dyn SymfonyCommand>>> {
        // TODO(plugin): plugin command discovery is part of the plugin API
        let commands: Vec<Box<dyn SymfonyCommand>> = vec![];

        // TODO(phase-c): discovering plugin-provided commands walks the PluginManager and
        // downcasts each plugin's CommandProvider capability — this is the Plugin API surface,
        // which is intentionally unimplemented (see TODO(plugin) above). Returns an empty list
        // until the plugin capability model exists.

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

/// Methods inherited from `Symfony\Component\Console\Application`. They live in the same `impl`
/// surface as the Composer overrides above so that polymorphic `self.*` calls dispatch to the
/// Composer version when one exists. Methods that Composer overrides while still calling `parent::`
/// are carried here under a `base_` prefix (`base_run`, `base_do_run`, `base_get_help`,
/// `base_get_default_input_definition`, `base_get_default_commands`).
impl Application {
    pub fn set_command_loader(&mut self, command_loader: Box<dyn CommandLoaderInterface>) {
        self.command_loader = Some(command_loader);
    }

    pub fn get_signal_registry(&self) -> anyhow::Result<&SignalRegistry> {
        match &self.signal_registry {
            None => Err(ConsoleRuntimeException(shirabe_php_shim::RuntimeException {
                message: "Signals are not supported. Make sure that the `pcntl` extension is installed and that \"pcntl_*\" functions are not disabled by your php.ini's \"disable_functions\" directive.".to_string(),
                code: 0,
            })
            .into()),
            Some(signal_registry) => Ok(signal_registry),
        }
    }

    pub fn set_signals_to_dispatch_event(&mut self, signals_to_dispatch_event: Vec<i64>) {
        self.signals_to_dispatch_event = signals_to_dispatch_event;
    }

    /// Runs the current application (Symfony base; `parent::run`).
    pub fn base_run(
        application: &std::rc::Rc<std::cell::RefCell<Application>>,
        input: Option<std::rc::Rc<std::cell::RefCell<dyn InputInterface>>>,
        output: Option<std::rc::Rc<std::cell::RefCell<dyn OutputInterface>>>,
    ) -> anyhow::Result<i32> {
        if shirabe_php_shim::function_exists("putenv") {
            let (height, width) = {
                let app = application.borrow();
                (app.terminal.get_height(), app.terminal.get_width())
            };
            shirabe_php_shim::putenv(&format!("LINES={}", height));
            shirabe_php_shim::putenv(&format!("COLUMNS={}", width));
        }

        let input: std::rc::Rc<std::cell::RefCell<dyn InputInterface>> = match input {
            None => std::rc::Rc::new(std::cell::RefCell::new(ArgvInput::new(None, None)?)),
            Some(input) => input,
        };

        let output: std::rc::Rc<std::cell::RefCell<dyn OutputInterface>> = match output {
            None => std::rc::Rc::new(std::cell::RefCell::new(ConsoleOutput::new(
                None, None, None,
            )?)),
            Some(output) => output,
        };

        // TODO: PHP installs a temporary `set_exception_handler($renderException)` and cooperates
        // with Symfony's ErrorHandler to keep/restore it. PHP's process-global exception handler
        // stack has no Rust equivalent; the rendering itself is invoked directly in the catch
        // branch below. Review needed for the handler save/restore dance.
        let render_exception =
            |this: &Application,
             e: &anyhow::Error,
             output: &std::rc::Rc<std::cell::RefCell<dyn OutputInterface>>| {
                // if ($output instanceof ConsoleOutputInterface) render to its error output
                // TODO(review): downcasting a `dyn OutputInterface` to `ConsoleOutputInterface`
                // is not directly expressible; the ConsoleOutputInterface branch needs design.
                this.render_throwable(e, output.clone());
            };

        let result = (|| -> anyhow::Result<i32> {
            application.borrow_mut().configure_io(&input, &output)?;

            let exit_code = Application::do_run(application, input.clone(), output.clone())?;

            Ok(exit_code)
        })();

        let exit_code = match result {
            Ok(exit_code) => exit_code,
            Err(e) => {
                // PHP's `exit` bypasses Symfony's try/catch: it never renders and forces the exit
                // code it carries. The message, if any, has already been written at the exit site.
                if let Some(exit) = e.downcast_ref::<shirabe_php_shim::ExitException>() {
                    return Ok(exit.code as i32);
                }

                if !application.borrow().catch_exceptions {
                    return Err(e);
                }

                render_exception(&application.borrow(), &e, &output);

                // $exitCode = $e->getCode();
                // is_numeric($exitCode) ? max(1, (int) $exitCode) : 1
                // TODO(review): anyhow::Error has no PHP-style getCode(); the exit code derived
                // from the exception's `code` field needs the downcast strategy decided.
                let exit_code = shirabe_php_shim::php_exception_get_code(&e);
                if shirabe_php_shim::is_numeric_string(&exit_code.to_string()) {
                    if exit_code <= 0 { 1 } else { exit_code }
                } else {
                    1
                }
            }
        };

        // finally: handler restore. See TODO above; no-op here.

        Ok(exit_code)
    }

    /// Runs the current application (Symfony base; `parent::doRun`).
    pub fn base_do_run(
        application: &std::rc::Rc<std::cell::RefCell<Application>>,
        input: std::rc::Rc<std::cell::RefCell<dyn InputInterface>>,
        output: std::rc::Rc<std::cell::RefCell<dyn OutputInterface>>,
    ) -> anyhow::Result<i32> {
        if input.borrow().has_parameter_option(
            PhpMixed::from(vec![
                PhpMixed::from("--version".to_string()),
                PhpMixed::from("-V".to_string()),
            ]),
            true,
        ) {
            let long_version = application.borrow().get_long_version();
            output
                .borrow()
                .writeln(&[long_version], output_interface::OUTPUT_NORMAL);

            return Ok(0);
        }

        // Makes ArgvInput::getFirstArgument() able to distinguish an option from an argument.
        let definition = application.borrow_mut().get_definition();
        match input.borrow_mut().bind(&definition.borrow()) {
            Ok(()) => {}
            Err(e) => {
                // Errors must be ignored, full binding/validation happens later when the command is known.
                if !is_exception_interface(&e) {
                    return Err(e);
                }
            }
        }

        let mut input = input;
        let mut name = application.borrow().get_command_name(&*input.borrow());
        if input.borrow().has_parameter_option(
            PhpMixed::from(vec![
                PhpMixed::from("--help".to_string()),
                PhpMixed::from("-h".to_string()),
            ]),
            true,
        ) {
            if name.is_none() {
                name = Some("help".to_string());
                let default_command = application.borrow().default_command.clone();
                input = std::rc::Rc::new(std::cell::RefCell::new(ArrayInput::new(
                    vec![(
                        PhpMixed::from("command_name".to_string()),
                        PhpMixed::from(default_command),
                    )],
                    None,
                )?));
            } else {
                application.borrow_mut().want_helps = true;
            }
        }

        let name = match name {
            Some(name) => name,
            None => {
                let name = application.borrow().default_command.clone();
                let definition = application.borrow_mut().get_definition();
                let command_description = definition
                    .borrow()
                    .get_argument(&PhpMixed::from("command".to_string()))?
                    .get_description()
                    .to_string();
                let new_command_argument = InputArgument::new(
                    "command".to_string(),
                    Some(InputArgument::OPTIONAL),
                    command_description,
                    PhpMixed::from(name.clone()),
                )?;
                // $definition->setArguments(array_merge($definition->getArguments(),
                //     ['command' => new InputArgument('command', InputArgument::OPTIONAL, ...)]))
                let mut merged_arguments: Vec<InputArgument> = Vec::new();
                let mut replaced_command = false;
                for (key, argument) in definition.borrow().get_arguments() {
                    if key == "command" {
                        merged_arguments.push(new_command_argument.clone());
                        replaced_command = true;
                    } else {
                        merged_arguments.push((**argument).clone());
                    }
                }
                if !replaced_command {
                    merged_arguments.push(new_command_argument);
                }
                definition.borrow_mut().set_arguments(merged_arguments)?;

                name
            }
        };

        let command: std::rc::Rc<std::cell::RefCell<dyn SymfonyCommand>>;
        application.borrow_mut().running_command = None;
        // the command name MUST be the first element of the input
        let find_result = application.borrow_mut().find(&name);

        match find_result {
            Ok(c) => {
                command = c;
            }
            Err(e) => {
                // if (!($e instanceof CommandNotFoundException && !$e instanceof NamespaceNotFoundException)
                //     || 1 !== count($alternatives = $e->getAlternatives()) || !$input->isInteractive())
                let alternatives: Option<Vec<String>> = downcast_command_not_found(&e)
                    .filter(|_| !is_namespace_not_found(&e))
                    .map(|cnf| cnf.get_alternatives().clone());

                let single_alternative = match &alternatives {
                    Some(alts) if alts.len() == 1 => Some(alts[0].clone()),
                    _ => None,
                };

                if single_alternative.is_none() || !input.borrow().is_interactive() {
                    return Err(e);
                }

                let alternative = single_alternative.unwrap();

                let mut style = SymfonyStyle::new(input.clone(), output.clone());
                output
                    .borrow()
                    .writeln(&["".to_string()], output_interface::OUTPUT_NORMAL);
                let formatted_block = FormatterHelper::default().format_block(
                    FormatBlockMessages::String(format!(
                        "Command \"{}\" is not defined.",
                        PhpMixed::from(name.clone()),
                    )),
                    "error",
                    true,
                );
                output
                    .borrow()
                    .writeln(&[formatted_block], output_interface::OUTPUT_NORMAL);
                if !style.confirm(
                    &format!(
                        "Do you want to run \"{}\" instead? ",
                        PhpMixed::from(alternative.clone()),
                    ),
                    false,
                ) {
                    return Ok(1);
                }

                command = application.borrow_mut().find(&alternative)?;
            }
        }

        // if ($command instanceof LazyCommand) $command = $command->getCommand();
        // TODO(review): LazyCommand is a distinct type from SymfonyCommand here; PHP unwraps the real
        // command. The `commands` map stores Rc<RefCell<dyn SymfonyCommand>>, so the LazyCommand-unwrap path
        // needs a design decision about how lazy commands are represented.
        let _ = std::marker::PhantomData::<LazyCommand>;

        application.borrow_mut().running_command = Some(command.clone());
        // do_run_command invokes the command's run(), which calls back into the application
        // (mergeApplicationDefinition etc.); it must run with no application borrow held.
        let exit_code = Application::do_run_command(
            application,
            command.clone(),
            input.clone(),
            output.clone(),
        )?;
        application.borrow_mut().running_command = None;

        Ok(exit_code)
    }

    pub fn set_helper_set(&mut self, helper_set: std::rc::Rc<std::cell::RefCell<HelperSet>>) {
        self.helper_set = Some(helper_set);
    }

    /// Get the helper set associated with the command.
    pub fn get_helper_set(&mut self) -> std::rc::Rc<std::cell::RefCell<HelperSet>> {
        if self.helper_set.is_none() {
            self.helper_set = Some(self.get_default_helper_set());
        }

        self.helper_set.as_ref().unwrap().clone()
    }

    pub fn set_definition(&mut self, definition: std::rc::Rc<std::cell::RefCell<InputDefinition>>) {
        self.definition = Some(definition);
    }

    /// Gets the InputDefinition related to this Application.
    pub fn get_definition(&mut self) -> std::rc::Rc<std::cell::RefCell<InputDefinition>> {
        if self.definition.is_none() {
            // `get_default_input_definition` is the Composer override (returns a Result because the
            // Rust `InputOption::new` is fallible); the option modes are constants that cannot fail,
            // so unwrapping mirrors the PHP call that never throws here.
            self.definition = Some(std::rc::Rc::new(std::cell::RefCell::new(
                self.get_default_input_definition().unwrap(),
            )));
        }

        if self.single_command {
            let input_definition = self.definition.as_ref().unwrap().clone();
            input_definition
                .borrow_mut()
                .set_arguments(Vec::new())
                .unwrap();

            return input_definition;
        }

        self.definition.as_ref().unwrap().clone()
    }

    /// Adds suggestions to `suggestions` for the current completion input (e.g. option or argument).
    pub fn complete(
        &mut self,
        input: &CompletionInput,
        suggestions: &mut CompletionSuggestions,
    ) -> anyhow::Result<()> {
        if CompletionInput::TYPE_ARGUMENT_VALUE == input.get_completion_type()
            && input.get_completion_name().as_deref() == Some("command")
        {
            let mut command_names: Vec<PhpMixed> = Vec::new();
            for (name, command) in self.all(None)? {
                // skip hidden commands and aliased commands as they already get added below
                if command.borrow().is_hidden() || command.borrow().get_name() != Some(name.clone())
                {
                    continue;
                }
                command_names.push(PhpMixed::from(
                    command.borrow().get_name().unwrap_or_default(),
                ));
                for name in command.borrow().get_aliases() {
                    command_names.push(PhpMixed::from(name));
                }
            }
            // array_filter($commandNames)
            let filtered: Vec<shirabe_external_packages::symfony::console::completion::completion_suggestions::StringOrSuggestion> =
                command_names
                    .into_iter()
                    .filter(shirabe_php_shim::php_truthy)
                    .map(|n| {
                        shirabe_external_packages::symfony::console::completion::completion_suggestions::StringOrSuggestion::String(
                            shirabe_php_shim::php_to_string(&n),
                        )
                    })
                    .collect();
            suggestions.suggest_values(filtered);

            return Ok(());
        }

        if CompletionInput::TYPE_OPTION_NAME == input.get_completion_type() {
            // $suggestions->suggestOptions($this->getDefinition()->getOptions());
            // TODO(review): get_options() yields Rc<InputOption> (shared, non-Clone) while
            // suggest_options() consumes owned InputOption values; an ownership/clone strategy
            // for InputOption is needed.
            suggestions.suggest_options(todo!("owned options from get_definition().get_options()"));

            return Ok(());
        }

        Ok(())
    }

    /// Gets the help message (Symfony base; `parent::getHelp`).
    pub fn base_get_help(&self) -> String {
        self.get_long_version()
    }

    /// Gets whether to catch exceptions or not during commands execution.
    pub fn are_exceptions_caught(&self) -> bool {
        self.catch_exceptions
    }

    /// Sets whether to catch exceptions or not during commands execution.
    pub fn set_catch_exceptions(&mut self, boolean: bool) {
        self.catch_exceptions = boolean;
    }

    /// Gets the name of the application.
    pub fn get_name(&self) -> String {
        self.name.clone()
    }

    /// Sets the application name.
    pub fn set_name(&mut self, name: &str) {
        self.name = name.to_string();
    }

    /// Gets the application version.
    pub fn get_version(&self) -> String {
        self.version.clone()
    }

    /// Sets the application version.
    pub fn set_version(&mut self, version: &str) {
        self.version = version.to_string();
    }

    /// Adds an array of command objects.
    ///
    /// If a SymfonyCommand is not enabled it will not be added.
    pub fn add_commands(
        &mut self,
        commands: Vec<std::rc::Rc<std::cell::RefCell<dyn SymfonyCommand>>>,
    ) -> anyhow::Result<()> {
        for command in commands {
            self.add(command)?;
        }
        Ok(())
    }

    /// Adds a command object.
    ///
    /// If a command with the same name already exists, it will be overridden.
    /// If the command is not enabled it will not be added.
    pub fn add(
        &mut self,
        command: std::rc::Rc<std::cell::RefCell<dyn SymfonyCommand>>,
    ) -> anyhow::Result<Option<std::rc::Rc<std::cell::RefCell<dyn SymfonyCommand>>>> {
        self.init()?;

        // TODO(review): $command->setApplication($this) needs an Rc<RefCell<Application>> to the
        // current instance. Application is held by value here; the self-reference required to set
        // the command's back-pointer needs the shared-ownership design (Phase C).
        command
            .borrow_mut()
            .set_application(todo!("Rc<RefCell<Application>> of self"));

        if !command.borrow().is_enabled() {
            command.borrow_mut().set_application(None);

            return Ok(None);
        }

        // if (!$command instanceof LazyCommand) { $command->getDefinition(); }
        // TODO(review): LazyCommand vs SymfonyCommand type distinction; eager definition probe omitted
        // pending lazy-command representation decision.
        command.borrow().get_definition();

        if command.borrow().get_name().is_none() {
            return Err(ConsoleLogicException(shirabe_php_shim::LogicException {
                message: format!(
                    "The command defined in \"{}\" cannot have an empty name.",
                    PhpMixed::from(shirabe_php_shim::get_debug_type_obj(&command,)),
                ),
                code: 0,
            })
            .into());
        }

        let name = command.borrow().get_name().unwrap();
        self.commands.insert(name, command.clone());

        for alias in command.borrow().get_aliases() {
            self.commands.insert(alias, command.clone());
        }

        Ok(Some(command))
    }

    /// Returns a registered command by name or alias.
    ///
    /// Throws CommandNotFoundException when given command name does not exist.
    pub fn get(
        &mut self,
        name: &str,
    ) -> anyhow::Result<std::rc::Rc<std::cell::RefCell<dyn SymfonyCommand>>> {
        self.init()?;

        if !self.has(name) {
            return Err(CommandNotFoundException::new(
                format!(
                    "The command \"{}\" does not exist.",
                    PhpMixed::from(name.to_string()),
                ),
                Vec::new(),
                0,
            )
            .into());
        }

        // When the command has a different name than the one used at the command loader level
        if !self.commands.contains_key(name) {
            return Err(CommandNotFoundException::new(
                format!(
                    "The \"{}\" command cannot be found because it is registered under multiple names. Make sure you don't set a different name via constructor or \"setName()\".",
                    PhpMixed::from(name.to_string()),
                ),
                Vec::new(),
                0,
            )
            .into());
        }

        let command = self.commands[name].clone();

        if self.want_helps {
            self.want_helps = false;

            let help_command = self.get("help")?;
            help_command
                .borrow_mut()
                .as_any_mut()
                .downcast_mut::<HelpCommand>()
                .expect("the help command is a HelpCommand instance")
                .set_command(command);

            return Ok(help_command);
        }

        Ok(command)
    }

    /// Returns true if the command exists, false otherwise.
    pub fn has(&mut self, name: &str) -> bool {
        self.init().unwrap();

        if self.commands.contains_key(name) {
            return true;
        }

        if let Some(command_loader) = &self.command_loader
            && command_loader.has(name)
        {
            let command = command_loader.get(name);
            // $this->add($this->commandLoader->get($name))
            // TODO(review): command_loader.get() returns Box<dyn SymfonyCommand> while add() expects
            // Rc<RefCell<dyn SymfonyCommand>>; the loader return type needs reconciliation.
            let _ = command;
            return self
                .add(todo!(
                    "Rc<RefCell<dyn SymfonyCommand>> from command_loader.get(name)"
                ))
                .map(|c| c.is_some())
                .unwrap_or(false);
        }

        false
    }

    /// Returns an array of all unique namespaces used by currently registered commands.
    ///
    /// It does not return the global namespace which always exists.
    pub fn get_namespaces(&mut self) -> anyhow::Result<Vec<String>> {
        let mut namespaces: Vec<Vec<String>> = Vec::new();
        for command in self.all(None)?.values() {
            if command.borrow().is_hidden() {
                continue;
            }

            namespaces.push(
                self.extract_all_namespaces(&command.borrow().get_name().unwrap_or_default()),
            );

            for alias in command.borrow().get_aliases() {
                namespaces.push(self.extract_all_namespaces(&alias));
            }
        }

        // array_values(array_unique(array_filter(array_merge([], ...$namespaces))))
        let mut merged: Vec<String> = Vec::new();
        for ns in namespaces {
            merged.extend(ns);
        }
        let merged: Vec<String> = merged.into_iter().filter(|s| !s.is_empty()).collect();
        let mut seen = std::collections::HashSet::new();
        let unique: Vec<String> = merged
            .into_iter()
            .filter(|s| seen.insert(s.clone()))
            .collect();

        Ok(unique)
    }

    /// Finds a registered namespace by a name or an abbreviation.
    ///
    /// Throws NamespaceNotFoundException when namespace is incorrect or ambiguous.
    pub fn find_namespace(&mut self, namespace: &str) -> anyhow::Result<String> {
        let all_namespaces = self.get_namespaces()?;
        // implode('[^:]*:', array_map('preg_quote', explode(':', $namespace))).'[^:]*'
        let parts: Vec<String> = shirabe_php_shim::explode(":", namespace)
            .into_iter()
            .map(|p| shirabe_php_shim::preg_quote(&p, None))
            .collect();
        let expr = format!("{}{}", shirabe_php_shim::implode("[^:]*:", &parts), "[^:]*");
        let namespaces = shirabe_php_shim::preg_grep(&format!("{{^{}}}", expr), &all_namespaces);

        if namespaces.is_empty() {
            let mut message = format!(
                "There are no commands defined in the \"{}\" namespace.",
                PhpMixed::from(namespace.to_string()),
            );

            let alternatives = self.find_alternatives(namespace, &all_namespaces);
            if !alternatives.is_empty() {
                if alternatives.len() == 1 {
                    message.push_str("\n\nDid you mean this?\n    ");
                } else {
                    message.push_str("\n\nDid you mean one of these?\n    ");
                }

                message.push_str(&shirabe_php_shim::implode("\n    ", &alternatives));
            }

            return Err(NamespaceNotFoundException(CommandNotFoundException::new(
                message,
                alternatives,
                0,
            ))
            .into());
        }

        let exact = namespaces.iter().any(|n| n == namespace);
        if namespaces.len() > 1 && !exact {
            return Err(NamespaceNotFoundException(CommandNotFoundException::new(
                format!(
                    "The namespace \"{}\" is ambiguous.\nDid you mean one of these?\n{}.",
                    PhpMixed::from(namespace.to_string()),
                    PhpMixed::from(self.get_abbreviation_suggestions(&namespaces)),
                ),
                namespaces.clone(),
                0,
            ))
            .into());
        }

        // $exact ? $namespace : reset($namespaces)
        if exact {
            Ok(namespace.to_string())
        } else {
            Ok(namespaces[0].clone())
        }
    }

    /// Finds a command by name or alias.
    ///
    /// Contrary to get, this command tries to find the best match if you give it an
    /// abbreviation of a name or alias.
    ///
    /// Throws CommandNotFoundException when command name is incorrect or ambiguous.
    pub fn find(
        &mut self,
        name: &str,
    ) -> anyhow::Result<std::rc::Rc<std::cell::RefCell<dyn SymfonyCommand>>> {
        self.init()?;

        let mut aliases: IndexMap<String, String> = IndexMap::new();

        let commands_snapshot: Vec<std::rc::Rc<std::cell::RefCell<dyn SymfonyCommand>>> =
            self.commands.values().cloned().collect();
        for command in &commands_snapshot {
            // A command's run() can re-enter find() (e.g. HelpCommand looks up the command it
            // describes). That command is mutably borrowed for the duration of its run(), so it
            // cannot be borrowed here. It is safe to skip: find() always completes this alias pass
            // before returning a command, so any command currently executing already had its
            // aliases registered in the earlier find() call that located it.
            // TODO: this work-around could be solved.
            let Ok(borrowed) = command.try_borrow() else {
                continue;
            };
            let aliases = borrowed.get_aliases();
            drop(borrowed);
            for alias in aliases {
                if !self.has(&alias) {
                    self.commands.insert(alias, command.clone());
                }
            }
        }

        if self.has(name) {
            return self.get(name);
        }

        // $allCommands = commandLoader ? array_merge(loader->getNames(), array_keys(commands)) : array_keys(commands)
        let all_commands: Vec<String> = match &self.command_loader {
            Some(command_loader) => {
                let mut all = command_loader.get_names();
                all.extend(self.commands.keys().cloned());
                all
            }
            None => self.commands.keys().cloned().collect(),
        };

        let parts: Vec<String> = shirabe_php_shim::explode(":", name)
            .into_iter()
            .map(|p| shirabe_php_shim::preg_quote(&p, None))
            .collect();
        let expr = format!("{}{}", shirabe_php_shim::implode("[^:]*:", &parts), "[^:]*");
        let mut commands = shirabe_php_shim::preg_grep(&format!("{{^{}}}", expr), &all_commands);

        if commands.is_empty() {
            commands = shirabe_php_shim::preg_grep(&format!("{{^{}}}i", expr), &all_commands);
        }

        // if no commands matched or we just matched namespaces
        if commands.is_empty()
            || shirabe_php_shim::preg_grep(&format!("{{^{}$}}i", expr), &commands).is_empty()
        {
            if let Some(pos) = shirabe_php_shim::strrpos(name, ":") {
                // check if a namespace exists and contains commands
                self.find_namespace(&name[..pos])?;
            }

            let mut message = format!(
                "SymfonyCommand \"{}\" is not defined.",
                PhpMixed::from(name.to_string()),
            );

            let mut alternatives = self.find_alternatives(name, &all_commands);
            if !alternatives.is_empty() {
                // remove hidden commands
                let mut filtered: Vec<String> = Vec::new();
                for alt in alternatives {
                    if !self.get(&alt)?.borrow().is_hidden() {
                        filtered.push(alt);
                    }
                }
                alternatives = filtered;

                if alternatives.len() == 1 {
                    message.push_str("\n\nDid you mean this?\n    ");
                } else {
                    message.push_str("\n\nDid you mean one of these?\n    ");
                }
                message.push_str(&shirabe_php_shim::implode("\n    ", &alternatives));
            }

            return Err(CommandNotFoundException::new(message, alternatives, 0).into());
        }

        // filter out aliases for commands which are already on the list
        if commands.len() > 1 {
            // $commandList = commandLoader ? array_merge(array_flip(loader->getNames()), commands) : commands
            // TODO(review): $commandList mixes flipped loader names (string => int) with
            // SymfonyCommand
            // instances; this heterogeneous PHP array needs a typed representation. The alias
            // de-duplication and the loader->get() lazy materialization are left to design.
            let mut command_list: IndexMap<
                String,
                std::rc::Rc<std::cell::RefCell<dyn SymfonyCommand>>,
            > = self.commands.clone();

            let commands_clone = commands.clone();
            let mut new_commands: Vec<String> = Vec::new();
            let mut seen = std::collections::HashSet::new();
            for name_or_alias in commands {
                if !command_list.contains_key(&name_or_alias) {
                    let loaded = self.command_loader.as_ref().unwrap().get(&name_or_alias);
                    let _ = loaded;
                    command_list.insert(
                        name_or_alias.clone(),
                        todo!(
                            "Rc<RefCell<dyn SymfonyCommand>> from command_loader.get(name_or_alias)"
                        ),
                    );
                }

                let command_name = command_list[&name_or_alias]
                    .borrow()
                    .get_name()
                    .unwrap_or_default();

                aliases.insert(name_or_alias.clone(), command_name.clone());

                let keep = command_name == name_or_alias || !commands_clone.contains(&command_name);
                if keep && seen.insert(name_or_alias.clone()) {
                    new_commands.push(name_or_alias);
                }
            }
            commands = new_commands;

            if commands.len() > 1 {
                let usable_width = self.terminal.get_width() - 10;
                let abbrevs: Vec<String> = commands.clone();
                let mut max_len: i64 = 0;
                for abbrev in &abbrevs {
                    max_len = std::cmp::max(Helper::width(abbrev), max_len);
                }
                let mut formatted_abbrevs: Vec<PhpMixed> = Vec::new();
                for cmd in commands.clone() {
                    if command_list[&cmd].borrow().is_hidden() {
                        // unset($commands[array_search($cmd, $commands)])
                        if let Some(idx) = commands.iter().position(|c| *c == cmd) {
                            commands.remove(idx);
                        }
                        formatted_abbrevs.push(PhpMixed::Bool(false));
                        continue;
                    }

                    let abbrev = format!(
                        "{} {}",
                        shirabe_php_shim::str_pad(
                            &cmd,
                            max_len as usize,
                            " ",
                            shirabe_php_shim::STR_PAD_RIGHT
                        ),
                        command_list[&cmd].borrow().get_description()
                    );

                    if Helper::width(&abbrev) > usable_width {
                        formatted_abbrevs.push(PhpMixed::from(format!(
                            "{}...",
                            Helper::substr(&abbrev, 0, Some(usable_width - 3))
                        )));
                    } else {
                        formatted_abbrevs.push(PhpMixed::from(abbrev));
                    }
                }

                if commands.len() > 1 {
                    let filtered: Vec<String> = formatted_abbrevs
                        .iter()
                        .filter(|a| shirabe_php_shim::php_truthy(a))
                        .map(shirabe_php_shim::php_to_string)
                        .collect();
                    let suggestions = self.get_abbreviation_suggestions(&filtered);

                    return Err(CommandNotFoundException::new(
                        format!(
                            "SymfonyCommand \"{}\" is ambiguous.\nDid you mean one of these?\n{}.",
                            PhpMixed::from(name.to_string()),
                            PhpMixed::from(suggestions),
                        ),
                        commands.clone(),
                        0,
                    )
                    .into());
                }
            }
        }

        // $command = $this->get(reset($commands));
        let command = self.get(&commands[0])?;

        if command.borrow().is_hidden() {
            return Err(CommandNotFoundException::new(
                format!(
                    "The command \"{}\" does not exist.",
                    PhpMixed::from(name.to_string()),
                ),
                Vec::new(),
                0,
            )
            .into());
        }

        Ok(command)
    }

    /// Gets the commands (registered in the given namespace if provided).
    ///
    /// The array keys are the full names and the values the command instances.
    pub fn all(
        &mut self,
        namespace: Option<&str>,
    ) -> anyhow::Result<IndexMap<String, std::rc::Rc<std::cell::RefCell<dyn SymfonyCommand>>>> {
        self.init()?;

        if namespace.is_none() {
            if self.command_loader.is_none() {
                return Ok(self.commands.clone());
            }

            let mut commands = self.commands.clone();
            let names = self.command_loader.as_ref().unwrap().get_names();
            for name in names {
                if !commands.contains_key(&name) && self.has(&name) {
                    commands.insert(name.clone(), self.get(&name)?);
                }
            }

            return Ok(commands);
        }

        let namespace = namespace.unwrap();
        let mut commands: IndexMap<String, std::rc::Rc<std::cell::RefCell<dyn SymfonyCommand>>> =
            IndexMap::new();
        let entries: Vec<(String, std::rc::Rc<std::cell::RefCell<dyn SymfonyCommand>>)> = self
            .commands
            .iter()
            .map(|(k, v)| (k.clone(), v.clone()))
            .collect();
        for (name, command) in entries {
            if namespace
                == self.extract_namespace(
                    &name,
                    Some(shirabe_php_shim::substr_count(namespace, ":") + 1),
                )
            {
                commands.insert(name, command);
            }
        }

        if self.command_loader.is_some() {
            let names = self.command_loader.as_ref().unwrap().get_names();
            for name in names {
                if !commands.contains_key(&name)
                    && namespace
                        == self.extract_namespace(
                            &name,
                            Some(shirabe_php_shim::substr_count(namespace, ":") + 1),
                        )
                    && self.has(&name)
                {
                    commands.insert(name.clone(), self.get(&name)?);
                }
            }
        }

        Ok(commands)
    }

    /// Returns an array of possible abbreviations given a set of names.
    pub fn get_abbreviations(names: Vec<String>) -> IndexMap<String, Vec<String>> {
        let mut abbrevs: IndexMap<String, Vec<String>> = IndexMap::new();
        for name in names {
            let mut len = shirabe_php_shim::strlen(&name);
            while len > 0 {
                let abbrev = shirabe_php_shim::substr(&name, 0, Some(len));
                abbrevs.entry(abbrev).or_default().push(name.clone());
                len -= 1;
            }
        }

        abbrevs
    }

    pub fn render_throwable(
        &self,
        e: &anyhow::Error,
        output: std::rc::Rc<std::cell::RefCell<dyn OutputInterface>>,
    ) {
        output
            .borrow()
            .writeln(&["".to_string()], output_interface::VERBOSITY_QUIET);

        self.do_render_throwable(e, output.clone());

        if let Some(running_command) = &self.running_command {
            // PHP: sprintf('<info>%s</info>', OutputFormatter::escape(sprintf($synopsis, $this->getName())))
            // A command synopsis carries no printf conversion specifier, so the inner sprintf is an
            // identity over the synopsis and the application-name argument is never substituted.
            let synopsis = running_command.borrow_mut().get_synopsis(false);
            output.borrow().writeln(
                &[format!(
                    "<info>{}</info>",
                    OutputFormatter::escape(&synopsis).unwrap(),
                )],
                output_interface::VERBOSITY_QUIET,
            );
            output
                .borrow()
                .writeln(&["".to_string()], output_interface::VERBOSITY_QUIET);
        }
    }

    pub fn do_render_throwable(
        &self,
        e: &anyhow::Error,
        output: std::rc::Rc<std::cell::RefCell<dyn OutputInterface>>,
    ) {
        // do { ... } while ($e = $e->getPrevious());
        // PHP walks getPrevious(); anyhow models the exception chain as the source chain, which
        // `anyhow::Error::chain()` yields head-first.
        for e in e.chain() {
            let message = shirabe_php_shim::trim(&e.to_string(), None);
            let verbosity = output.borrow().get_verbosity();

            let mut len;
            let mut title = String::new();
            if message.is_empty() || output_interface::VERBOSITY_VERBOSE <= verbosity {
                let class = throwable_debug_type(e);
                let code = throwable_get_code(e);
                title = format!(
                    "  [{}{}]  ",
                    class,
                    if code != 0 {
                        format!(" ({})", code)
                    } else {
                        String::new()
                    }
                );
                len = Helper::width(&title);
            } else {
                len = 0;
            }

            // PHP rewrites `@anonymous\0` markers via class_exists/get_parent_class/class_implements.
            // Rust error messages never carry PHP's anonymous-class marker and those reflection
            // primitives have no Rust equivalent, so the branch is unreachable here.
            // TODO(review): port the @anonymous rewrite if it ever becomes relevant.

            let width = if self.terminal.get_width() != 0 {
                self.terminal.get_width() - 1
            } else {
                i64::MAX
            };
            let mut lines: Vec<(String, i64)> = Vec::new();
            let split = if !message.is_empty() {
                shirabe_php_shim::preg_split(r"/\r?\n/", &message)
            } else {
                Vec::new()
            };
            for line in split {
                for line in self.split_string_by_width(&line, width - 4) {
                    // pre-format lines to get the right string length
                    let line_length = Helper::width(&line) + 4;
                    lines.push((line, line_length));
                    len = std::cmp::max(line_length, len);
                }
            }

            let mut messages: Vec<String> = Vec::new();
            if !throwable_is_exception_interface(e)
                || output_interface::VERBOSITY_VERBOSE <= verbosity
            {
                // TODO(review): anyhow::Error carries no PHP file/line, so getFile()/getLine() take
                // the 'n/a' fallback PHP itself uses when they are unavailable. The real source
                // location cannot be reproduced (it would be a Rust path, not Composer's PHP path).
                messages.push(format!(
                    "<comment>{}</comment>",
                    OutputFormatter::escape(&format!("In {} line {}:", "n/a", "n/a")).unwrap()
                ));
            }
            let empty_line = format!(
                "<error>{}</error>",
                shirabe_php_shim::str_repeat(" ", len as usize)
            );
            messages.push(empty_line.clone());
            if message.is_empty() || output_interface::VERBOSITY_VERBOSE <= verbosity {
                messages.push(format!(
                    "<error>{}{}</error>",
                    title,
                    shirabe_php_shim::str_repeat(
                        " ",
                        (len - Helper::width(&title)).max(0) as usize
                    )
                ));
            }
            for (line, line_length) in &lines {
                messages.push(format!(
                    "<error>  {}  {}</error>",
                    OutputFormatter::escape(line).unwrap(),
                    shirabe_php_shim::str_repeat(" ", (len - line_length) as usize)
                ));
            }
            messages.push(empty_line);
            messages.push(String::new());

            output
                .borrow()
                .writeln(&messages, output_interface::VERBOSITY_QUIET);

            // PHP renders the `Exception trace:` block (getTrace()) at -v or higher. A PHP backtrace
            // has no faithful Rust equivalent, so this block is intentionally never ported; verbose
            // output simply omits the trace.
        }
    }

    /// Configures the input and output instances based on the user arguments and options.
    pub fn configure_io(
        &self,
        input: &std::rc::Rc<std::cell::RefCell<dyn InputInterface>>,
        output: &std::rc::Rc<std::cell::RefCell<dyn OutputInterface>>,
    ) -> anyhow::Result<()> {
        if input.borrow().has_parameter_option(
            PhpMixed::from(vec![PhpMixed::from("--ansi".to_string())]),
            true,
        ) {
            output.borrow().set_decorated(true);
        } else if input.borrow().has_parameter_option(
            PhpMixed::from(vec![PhpMixed::from("--no-ansi".to_string())]),
            true,
        ) {
            output.borrow().set_decorated(false);
        }

        if input.borrow().has_parameter_option(
            PhpMixed::from(vec![
                PhpMixed::from("--no-interaction".to_string()),
                PhpMixed::from("-n".to_string()),
            ]),
            true,
        ) {
            input.borrow_mut().set_interactive(false);
        }

        let mut shell_verbosity = shirabe_php_shim::getenv("SHELL_VERBOSITY").unwrap_or_default();
        let shell_verbosity_int: i64 = shell_verbosity.parse().unwrap_or(0);
        let mut shell_verbosity: i64 = shell_verbosity_int;
        match shell_verbosity_int {
            -1 => {
                output
                    .borrow()
                    .set_verbosity(output_interface::VERBOSITY_QUIET);
            }
            1 => {
                output
                    .borrow()
                    .set_verbosity(output_interface::VERBOSITY_VERBOSE);
            }
            2 => {
                output
                    .borrow()
                    .set_verbosity(output_interface::VERBOSITY_VERY_VERBOSE);
            }
            3 => {
                output
                    .borrow()
                    .set_verbosity(output_interface::VERBOSITY_DEBUG);
            }
            _ => {
                shell_verbosity = 0;
            }
        }

        if input.borrow().has_parameter_option(
            PhpMixed::from(vec![
                PhpMixed::from("--quiet".to_string()),
                PhpMixed::from("-q".to_string()),
            ]),
            true,
        ) {
            output
                .borrow()
                .set_verbosity(output_interface::VERBOSITY_QUIET);
            shell_verbosity = -1;
        } else if input
            .borrow()
            .has_parameter_option(PhpMixed::from("-vvv".to_string()), true)
            || input
                .borrow()
                .has_parameter_option(PhpMixed::from("--verbose=3".to_string()), true)
            || input.borrow().get_parameter_option(
                PhpMixed::from("--verbose".to_string()),
                PhpMixed::Bool(false),
                true,
            ) == PhpMixed::from(3i64)
        {
            output
                .borrow()
                .set_verbosity(output_interface::VERBOSITY_DEBUG);
            shell_verbosity = 3;
        } else if input
            .borrow()
            .has_parameter_option(PhpMixed::from("-vv".to_string()), true)
            || input
                .borrow()
                .has_parameter_option(PhpMixed::from("--verbose=2".to_string()), true)
            || input.borrow().get_parameter_option(
                PhpMixed::from("--verbose".to_string()),
                PhpMixed::Bool(false),
                true,
            ) == PhpMixed::from(2i64)
        {
            output
                .borrow()
                .set_verbosity(output_interface::VERBOSITY_VERY_VERBOSE);
            shell_verbosity = 2;
        } else if input
            .borrow()
            .has_parameter_option(PhpMixed::from("-v".to_string()), true)
            || input
                .borrow()
                .has_parameter_option(PhpMixed::from("--verbose=1".to_string()), true)
            || input
                .borrow()
                .has_parameter_option(PhpMixed::from("--verbose".to_string()), true)
            || shirabe_php_shim::php_truthy(&input.borrow().get_parameter_option(
                PhpMixed::from("--verbose".to_string()),
                PhpMixed::Bool(false),
                true,
            ))
        {
            output
                .borrow()
                .set_verbosity(output_interface::VERBOSITY_VERBOSE);
            shell_verbosity = 1;
        }

        if shell_verbosity == -1 {
            input.borrow_mut().set_interactive(false);
        }

        if shirabe_php_shim::function_exists("putenv") {
            shirabe_php_shim::putenv(&format!("SHELL_VERBOSITY={}", shell_verbosity));
        }
        shirabe_php_shim::env_set("SHELL_VERBOSITY", shell_verbosity.to_string());
        shirabe_php_shim::server_set("SHELL_VERBOSITY", shell_verbosity.to_string());

        let _ = &mut shell_verbosity;

        Ok(())
    }

    /// Runs the current command.
    ///
    /// If an event dispatcher has been attached to the application, events are also
    /// dispatched during the life-cycle of the command.
    ///
    /// Returns 0 if everything went fine, or an error code.
    pub fn do_run_command(
        application: &std::rc::Rc<std::cell::RefCell<Application>>,
        command: std::rc::Rc<std::cell::RefCell<dyn SymfonyCommand>>,
        input: std::rc::Rc<std::cell::RefCell<dyn InputInterface>>,
        output: std::rc::Rc<std::cell::RefCell<dyn OutputInterface>>,
    ) -> anyhow::Result<i32> {
        if let Some(helper_set) = command.borrow().get_helper_set() {
            for (_alias, helper) in helper_set.borrow().get_iterator() {
                // if ($helper instanceof InputAwareInterface) $helper->setInput($input);
                // TODO(review): downcasting a HelperInterface to InputAwareInterface is not
                // expressible without a typed mechanism; needs design.
                let _ = helper;
                let _ = std::marker::PhantomData::<dyn InputAwareInterface>;
            }
        }

        if !application.borrow().signals_to_dispatch_event.is_empty() {
            // $commandSignals = $command instanceof SignalableCommandInterface ? $command->getSubscribedSignals() : []
            // TODO(review): SymfonyCommand is not a SignalableCommandInterface here; downcast needed.
            let command_signals: Vec<i64> = Vec::new();
            let _ = std::marker::PhantomData::<dyn SignalableCommandInterface>;

            if !command_signals.is_empty() {
                if application.borrow().signal_registry.is_none() {
                    return Err(ConsoleRuntimeException(shirabe_php_shim::RuntimeException {
                        message: "Unable to subscribe to signal events. Make sure that the `pcntl` extension is installed and that \"pcntl_*\" functions are not disabled by your php.ini's \"disable_functions\" directive.".to_string(),
                        code: 0,
                    })
                    .into());
                }

                if Terminal::has_stty_available() {
                    // TODO: registers SIGINT/SIGTERM handlers that restore the stty mode via
                    // shell_exec('stty ...'). pcntl signal handlers have no faithful Rust
                    // equivalent in Phase A.
                    let _stty_mode = shirabe_php_shim::shell_exec("stty -g");
                    for _signal in [shirabe_php_shim::SIGINT, shirabe_php_shim::SIGTERM] {
                        todo!("register signal handler to restore stty mode");
                    }
                }
            }

            for _signal in command_signals {
                // $this->signalRegistry->register($signal, [$command, 'handleSignal']);
                todo!("register command->handle_signal as signal handler");
            }
        }

        command
            .borrow_mut()
            .run(input.clone(), output.clone())
            .map(|c| c as i32)
    }

    /// Gets the name of the command based on input.
    pub fn get_command_name(&self, input: &dyn InputInterface) -> Option<String> {
        if self.single_command {
            Some(self.default_command.clone())
        } else {
            input.get_first_argument()
        }
    }

    /// Gets the default input definition (Symfony base; `parent::getDefaultInputDefinition`).
    pub fn base_get_default_input_definition(&self) -> InputDefinition {
        use shirabe_external_packages::symfony::console::input::input_definition::DefinitionItem;
        InputDefinition::new(vec![
            DefinitionItem::InputArgument(
                InputArgument::new(
                    "command".to_string(),
                    Some(InputArgument::REQUIRED),
                    "The command to execute".to_string(),
                    PhpMixed::Null,
                )
                .unwrap(),
            ),
            DefinitionItem::InputOption(
                InputOption::new(
                    "--help",
                    PhpMixed::from("-h".to_string()),
                    Some(InputOption::VALUE_NONE),
                    format!(
                        "Display help for the given command. When no command is given display help for the <info>{}</info> command",
                        self.default_command
                    ),
                    PhpMixed::Null,
                )
                .unwrap(),
            ),
            DefinitionItem::InputOption(
                InputOption::new(
                    "--quiet",
                    PhpMixed::from("-q".to_string()),
                    Some(InputOption::VALUE_NONE),
                    "Do not output any message".to_string(),
                    PhpMixed::Null,
                )
                .unwrap(),
            ),
            DefinitionItem::InputOption(
                InputOption::new(
                    "--verbose",
                    PhpMixed::from("-v|vv|vvv".to_string()),
                    Some(InputOption::VALUE_NONE),
                    "Increase the verbosity of messages: 1 for normal output, 2 for more verbose output and 3 for debug".to_string(),
                    PhpMixed::Null,
                )
                .unwrap(),
            ),
            DefinitionItem::InputOption(
                InputOption::new(
                    "--version",
                    PhpMixed::from("-V".to_string()),
                    Some(InputOption::VALUE_NONE),
                    "Display this application version".to_string(),
                    PhpMixed::Null,
                )
                .unwrap(),
            ),
            DefinitionItem::InputOption(
                InputOption::new(
                    "--ansi",
                    PhpMixed::from("".to_string()),
                    Some(InputOption::VALUE_NEGATABLE),
                    "Force (or disable --no-ansi) ANSI output".to_string(),
                    PhpMixed::Null,
                )
                .unwrap(),
            ),
            DefinitionItem::InputOption(
                InputOption::new(
                    "--no-interaction",
                    PhpMixed::from("-n".to_string()),
                    Some(InputOption::VALUE_NONE),
                    "Do not ask any interactive question".to_string(),
                    PhpMixed::Null,
                )
                .unwrap(),
            ),
        ])
        .unwrap()
    }

    /// Gets the default commands that should always be available (Symfony base;
    /// `parent::getDefaultCommands`).
    pub fn base_get_default_commands(
        &self,
    ) -> Vec<std::rc::Rc<std::cell::RefCell<dyn SymfonyCommand>>> {
        use shirabe_external_packages::symfony::console::command::complete_command::CompleteCommand;
        use shirabe_external_packages::symfony::console::command::dump_completion_command::DumpCompletionCommand;
        use shirabe_external_packages::symfony::console::command::help_command::HelpCommand;
        use shirabe_external_packages::symfony::console::command::list_command::ListCommand;

        vec![
            std::rc::Rc::new(std::cell::RefCell::new(HelpCommand::new()))
                as std::rc::Rc<std::cell::RefCell<dyn SymfonyCommand>>,
            std::rc::Rc::new(std::cell::RefCell::new(ListCommand::new())),
            std::rc::Rc::new(std::cell::RefCell::new(
                CompleteCommand::new(IndexMap::new())
                    .expect("CompleteCommand with default outputs is valid"),
            )),
            std::rc::Rc::new(std::cell::RefCell::new(DumpCompletionCommand::new())),
        ]
    }

    /// Gets the default helper set with the helpers that should always be available.
    pub fn get_default_helper_set(&self) -> std::rc::Rc<std::cell::RefCell<HelperSet>> {
        let helper_set = std::rc::Rc::new(std::cell::RefCell::new(HelperSet::default()));
        let helpers: IndexMap<HelperSetKey, std::rc::Rc<std::cell::RefCell<dyn HelperInterface>>> = {
            let mut m: IndexMap<
                HelperSetKey,
                std::rc::Rc<std::cell::RefCell<dyn HelperInterface>>,
            > = IndexMap::new();
            m.insert(
                HelperSetKey::Int(0),
                std::rc::Rc::new(std::cell::RefCell::new(FormatterHelper::default())),
            );
            m.insert(
                HelperSetKey::Int(1),
                std::rc::Rc::new(std::cell::RefCell::new(DebugFormatterHelper::default())),
            );
            m.insert(
                HelperSetKey::Int(2),
                std::rc::Rc::new(std::cell::RefCell::new(ProcessHelper::default())),
            );
            m.insert(
                HelperSetKey::Int(3),
                std::rc::Rc::new(std::cell::RefCell::new(QuestionHelper::default())),
            );
            m
        };
        HelperSet::new(&helper_set, helpers);
        helper_set
    }

    /// Returns abbreviated suggestions in string format.
    fn get_abbreviation_suggestions(&self, abbrevs: &[String]) -> String {
        format!("    {}", shirabe_php_shim::implode("\n    ", abbrevs))
    }

    /// Returns the namespace part of the command name.
    ///
    /// This method is not part of public API and should not be used directly.
    pub fn extract_namespace(&self, name: &str, limit: Option<i64>) -> String {
        // $parts = explode(':', $name, -1);
        let parts = shirabe_php_shim::explode_limit(":", name, -1);

        // implode(':', null === $limit ? $parts : array_slice($parts, 0, $limit))
        match limit {
            None => shirabe_php_shim::implode(":", &parts),
            Some(limit) => {
                let sliced: Vec<String> = parts.into_iter().take(limit.max(0) as usize).collect();
                shirabe_php_shim::implode(":", &sliced)
            }
        }
    }

    /// Finds alternative of $name among $collection, if nothing is found in
    /// $collection, try in $abbrevs.
    fn find_alternatives(&self, name: &str, collection: &[String]) -> Vec<String> {
        let threshold = 1e3;
        let mut alternatives: IndexMap<String, f64> = IndexMap::new();

        let mut collection_parts: IndexMap<String, Vec<String>> = IndexMap::new();
        for item in collection {
            collection_parts.insert(item.clone(), shirabe_php_shim::explode(":", item));
        }

        for (i, subname) in shirabe_php_shim::explode(":", name).into_iter().enumerate() {
            for (collection_name, parts) in &collection_parts {
                let exists = alternatives.contains_key(collection_name);
                if parts.get(i).is_none() && exists {
                    *alternatives.get_mut(collection_name).unwrap() += threshold;
                    continue;
                } else if parts.get(i).is_none() {
                    continue;
                }

                let lev = shirabe_php_shim::levenshtein(&subname, &parts[i]) as f64;
                if lev <= shirabe_php_shim::strlen(&subname) as f64 / 3.0
                    || (!subname.is_empty() && parts[i].contains(&subname))
                {
                    let v = if exists {
                        alternatives[collection_name] + lev
                    } else {
                        lev
                    };
                    alternatives.insert(collection_name.clone(), v);
                } else if exists {
                    *alternatives.get_mut(collection_name).unwrap() += threshold;
                }
            }
        }

        for item in collection {
            let lev = shirabe_php_shim::levenshtein(name, item) as f64;
            if lev <= shirabe_php_shim::strlen(name) as f64 / 3.0 || item.contains(name) {
                let v = if alternatives.contains_key(item) {
                    alternatives[item] - lev
                } else {
                    lev
                };
                alternatives.insert(item.clone(), v);
            }
        }

        // array_filter($alternatives, fn($lev) => $lev < 2 * $threshold)
        alternatives.retain(|_, lev| *lev < 2.0 * threshold);
        // ksort($alternatives, SORT_NATURAL | SORT_FLAG_CASE)
        let mut keys: Vec<String> = alternatives.keys().cloned().collect();
        shirabe_php_shim::sort_natural_flag_case(&mut keys);

        keys
    }

    /// Sets the default SymfonyCommand name.
    pub fn set_default_command(
        &mut self,
        command_name: &str,
        is_single_command: bool,
    ) -> anyhow::Result<&mut Self> {
        // $this->defaultCommand = explode('|', ltrim($commandName, '|'))[0];
        let trimmed = shirabe_php_shim::ltrim(command_name, Some("|"));
        self.default_command = shirabe_php_shim::explode("|", &trimmed)
            .into_iter()
            .next()
            .unwrap_or_default();

        if is_single_command {
            // Ensure the command exist
            self.find(command_name)?;

            self.single_command = true;
        }

        Ok(self)
    }

    pub fn is_single_command(&self) -> bool {
        self.single_command
    }

    fn split_string_by_width(&self, string: &str, width: i64) -> Vec<String> {
        // str_split is not suitable for multi-byte characters, we should use preg_split to get char array properly.
        let encoding = match shirabe_php_shim::mb_detect_encoding(string, None, true) {
            None => return shirabe_php_shim::str_split(string, width),
            Some(encoding) => encoding,
        };

        let utf8_string = shirabe_php_shim::mb_convert_encoding(string.into(), "utf8", &encoding);
        let mut lines: Vec<String> = Vec::new();
        let mut line = String::new();

        let mut offset = 0i64;
        let mut m: indexmap::IndexMap<shirabe_php_shim::CaptureKey, Option<String>> =
            indexmap::IndexMap::new();
        while shirabe_php_shim::preg_match2(
            r"/.{1,10000}/u",
            &utf8_string,
            &mut m,
            0,
            offset as usize,
        ) {
            let m0 = m[&shirabe_php_shim::CaptureKey::ByIndex(0)]
                .as_deref()
                .unwrap_or("");
            offset += shirabe_php_shim::strlen(m0);

            let chunk = m0;
            for char in chunk
                .char_indices()
                .map(|(i, c)| &chunk[i..i + c.len_utf8()])
            {
                // test if $char could be appended to current line
                if shirabe_php_shim::mb_strwidth(&format!("{}{}", line, char), Some("utf8"))
                    <= width
                {
                    line.push_str(char);
                    continue;
                }
                // if not, push current line to array and make new line
                lines.push(shirabe_php_shim::str_pad(
                    &line,
                    width as usize,
                    " ",
                    shirabe_php_shim::STR_PAD_RIGHT,
                ));
                line = char.to_string();
            }
        }

        lines.push(if !lines.is_empty() {
            shirabe_php_shim::str_pad(&line, width as usize, " ", shirabe_php_shim::STR_PAD_RIGHT)
        } else {
            line.clone()
        });

        shirabe_php_shim::mb_convert_variables(&encoding, "utf8", &mut lines);

        lines
    }

    /// Returns all namespaces of the command name.
    fn extract_all_namespaces(&self, name: &str) -> Vec<String> {
        // -1 as third argument is needed to skip the command short name when exploding
        let parts = shirabe_php_shim::explode_limit(":", name, -1);
        let mut namespaces: Vec<String> = Vec::new();

        for part in parts {
            if !namespaces.is_empty() {
                let last = namespaces.last().unwrap().clone();
                namespaces.push(format!("{}:{}", last, part));
            } else {
                namespaces.push(part);
            }
        }

        namespaces
    }

    fn init(&mut self) -> anyhow::Result<()> {
        if self.initialized {
            return Ok(());
        }
        self.initialized = true;

        for command in self.get_default_commands() {
            self.add(command)?;
        }

        Ok(())
    }
}

impl BaseApplication for Application {
    fn get_name(&self) -> String {
        Application::get_name(self)
    }

    fn get_version(&self) -> String {
        Application::get_version(self)
    }

    fn get_help(&self) -> String {
        Application::get_help(self)
    }

    fn is_single_command(&self) -> bool {
        Application::is_single_command(self)
    }

    fn extract_namespace(&self, name: &str, limit: Option<i64>) -> String {
        Application::extract_namespace(self, name, limit)
    }

    fn find_namespace(&mut self, namespace: &str) -> anyhow::Result<String> {
        Application::find_namespace(self, namespace)
    }

    fn all(
        &mut self,
        namespace: Option<&str>,
    ) -> anyhow::Result<IndexMap<String, std::rc::Rc<std::cell::RefCell<dyn SymfonyCommand>>>> {
        Application::all(self, namespace)
    }

    fn find(
        &mut self,
        name: &str,
    ) -> anyhow::Result<std::rc::Rc<std::cell::RefCell<dyn SymfonyCommand>>> {
        Application::find(self, name)
    }

    fn get_definition(&mut self) -> std::rc::Rc<std::cell::RefCell<InputDefinition>> {
        Application::get_definition(self)
    }

    fn get_helper_set(&mut self) -> std::rc::Rc<std::cell::RefCell<HelperSet>> {
        Application::get_helper_set(self)
    }

    fn complete(
        &mut self,
        input: &CompletionInput,
        suggestions: &mut CompletionSuggestions,
    ) -> anyhow::Result<()> {
        Application::complete(self, input, suggestions)
    }
}

/// Helper mirroring PHP's `$e instanceof ExceptionInterface`.
fn is_exception_interface(e: &anyhow::Error) -> bool {
    // anyhow::Error stores concrete error types; enumerate the console exceptions
    // that implement ExceptionInterface (PHP's `$e instanceof ExceptionInterface`).
    e.downcast_ref::<CommandNotFoundException>().is_some()
        || e.downcast_ref::<NamespaceNotFoundException>().is_some()
        || e.downcast_ref::<ConsoleLogicException>().is_some()
        || e.downcast_ref::<ConsoleRuntimeException>().is_some()
        || e.downcast_ref::<ConsoleInvalidArgumentException>()
            .is_some()
        || e.downcast_ref::<InvalidOptionException>().is_some()
        || e.downcast_ref::<MissingInputException>().is_some()
}

/// `is_exception_interface` for a node of the `anyhow::Error` source chain (`&dyn Error`), used
/// while walking the getPrevious() chain in `do_render_throwable`.
fn throwable_is_exception_interface(e: &(dyn std::error::Error + 'static)) -> bool {
    e.downcast_ref::<CommandNotFoundException>().is_some()
        || e.downcast_ref::<NamespaceNotFoundException>().is_some()
        || e.downcast_ref::<ConsoleLogicException>().is_some()
        || e.downcast_ref::<ConsoleRuntimeException>().is_some()
        || e.downcast_ref::<ConsoleInvalidArgumentException>()
            .is_some()
        || e.downcast_ref::<InvalidOptionException>().is_some()
        || e.downcast_ref::<MissingInputException>().is_some()
}

/// PHP's `$e->getCode()` for a node of the source chain. Enumerates the flat standard exception
/// structs that carry a `code`; everything else defaults to PHP's 0.
fn throwable_get_code(e: &(dyn std::error::Error + 'static)) -> i64 {
    if let Some(e) = e.downcast_ref::<shirabe_php_shim::Exception>() {
        return e.code;
    }
    if let Some(e) = e.downcast_ref::<shirabe_php_shim::RuntimeException>() {
        return e.code;
    }
    if let Some(e) = e.downcast_ref::<shirabe_php_shim::UnexpectedValueException>() {
        return e.code;
    }
    if let Some(e) = e.downcast_ref::<shirabe_php_shim::InvalidArgumentException>() {
        return e.code;
    }
    if let Some(e) = e.downcast_ref::<shirabe_php_shim::TypeError>() {
        return e.code;
    }
    if let Some(e) = e.downcast_ref::<shirabe_php_shim::LogicException>() {
        return e.code;
    }
    if let Some(e) = e.downcast_ref::<shirabe_php_shim::BadMethodCallException>() {
        return e.code;
    }
    if let Some(e) = e.downcast_ref::<shirabe_php_shim::OutOfBoundsException>() {
        return e.code;
    }
    if let Some(e) = e.downcast_ref::<shirabe_php_shim::ErrorException>() {
        return e.code;
    }
    if let Some(e) = e.downcast_ref::<shirabe_php_shim::PharException>() {
        return e.code;
    }
    0
}

/// PHP's `get_debug_type($e)` for the title line, reached only when the message is empty or output
/// is verbose. PHP returns the exception's fully-qualified class name; Rust has no runtime FQCN, so
/// this maps the enumerable exception types to their PHP class names and falls back to `Exception`.
/// TODO(review): the fully-qualified name (e.g. `Composer\...`) cannot be reproduced faithfully.
fn throwable_debug_type(e: &(dyn std::error::Error + 'static)) -> String {
    let name = if e
        .downcast_ref::<shirabe_php_shim::RuntimeException>()
        .is_some()
    {
        "RuntimeException"
    } else if e
        .downcast_ref::<shirabe_php_shim::UnexpectedValueException>()
        .is_some()
    {
        "UnexpectedValueException"
    } else if e
        .downcast_ref::<shirabe_php_shim::InvalidArgumentException>()
        .is_some()
    {
        "InvalidArgumentException"
    } else if e.downcast_ref::<shirabe_php_shim::TypeError>().is_some() {
        "TypeError"
    } else if e
        .downcast_ref::<shirabe_php_shim::LogicException>()
        .is_some()
    {
        "LogicException"
    } else if e
        .downcast_ref::<shirabe_php_shim::BadMethodCallException>()
        .is_some()
    {
        "BadMethodCallException"
    } else if e
        .downcast_ref::<shirabe_php_shim::OutOfBoundsException>()
        .is_some()
    {
        "OutOfBoundsException"
    } else if e
        .downcast_ref::<shirabe_php_shim::ErrorException>()
        .is_some()
    {
        "ErrorException"
    } else if e
        .downcast_ref::<shirabe_php_shim::PharException>()
        .is_some()
    {
        "PharException"
    } else {
        "Exception"
    };
    name.to_string()
}

/// Helper mirroring PHP's `$e instanceof CommandNotFoundException`.
fn downcast_command_not_found(e: &anyhow::Error) -> Option<&CommandNotFoundException> {
    if let Some(cnf) = e.downcast_ref::<CommandNotFoundException>() {
        return Some(cnf);
    }
    e.downcast_ref::<NamespaceNotFoundException>().map(|n| &n.0)
}

/// Helper mirroring PHP's `$e instanceof NamespaceNotFoundException`.
fn is_namespace_not_found(e: &anyhow::Error) -> bool {
    e.downcast_ref::<NamespaceNotFoundException>().is_some()
}

/// Borrows the shared input as a mutable `dyn InputInterface` for passing to
/// `SymfonyCommand::run`, which takes `&mut dyn InputInterface`.
fn borrow_input_mut(
    input: &std::rc::Rc<std::cell::RefCell<dyn InputInterface>>,
) -> std::cell::RefMut<'_, dyn InputInterface> {
    input.borrow_mut()
}

/// Borrows the shared output as a mutable `dyn OutputInterface` for passing to
/// `SymfonyCommand::run`, which takes `&mut dyn OutputInterface`.
fn borrow_output_mut(
    output: &std::rc::Rc<std::cell::RefCell<dyn OutputInterface>>,
) -> std::cell::RefMut<'_, dyn OutputInterface> {
    output.borrow_mut()
}
