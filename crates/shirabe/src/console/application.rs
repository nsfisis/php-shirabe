//! ref: composer/src/Composer/Console/Application.php sha1:9a852708df1cec141ed6fe16e70143ed4a6b8a3f
//! ref: composer/vendor/symfony/console/Application.php sha1:b1f6f6136fb657791ac69b1a42cdc9926c2f2566

use crate::autoload::ClassLoader;
use crate::command::{
    AboutCommand, ArchiveCommand, AuditCommand, BumpCommand, CheckPlatformReqsCommand,
    ClearCacheCommand, ConfigCommand, CreateProjectCommand, DependsCommand, DiagnoseCommand,
    DumpAutoloadCommand, ExecCommand, FundCommand, GlobalCommand, HomeCommand, InitCommand,
    InstallCommand, LicensesCommand, OutdatedCommand, ProhibitsCommand, ReinstallCommand,
    RemoveCommand, RepositoryCommand, RequireCommand, RunScriptCommand, ScriptAliasCommand,
    SearchCommand, SelfUpdateCommand, ShowCommand, StatusCommand, SuggestsCommand, UpdateCommand,
    ValidateCommand,
};
use crate::composer;
use crate::composer::{ComposerHandle, PartialComposerHandle, composer_full};
use crate::config::UseParentDir;
use crate::console::GithubActionError;
use crate::downloader::TransportException;
use crate::event_dispatcher::{EventDispatcher, ScriptExecutionException};
use crate::exception::NoSslException;
use crate::factory::{DisablePlugins, Factory};
use crate::installer::Installer;
use crate::io::{
    ConsoleIO, IOInterface, IOInterfaceImmutable, IOInterfaceWrite, NullIO, io_interface,
};
use crate::json::JsonValidationException;
use crate::plugin::console_bridge::{
    RustCommandMetadata, extend_console_application_commands,
    extend_console_application_rust_command, publish_console_application_context,
    reset_pending_plugin_command_handles, take_pending_plugin_command_handles,
};
use crate::plugin::php_call::PhpCall;
use crate::plugin::proxy::{PhpCommandProxy, new_php_object, php_class_query};
use crate::plugin::rpc::dispatch::find_file_in_registered_loaders;
use crate::plugin::rpc::table::{composer_handle_value, io_handle_value};
use crate::util::sync_executor::block_on;
use crate::util::{ErrorHandler, Filesystem, HttpDownloader, Platform, Silencer};
use bstr::{B, BStr, BString, ByteSlice, ByteVec};
use indexmap::IndexMap;
use shirabe_php_shim::{
    AnyThrowable, AsAny, Catch as _, Error, ExitException, InvalidArgumentException,
    LogicException as ShimLogicException, PHP_ENV, PHP_SERVER, PhpMixed, PregMatches,
    RuntimeException, STDIN, STR_PAD_RIGHT, basename, bconcat, bin2hex, chdir, defined, dirname,
    disk_free_space, display_to_bytes, error_message, exec, explode, explode_limit,
    extension_loaded, file_exists, file_get_contents, file_put_contents, function_exists,
    get_debug_type_obj, getcwd, getenv, getmypid, glob, implode, ini_set, intval, is_dir, is_file,
    is_numeric_string, is_string, json_decode_assoc, levenshtein, ltrim_chars, mb_convert_encoding,
    mb_convert_variables, mb_detect_encoding, mb_strwidth, microtime, os_str_from_bytes,
    path_from_bytes, path_to_bytes, php_regex, php_str, php_to_string, php_truthy, php_uname,
    posix_getuid, preg_grep, preg_match, preg_quote, preg_replace_callback, preg_split, putenv,
    random_bytes, realpath, restore_error_handler, round, sort_natural_flag_case, str_pad,
    str_repeat, str_replace, str_split, strlen, strpos, strrpos, strtoupper, substr, substr_count,
    sys_get_temp_dir, time, trim, unlink,
};
use shirabe_seld_json_lint::ParsingException;
use shirabe_symfony_console::command::{Command as SymfonyCommand, HelpCommand};
use shirabe_symfony_console::command_loader::CommandLoaderInterface;
use shirabe_symfony_console::completion::{CompletionInput, CompletionSuggestions};
use shirabe_symfony_console::exception::{
    CommandNotFoundException, InvalidArgumentException as ConsoleInvalidArgumentException,
    InvalidOptionException, LogicException as ConsoleLogicException, MissingInputException,
    NamespaceNotFoundException, RuntimeException as ConsoleRuntimeException,
};
use shirabe_symfony_console::formatter::OutputFormatter;
use shirabe_symfony_console::helper::{
    DebugFormatterHelper, FormatBlockMessages, FormatterHelper, Helper, HelperSet, ProcessHelper,
    QuestionHelper,
};
use shirabe_symfony_console::input::{
    ArgumentName, ArgvInput, ArrayInput, InputArgument, InputDefinition, InputInterface,
    InputInterfaceByName, InputOption, InputValue, ParameterName,
};
use shirabe_symfony_console::output::{
    ConsoleOutput, ConsoleOutputInterface, OutputInterface, output_interface,
};
use shirabe_symfony_console::style::{StyleInterface, SymfonyStyle};
use shirabe_symfony_console::{Application as BaseApplication, Terminal};
use shirabe_symfony_process::exception::ProcessTimedOutException;
use std::path::PathBuf;

/// The PHP `Composer\Console\Application` and `Symfony\Component\Console\Application` are
/// flattened into a single struct. Methods that are overridden by subclass and called via
/// `parent::` are prefixed by `base_`.
///
/// Symfony's signal dispatch — `$signalRegistry`, `$signalsToDispatchEvent`, and the
/// `SignalableCommandInterface` branch of `doRunCommand` — is unported. See
/// `docs/dev/signals.md`.
#[derive(Debug)]
pub struct Application {
    commands: IndexMap<BString, std::rc::Rc<std::cell::RefCell<dyn SymfonyCommand>>>,
    want_helps: bool,
    running_command: Option<std::rc::Rc<std::cell::RefCell<dyn SymfonyCommand>>>,
    name: BString,
    version: BString,
    command_loader: Option<Box<dyn CommandLoaderInterface>>,
    catch_exceptions: bool,
    definition: Option<std::rc::Rc<std::cell::RefCell<InputDefinition>>>,
    helper_set: Option<std::rc::Rc<std::cell::RefCell<HelperSet>>>,
    terminal: Terminal,
    default_command: BString,
    single_command: bool,
    // $initialized is omitted. See ApplicationHandle::init().
    composer: Option<PartialComposerHandle>,
    io: std::rc::Rc<std::cell::RefCell<dyn IOInterface>>,
    has_plugin_commands: bool,
    /// Whether this application published the worker-side console application handoff. The
    /// handoff is process-wide, so a second application in the same process must replace it
    /// rather than extend the one its predecessor left behind.
    published_worker_console_context: bool,
    disable_plugins_by_default: bool,
    disable_scripts_by_default: bool,
    /// Store the initial working directory at startup time
    initial_working_directory: Option<std::path::PathBuf>,
    /// For testing only: see [__set_dev_warning_time](Application::__set_dev_warning_time) for
    /// details.
    dev_warning_time: Option<i64>,
    /// Self-reference used to hand commands their owning application (PHP's `$command->setApplication($this)`).
    /// Set by `new_shared`.
    me: std::rc::Weak<std::cell::RefCell<Application>>,
}

impl Application {
    const LOGO: &'static BStr = php_str(
        r#"   _____ __    _            __
  / ___// /_  (_)________ _/ /_  ___
  \__ \/ __ \/ / ___/ __ `/ __ \/ _ \
 ___/ / / / / / /  / /_/ / /_/ /  __/
/____/_/ /_/_/_/   \__,_/_.___/\___/
"#
        .as_bytes(),
    );

    pub fn new(name: BString, mut version: BString) -> Self {
        if version.is_empty() {
            version = composer::SHIRABE_VERSION.into();
        }
        if function_exists("ini_set") && extension_loaded("xdebug") {
            ini_set("xdebug.show_exception_trace", "0");
            ini_set("xdebug.scream", "0");
        }

        let io: std::rc::Rc<std::cell::RefCell<dyn IOInterface>> =
            std::rc::Rc::new(std::cell::RefCell::new(NullIO::new()));

        // TODO(php-runtime): Composer registers shutdown function that reports special message for
        // OOM. In Shirabe, limit of memory allocation has effect only on PHP side so that the
        // corresponding shutdown function should be registered in PHP runtime, not here.
        // if (!$shutdownRegistered) { ... }

        let initial_working_directory = getcwd();

        Self {
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
            default_command: "list".into(),
            single_command: false,
            composer: None,
            io,
            has_plugin_commands: false,
            published_worker_console_context: false,
            disable_plugins_by_default: false,
            disable_scripts_by_default: false,
            initial_working_directory,
            dev_warning_time: composer::COMPOSER_DEV_WARNING_TIME,
            me: std::rc::Weak::new(),
        }
    }

    /// Returns the shared handle to this application set up by `ApplicationHandle::new`. Proxy
    /// commands use it to re-enter the run flow (PHP's `$this->getApplication()->run(...)`).
    pub fn shared(&self) -> ApplicationHandle {
        ApplicationHandle(
            self.me
                .upgrade()
                .expect("Application must be constructed through ApplicationHandle::new"),
        )
    }

    fn get_new_working_dir(
        &self,
        input: std::rc::Rc<std::cell::RefCell<dyn InputInterface>>,
    ) -> anyhow::Result<Option<PathBuf>> {
        let working_dir = input
            .borrow()
            .get_parameter_option(&["--working-dir", "-d"], InputValue::Null, true)
            .as_string()
            .map(|s| path_from_bytes(s).to_path_buf());
        if let Some(ref wd) = working_dir
            && !is_dir(wd)
        {
            return Err(RuntimeException::new(bconcat!(
                "Invalid working directory specified, ",
                wd,
                " does not exist."
            ))
            .into());
        }

        Ok(working_dir)
    }

    fn hint_common_errors(
        &mut self,
        exception: &anyhow::Error,
        output: std::rc::Rc<std::cell::RefCell<dyn OutputInterface>>,
    ) {
        if (exception.is_class::<ShimLogicException>() || exception.is_instanceof::<Error>())
            && output.borrow().get_verbosity() < output_interface::VERBOSITY_VERBOSE
        {
            output
                .borrow_mut()
                .set_verbosity(output_interface::VERBOSITY_VERBOSE);
        }

        Silencer::suppress(None);
        let disk_hint_msg: Option<BString> = (|| -> anyhow::Result<Option<BString>> {
            let composer = self.get_composer(false, Some(true), None)?;
            if let Some(composer) = composer {
                let composer = composer.borrow_partial();
                let config = composer.get_config();

                let min_space_free: f64 = 100.0 * 1024.0 * 1024.0;
                let mut dir = path_from_bytes(config
                    .borrow()
                    .get_home()?.as_bstr()).to_path_buf();
                let df = disk_free_space(&dir);
                let mut hit = df.map(|d| d < min_space_free).unwrap_or(false);
                if !hit {
                    dir = path_from_bytes(config
                        .borrow()
                        .get_path("vendor-dir")?.as_bstr()).to_path_buf();
                    let df = disk_free_space(&dir);
                    hit = df.map(|d| d < min_space_free).unwrap_or(false);
                }
                if !hit {
                    dir = sys_get_temp_dir();
                    let df = disk_free_space(&dir);
                    hit = df.map(|d| d < min_space_free).unwrap_or(false);
                }
                if hit {
                    return Ok(Some(bconcat!("<error>The disk hosting ", dir, " has less than 100MiB of free space, this may be the cause of the following exception</error>")));
                }
            }
            Ok(None)
        })()
        .ok()
        .flatten();
        Silencer::restore();

        let io = self.get_io();
        if let Some(msg) = &disk_hint_msg {
            io.write_error_with_verbosity(msg, io_interface::QUIET);
        }

        let message = error_message(exception);
        if exception.is_instanceof::<TransportException>()
            && message.contains_str("Unable to use a proxy")
        {
            io.write_error_with_verbosity(
                "<error>The following exception indicates your proxy is misconfigured</error>",
                io_interface::QUIET,
            );
            io.write_error_with_verbosity("<error>Check https://getcomposer.org/doc/faqs/how-to-use-composer-behind-a-proxy.md for details</error>", io_interface::QUIET);
        }

        if Platform::is_windows()
            && exception.is_instanceof::<TransportException>()
            && message.contains_str("unable to get local issuer certificate")
        {
            let avast_detect = glob("C:\\Program Files\\Avast*");
            if !avast_detect.is_empty() {
                io.write_error_with_verbosity("<error>The following exception indicates a possible issue with the Avast Firewall</error>", io_interface::QUIET);
                io.write_error_with_verbosity(
                    "<error>Check https://getcomposer.org/local-issuer for details</error>",
                    io_interface::QUIET,
                );
            } else {
                io.write_error_with_verbosity("<error>The following exception indicates a possible issue with a Firewall/Antivirus</error>", io_interface::QUIET);
                io.write_error_with_verbosity(
                    "<error>Check https://getcomposer.org/local-issuer for details</error>",
                    io_interface::QUIET,
                );
            }
        }

        if Platform::is_windows()
            && strpos(&message, "The system cannot find the path specified").is_some()
        {
            io.write_error_with_verbosity("<error>The following exception may be caused by a stale entry in your cmd.exe AutoRun</error>", io_interface::QUIET);
            io.write_error_with_verbosity("<error>Check https://getcomposer.org/doc/articles/troubleshooting.md#-the-system-cannot-find-the-path-specified-windows- for details</error>", io_interface::QUIET);
        }

        if strpos(&message, "fork failed - Cannot allocate memory").is_some() {
            io.write_error_with_verbosity("<error>The following exception is caused by a lack of memory or swap, or not having swap configured</error>", io_interface::QUIET);
            io.write_error_with_verbosity("<error>Check https://getcomposer.org/doc/articles/troubleshooting.md#proc-open-fork-failed-errors for details</error>", io_interface::QUIET);
        }

        if exception.is_instanceof::<ProcessTimedOutException>() {
            io.write_error_with_verbosity(
                "<error>The following exception is caused by a process timeout</error>",
                io_interface::QUIET,
            );
            io.write_error_with_verbosity("<error>Check https://getcomposer.org/doc/06-config.md#process-timeout for details</error>", io_interface::QUIET);
        }

        if self.get_disable_plugins_by_default()
            && self.is_running_as_root()
            && !self.io.is_interactive()
        {
            io.write_error_with_verbosity("<error>Plugins have been disabled automatically as you are running as root, this may be the cause of the following exception. See also https://getcomposer.org/root</error>", io_interface::QUIET);
        } else if exception.is_instanceof::<CommandNotFoundException>()
            && self.get_disable_plugins_by_default()
        {
            io.write_error_with_verbosity("<error>Plugins have been disabled, which may be why some commands are missing, unless you made a typo</error>", io_interface::QUIET);
        }

        let hints = HttpDownloader::get_exception_hints(exception).unwrap_or_default();
        if !hints.is_empty() {
            for hint in &hints {
                io.write_error_with_verbosity(hint, io_interface::QUIET);
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
                DisablePlugins::All
            } else {
                DisablePlugins::None
            };
            match Factory::create(io_for_factory, None, disable_plugins_enum, disable_scripts) {
                Ok(c) => self.composer = Some(c.upcast()),
                Err(e) => {
                    if e.is_instanceof::<InvalidArgumentException>() {
                        if required {
                            self.io.write_error(error_message(&e));
                            if self.are_exceptions_caught() {
                                // PHP calls `exit(1)` here, terminating before parent::run() can
                                // re-render the exception. Propagate it as an ExitException so the
                                // top-level handler turns it into exit code 1 without re-rendering.
                                return Err(ExitException { code: 1 }.into());
                            }
                            return Err(e);
                        }
                    } else if e.is_instanceof::<JsonValidationException>()
                        || e.is_instanceof::<RuntimeException>()
                        // PHP's `catch (RuntimeException)` also catches subclasses;
                        // NoSslException is the one Factory::create raises.
                        || e.is_instanceof::<NoSslException>()
                    {
                        if required {
                            return Err(e);
                        }
                    } else {
                        // Anything else (e.g. seld/jsonlint's ParsingException) propagates
                        // regardless of $required, feeding doRun's GithubActionError path.
                        return Err(e);
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

    pub fn get_help(&self) -> BString {
        bconcat!(Self::LOGO, self.base_get_help())
    }

    /// Initializes all the composer commands.
    fn get_default_commands(&self) -> Vec<std::rc::Rc<std::cell::RefCell<dyn SymfonyCommand>>> {
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
    ) -> anyhow::Result<Option<BString>> {
        let input = input.borrow().dup();
        // Makes ArgvInput::getFirstArgument() able to distinguish an option from an argument.
        match input.borrow_mut().bind(&self.get_definition().borrow()) {
            Ok(()) => {}
            Err(e) => {
                // Errors must be ignored, full binding/validation happens later when the command is known.
                if !throwable_is_exception_interface(AnyThrowable::of(&e)) {
                    return Err(e);
                }
            }
        }

        Ok(input.borrow().get_first_argument())
    }

    pub fn get_long_version(&self) -> BString {
        let mut branch_alias_string = BString::default();
        if !composer::SHIRABE_BRANCH_ALIAS_VERSION.is_empty() {
            branch_alias_string = bconcat!(" (", composer::SHIRABE_BRANCH_ALIAS_VERSION, ")");
        }

        bconcat!(
            "<info>",
            self.get_name(),
            "</info> version <comment>",
            self.get_version(),
            branch_alias_string,
            "</comment> (based on Composer ",
            composer::VERSION,
            ") ",
            composer::SHIRABE_RELEASE_DATE,
        )
    }

    fn get_default_input_definition(&self) -> anyhow::Result<InputDefinition> {
        let mut definition = self.base_get_default_input_definition();
        definition.add_option(InputOption::new(
            "--profile",
            None,
            Some(InputOption::VALUE_NONE),
            "Display timing and memory usage information",
            InputValue::Null,
        )?)?;
        definition.add_option(InputOption::new(
            "--no-plugins",
            None,
            Some(InputOption::VALUE_NONE),
            "Whether to disable plugins.",
            InputValue::Null,
        )?)?;
        definition.add_option(InputOption::new(
            "--no-scripts",
            None,
            Some(InputOption::VALUE_NONE),
            "Skips the execution of all scripts defined in composer.json file.",
            InputValue::Null,
        )?)?;
        definition.add_option(InputOption::new(
            "--working-dir",
            Some("-d".into()),
            Some(InputOption::VALUE_REQUIRED),
            "If specified, use the given directory as working directory.",
            InputValue::Null,
        )?)?;
        definition.add_option(InputOption::new(
            "--no-cache",
            None,
            Some(InputOption::VALUE_NONE),
            "Prevent use of the cache",
            InputValue::Null,
        )?)?;

        Ok(definition)
    }

    fn get_plugin_commands(
        &mut self,
    ) -> anyhow::Result<Vec<std::rc::Rc<std::cell::RefCell<dyn SymfonyCommand>>>> {
        let mut commands: Vec<std::rc::Rc<std::cell::RefCell<dyn SymfonyCommand>>> = vec![];

        reset_pending_plugin_command_handles();

        let mut composer = self.get_composer(false, Some(false), None)?;
        if composer.is_none() {
            let disable_plugins = if self.disable_plugins_by_default {
                DisablePlugins::All
            } else {
                DisablePlugins::None
            };
            composer = Factory::create_global(
                self.io.clone(),
                disable_plugins,
                self.disable_scripts_by_default,
            )?;
        }

        if let Some(composer) = composer {
            let composer = composer
                .as_full()
                .expect("Factory::create and Factory::create_global build a full Composer");
            let pm = composer.borrow().get_plugin_manager();
            let mut ctor_args: IndexMap<BString, shirabe_php_rpc::PluginValue> = IndexMap::new();
            ctor_args.insert(BString::from("composer"), composer_handle_value(&composer));
            ctor_args.insert(BString::from("io"), io_handle_value(&self.io)?);
            let capabilities = pm.borrow().get_plugin_capabilities(
                BStr::new("Composer\\Plugin\\Capability\\CommandProvider"),
                ctor_args,
            )?;
            for capability in capabilities {
                let provider = capability.as_command_provider().expect(
                    "get_plugin_capability builds a CommandProvider adapter for this capability class",
                );
                // The is_array / instanceof BaseCommand checks PHP performs on the raw
                // getCommands value live in the adapter.
                let new_commands = provider.get_commands()?;
                commands.extend(
                    new_commands.into_iter().map(|command| {
                        command as std::rc::Rc<std::cell::RefCell<dyn SymfonyCommand>>
                    }),
                );
            }

            if !commands.is_empty() {
                self.register_worker_console_commands(
                    Some(&composer),
                    take_pending_plugin_command_handles(),
                )?;
            }
        }

        Ok(commands)
    }

    /// Makes worker-hosted commands runnable through the worker-side console application: they
    /// join the handoff it boots from when one of them actually runs. This application's first
    /// call publishes that handoff, carrying the shared object graph plus metadata mirrors of
    /// the commands registered so far — all Rust-implemented at that point, since a worker-hosted
    /// command is only registered after being handed to this method. Rust-implemented commands
    /// registered later join it through `ApplicationHandle::register_worker_rust_command`.
    fn register_worker_console_commands(
        &mut self,
        composer: Option<&ComposerHandle>,
        handles: Vec<shirabe_php_rpc::PhpObjHandle>,
    ) -> anyhow::Result<()> {
        if self.published_worker_console_context {
            return extend_console_application_commands(handles);
        }
        self.published_worker_console_context = true;

        let mut seen: Vec<*const ()> = Vec::new();
        let mut rust_commands: Vec<RustCommandMetadata> = Vec::new();
        for command in self.commands.values() {
            let ptr = std::rc::Rc::as_ptr(command) as *const ();
            if seen.contains(&ptr) {
                continue;
            }
            seen.push(ptr);
            rust_commands.push(rust_command_metadata(&*command.borrow()));
        }
        publish_console_application_context(
            composer,
            &self.io,
            self.get_initial_working_directory(),
            self.disable_plugins_by_default,
            self.disable_scripts_by_default,
            rust_commands,
            handles,
        );
        register_worker_reverse_application(self.me.clone());
        Ok(())
    }

    /// Get the working directory at startup time
    pub fn get_initial_working_directory(&self) -> Option<std::path::PathBuf> {
        self.initial_working_directory.clone()
    }

    pub fn get_disable_plugins_by_default(&self) -> bool {
        self.disable_plugins_by_default
    }

    pub fn get_disable_scripts_by_default(&self) -> bool {
        self.disable_scripts_by_default
    }

    fn get_use_parent_dir_config_value(&self) -> anyhow::Result<UseParentDir> {
        let config = Factory::create_config(Some(self.io.clone()), None)?;

        config.get_use_parent_dir()
    }

    fn is_running_as_root(&self) -> bool {
        function_exists("posix_getuid") && posix_getuid() == 0
    }

    pub fn set_command_loader(&mut self, command_loader: Box<dyn CommandLoaderInterface>) {
        self.command_loader = Some(command_loader);
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
            && input
                .get_completion_name()
                .is_some_and(|name| name == "command")
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
            let filtered: Vec<shirabe_symfony_console::completion::StringOrSuggestion> =
                command_names
                    .into_iter()
                    .filter(php_truthy)
                    .map(|n| {
                        shirabe_symfony_console::completion::StringOrSuggestion::String(
                            php_to_string(&n),
                        )
                    })
                    .collect();
            suggestions.suggest_values(filtered);

            return Ok(());
        }

        if CompletionInput::TYPE_OPTION_NAME == input.get_completion_type() {
            let options: Vec<std::rc::Rc<InputOption>> = self
                .get_definition()
                .borrow()
                .get_options()
                .values()
                .cloned()
                .collect();
            suggestions.suggest_options(options);

            return Ok(());
        }

        Ok(())
    }

    /// Gets the help message (Symfony base; `parent::getHelp`).
    pub fn base_get_help(&self) -> BString {
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
    pub fn get_name(&self) -> BString {
        self.name.clone()
    }

    /// Sets the application name.
    pub fn set_name(&mut self, name: &BStr) {
        self.name = name.into();
    }

    /// Gets the application version.
    pub fn get_version(&self) -> BString {
        self.version.clone()
    }

    /// Sets the application version.
    pub fn set_version(&mut self, version: &BStr) {
        self.version = version.into();
    }

    /// Returns a registered command by name or alias.
    ///
    /// Throws CommandNotFoundException when given command name does not exist.
    pub fn get(
        &mut self,
        name: &BStr,
    ) -> anyhow::Result<std::rc::Rc<std::cell::RefCell<dyn SymfonyCommand>>> {
        if !self.has(name) {
            return Err(CommandNotFoundException::new(
                bconcat!("The command \"", name, "\" does not exist."),
                Vec::new(),
                0,
            )
            .into());
        }

        // When the command has a different name than the one used at the command loader level
        if !self.commands.contains_key(name) {
            return Err(CommandNotFoundException::new(
                bconcat!("The \"", name, "\" command cannot be found because it is registered under multiple names. Make sure you don't set a different name via constructor or \"setName()\"."),
                Vec::new(),
                0,
            )
            .into());
        }

        let command = self.commands[name].clone();

        if self.want_helps {
            self.want_helps = false;

            let help_command = self.get("help".into())?;
            (*help_command.borrow_mut())
                .as_any_mut()
                .downcast_mut::<HelpCommand>()
                .expect("the help command is a HelpCommand instance")
                .set_command(command);

            return Ok(help_command);
        }

        Ok(command)
    }

    /// Returns true if the command exists, false otherwise.
    pub fn has(&mut self, name: &BStr) -> bool {
        if self.commands.contains_key(name) {
            return true;
        }

        if let Some(command_loader) = &self.command_loader
            && command_loader.has(name)
        {
            let command = command_loader.get(name);
            return self
                .shared()
                .add(command)
                .map(|c| c.is_some())
                .unwrap_or(false);
        }

        false
    }

    /// Returns an array of all unique namespaces used by currently registered commands.
    ///
    /// It does not return the global namespace which always exists.
    pub fn get_namespaces(&mut self) -> anyhow::Result<Vec<BString>> {
        let mut namespaces: Vec<Vec<BString>> = Vec::new();
        for command in self.all(None)?.values() {
            if command.borrow().is_hidden() {
                continue;
            }

            namespaces.push(
                self.extract_all_namespaces(
                    command.borrow().get_name().unwrap_or_default().as_bstr(),
                ),
            );

            for alias in command.borrow().get_aliases() {
                namespaces.push(self.extract_all_namespaces(alias.as_bstr()));
            }
        }

        // array_values(array_unique(array_filter(array_merge([], ...$namespaces))))
        let mut merged: Vec<BString> = Vec::new();
        for ns in namespaces {
            merged.extend(ns);
        }
        let merged: Vec<BString> = merged.into_iter().filter(|s| !s.is_empty()).collect();
        let mut seen = indexmap::IndexSet::new();
        let unique: Vec<BString> = merged
            .into_iter()
            .filter(|s| seen.insert(s.clone()))
            .collect();

        Ok(unique)
    }

    /// Finds a registered namespace by a name or an abbreviation.
    ///
    /// Throws NamespaceNotFoundException when namespace is incorrect or ambiguous.
    pub fn find_namespace(&mut self, namespace: &BStr) -> anyhow::Result<BString> {
        let all_namespaces = self.get_namespaces()?;
        // implode('[^:]*:', array_map('preg_quote', explode(':', $namespace))).'[^:]*'
        let parts: Vec<BString> = explode(":", namespace)
            .into_iter()
            .map(|p| preg_quote(&p, None))
            .collect();
        let expr = bconcat!(implode("[^:]*:", &parts), "[^:]*");
        let namespaces: Vec<BString> =
            preg_grep(bconcat!("{^", expr, "}"), all_namespaces.iter().cloned()).collect();

        if namespaces.is_empty() {
            let mut message = bconcat!(
                "There are no commands defined in the \"",
                namespace,
                "\" namespace."
            );

            let alternatives = self.find_alternatives(namespace, &all_namespaces);
            if !alternatives.is_empty() {
                if alternatives.len() == 1 {
                    message.push_str("\n\nDid you mean this?\n    ");
                } else {
                    message.push_str("\n\nDid you mean one of these?\n    ");
                }

                message.push_str(implode("\n    ", &alternatives));
            }

            return Err(NamespaceNotFoundException::new(message, alternatives, 0).into());
        }

        let exact = namespaces.iter().any(|n| n.as_bstr() == namespace);
        if namespaces.len() > 1 && !exact {
            return Err(NamespaceNotFoundException::new(
                bconcat!(
                    "The namespace \"",
                    namespace,
                    "\" is ambiguous.\nDid you mean one of these?\n",
                    self.get_abbreviation_suggestions(&namespaces),
                    "."
                ),
                namespaces,
                0,
            )
            .into());
        }

        // $exact ? $namespace : reset($namespaces)
        if exact {
            Ok(namespace.into())
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
        name: &BStr,
    ) -> anyhow::Result<std::rc::Rc<std::cell::RefCell<dyn SymfonyCommand>>> {
        let mut aliases: IndexMap<BString, BString> = IndexMap::new();

        let commands_snapshot: Vec<std::rc::Rc<std::cell::RefCell<dyn SymfonyCommand>>> =
            self.commands.values().cloned().collect();
        for command in &commands_snapshot {
            let aliases = command.borrow().get_aliases();
            for alias in aliases {
                if !self.has(alias.as_bstr()) {
                    self.commands.insert(alias, command.clone());
                }
            }
        }

        if self.has(name) {
            return self.get(name);
        }

        // $allCommands = commandLoader ? array_merge(loader->getNames(), array_keys(commands)) : array_keys(commands)
        let all_commands: Vec<BString> = match &self.command_loader {
            Some(command_loader) => {
                let mut all = command_loader.get_names();
                all.extend(self.commands.keys().cloned());
                all
            }
            None => self.commands.keys().cloned().collect(),
        };

        let parts: Vec<BString> = explode(":", name)
            .into_iter()
            .map(|p| preg_quote(&p, None))
            .collect();
        let expr = bconcat!(implode("[^:]*:", &parts), "[^:]*");
        let mut commands: Vec<BString> =
            preg_grep(bconcat!("{^", expr, "}"), all_commands.iter().cloned()).collect();

        if commands.is_empty() {
            commands =
                preg_grep(bconcat!("{^", expr, "}i"), all_commands.iter().cloned()).collect();
        }

        // if no commands matched or we just matched namespaces
        if commands.is_empty()
            || preg_grep(bconcat!("{^", expr, "$}i"), commands.iter())
                .next()
                .is_none()
        {
            if let Some(pos) = strrpos(name, ":") {
                // check if a namespace exists and contains commands
                self.find_namespace(&name[..pos])?;
            }

            let mut message = bconcat!("Command \"", name, "\" is not defined.");

            let mut alternatives = self.find_alternatives(name, &all_commands);
            if !alternatives.is_empty() {
                // remove hidden commands
                let mut filtered: Vec<BString> = Vec::new();
                for alt in alternatives {
                    if !self.get(alt.as_bstr())?.borrow().is_hidden() {
                        filtered.push(alt);
                    }
                }
                alternatives = filtered;

                if alternatives.len() == 1 {
                    message.push_str("\n\nDid you mean this?\n    ");
                } else {
                    message.push_str("\n\nDid you mean one of these?\n    ");
                }
                message.push_str(implode("\n    ", &alternatives));
            }

            return Err(CommandNotFoundException::new(message, alternatives, 0).into());
        }

        // filter out aliases for commands which are already on the list
        if commands.len() > 1 {
            let mut command_list: IndexMap<
                BString,
                std::rc::Rc<std::cell::RefCell<dyn SymfonyCommand>>,
            > = self.commands.clone();

            let commands_clone = commands.clone();
            let mut new_commands: Vec<BString> = Vec::new();
            let mut seen = indexmap::IndexSet::new();
            for name_or_alias in commands {
                if !command_list.contains_key(&name_or_alias) {
                    let loaded = self
                        .command_loader
                        .as_ref()
                        .unwrap()
                        .get(name_or_alias.as_bstr());
                    command_list.insert(name_or_alias.clone(), loaded);
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
                let abbrevs: Vec<BString> = commands.clone();
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

                    let abbrev = bconcat!(
                        str_pad(&cmd, max_len as usize, " ", STR_PAD_RIGHT),
                        " ",
                        command_list[&cmd].borrow().get_description()
                    );

                    if Helper::width(&abbrev) > usable_width {
                        formatted_abbrevs.push(PhpMixed::from(bconcat!(
                            Helper::substr(&abbrev, 0, Some(usable_width - 3)),
                            "..."
                        )));
                    } else {
                        formatted_abbrevs.push(PhpMixed::from(abbrev));
                    }
                }

                if commands.len() > 1 {
                    let filtered: Vec<BString> = formatted_abbrevs
                        .iter()
                        .filter(|a| php_truthy(a))
                        .map(php_to_string)
                        .collect();
                    let suggestions = self.get_abbreviation_suggestions(&filtered);

                    return Err(CommandNotFoundException::new(
                        bconcat!(
                            "Command \"",
                            name,
                            "\" is ambiguous.\nDid you mean one of these?\n",
                            suggestions,
                            "."
                        ),
                        commands.clone(),
                        0,
                    )
                    .into());
                }
            }
        }

        // $command = $this->get(reset($commands));
        let command = self.get(commands[0].as_bstr())?;

        if command.borrow().is_hidden() {
            return Err(CommandNotFoundException::new(
                bconcat!("The command \"", name, "\" does not exist."),
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
        namespace: Option<&BStr>,
    ) -> anyhow::Result<IndexMap<BString, std::rc::Rc<std::cell::RefCell<dyn SymfonyCommand>>>>
    {
        if namespace.is_none() {
            if self.command_loader.is_none() {
                return Ok(self.commands.clone());
            }

            let mut commands = self.commands.clone();
            let names = self.command_loader.as_ref().unwrap().get_names();
            for name in names {
                if !commands.contains_key(&name) && self.has(name.as_bstr()) {
                    commands.insert(name.clone(), self.get(name.as_bstr())?);
                }
            }

            return Ok(commands);
        }

        let namespace = namespace.unwrap();
        let mut commands: IndexMap<BString, std::rc::Rc<std::cell::RefCell<dyn SymfonyCommand>>> =
            IndexMap::new();
        let entries: Vec<(BString, std::rc::Rc<std::cell::RefCell<dyn SymfonyCommand>>)> = self
            .commands
            .iter()
            .map(|(k, v)| (k.clone(), v.clone()))
            .collect();
        for (name, command) in entries {
            if namespace
                == self.extract_namespace(name.as_bstr(), Some(substr_count(namespace, ":") + 1))
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
                            name.as_bstr(),
                            Some(substr_count(namespace, ":") + 1),
                        )
                    && self.has(name.as_bstr())
                {
                    commands.insert(name.clone(), self.get(name.as_bstr())?);
                }
            }
        }

        Ok(commands)
    }

    /// Returns an array of possible abbreviations given a set of names.
    pub fn get_abbreviations(names: Vec<BString>) -> IndexMap<BString, Vec<BString>> {
        let mut abbrevs: IndexMap<BString, Vec<BString>> = IndexMap::new();
        for name in names {
            let mut len = strlen(&name);
            while len > 0 {
                let abbrev = substr(&name, 0, Some(len));
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
    ) -> anyhow::Result<()> {
        output
            .borrow()
            .writeln(&[BString::from("")], output_interface::VERBOSITY_QUIET);

        self.do_render_throwable(e, output.clone())?;

        if let Some(running_command) = &self.running_command {
            // PHP: sprintf('<info>%s</info>', OutputFormatter::escape(sprintf($synopsis, $this->getName())))
            // A command synopsis carries no printf conversion specifier, so the inner sprintf is an
            // identity over the synopsis and the application-name argument is never substituted.
            let synopsis = running_command.borrow().get_synopsis(false);
            output.borrow().writeln(
                &[bconcat!(
                    "<info>",
                    OutputFormatter::escape(&synopsis).unwrap(),
                    "</info>"
                )],
                output_interface::VERBOSITY_QUIET,
            );
            output
                .borrow()
                .writeln(&[BString::from("")], output_interface::VERBOSITY_QUIET);
        }

        Ok(())
    }

    pub fn do_render_throwable(
        &self,
        e: &anyhow::Error,
        output: std::rc::Rc<std::cell::RefCell<dyn OutputInterface>>,
    ) -> anyhow::Result<()> {
        let mut current = Some(e);
        while let Some(e) = current {
            let exception = AnyThrowable::of(e);
            let mut message = trim(error_message(e));
            let verbosity = output.borrow().get_verbosity();

            let mut len;
            let mut title = BString::default();
            if message.is_empty() || output_interface::VERBOSITY_VERBOSE <= verbosity {
                let class =
                    exception.map_or_else(|| BStr::new("Exception"), AnyThrowable::get_debug_type);
                let code = exception.map_or(0, AnyThrowable::get_code);
                title = bconcat!(
                    "  [",
                    class,
                    if code != 0 {
                        bconcat!(" (", code, ")")
                    } else {
                        BString::default()
                    },
                    "]  "
                );
                len = Helper::width(&title);
            } else {
                len = 0;
            }

            if message.contains_str(b"@anonymous\0") {
                message = preg_replace_callback(
                    php_regex!(
                        r"/[a-zA-Z_\x7f-\xff][\\a-zA-Z0-9_\x7f-\xff]*@anonymous\x00.*?\.php(?:0x?|:[0-9]+\$)?[0-9a-fA-F]+/"
                    ),
                    |m: &PregMatches| anonymous_class_display_name(m.get(0).unwrap()),
                    &message,
                )?;
            }

            let width = if self.terminal.get_width() != 0 {
                self.terminal.get_width() - 1
            } else {
                i64::MAX
            };
            let mut lines: Vec<(BString, i64)> = Vec::new();
            let split = if !message.is_empty() {
                preg_split(php_regex!(r"/\r?\n/"), &message)
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

            let mut messages: Vec<BString> = Vec::new();
            if !throwable_is_exception_interface(exception)
                || output_interface::VERBOSITY_VERBOSE <= verbosity
            {
                // An exception the port raised was created in Rust, so its file is a Rust source
                // file rather than one of Composer's. An error that is no exception has no
                // location, which PHP's 'n/a' fallback stands for.
                let file = exception
                    .map(|e| basename(e.get_file()))
                    .filter(|file| !file.is_empty())
                    .unwrap_or_else(|| BString::from("n/a"));
                let line = exception
                    .map(AnyThrowable::get_line)
                    .filter(|line| *line != 0)
                    .map_or_else(|| BString::from("n/a"), |line| bconcat!(line));
                messages.push(bconcat!(
                    "<comment>",
                    OutputFormatter::escape(bconcat!("In ", file, " line ", line, ":")).unwrap(),
                    "</comment>"
                ));
            }
            let empty_line = bconcat!("<error>", str_repeat(" ", len as usize), "</error>");
            messages.push(empty_line.clone());
            if message.is_empty() || output_interface::VERBOSITY_VERBOSE <= verbosity {
                messages.push(bconcat!(
                    "<error>",
                    title,
                    str_repeat(" ", (len - Helper::width(&title)).max(0) as usize),
                    "</error>"
                ));
            }
            for (line, line_length) in &lines {
                messages.push(bconcat!(
                    "<error>  ",
                    OutputFormatter::escape(line).unwrap(),
                    "  ",
                    str_repeat(" ", (len - line_length) as usize),
                    "</error>"
                ));
            }
            messages.push(empty_line);
            messages.push(BString::default());

            output
                .borrow()
                .writeln(&messages, output_interface::VERBOSITY_QUIET);

            // PHP renders the `Exception trace:` block (getTrace()) at -v or higher. A PHP backtrace
            // has no faithful Rust equivalent, so this block is intentionally never ported; verbose
            // output simply omits the trace.

            current = exception.and_then(AnyThrowable::get_previous);
        }

        Ok(())
    }

    /// Configures the input and output instances based on the user arguments and options.
    pub fn configure_io(
        &self,
        input: &std::rc::Rc<std::cell::RefCell<dyn InputInterface>>,
        output: &std::rc::Rc<std::cell::RefCell<dyn OutputInterface>>,
    ) -> anyhow::Result<()> {
        if input.borrow().has_parameter_option(&["--ansi"], true) {
            output.borrow().set_decorated(true);
        } else if input.borrow().has_parameter_option(&["--no-ansi"], true) {
            output.borrow().set_decorated(false);
        }

        if input
            .borrow()
            .has_parameter_option(&["--no-interaction", "-n"], true)
        {
            input.borrow_mut().set_interactive(false);
        }

        let shell_verbosity_int = intval(&PhpMixed::string(
            getenv("SHELL_VERBOSITY")
                .unwrap_or_default()
                .as_encoded_bytes(),
        ));
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

        if input
            .borrow()
            .has_parameter_option(&["--quiet", "-q"], true)
        {
            output
                .borrow()
                .set_verbosity(output_interface::VERBOSITY_QUIET);
            shell_verbosity = -1;
        } else if input.borrow().has_parameter_option(&["-vvv"], true)
            || input.borrow().has_parameter_option(&["--verbose=3"], true)
        // TODO(type-model): PHP also matches when `--verbose` carries the int 3;
        // `InputValue` has no int variant, so only the flag spellings above are checked.
        {
            output
                .borrow()
                .set_verbosity(output_interface::VERBOSITY_DEBUG);
            shell_verbosity = 3;
        } else if input.borrow().has_parameter_option(&["-vv"], true)
            || input.borrow().has_parameter_option(&["--verbose=2"], true)
        // TODO(type-model): PHP also matches when `--verbose` carries the int 2;
        // `InputValue` has no int variant, so only the flag spellings above are checked.
        {
            output
                .borrow()
                .set_verbosity(output_interface::VERBOSITY_VERY_VERBOSE);
            shell_verbosity = 2;
        } else if input.borrow().has_parameter_option(&["-v"], true)
            || input.borrow().has_parameter_option(&["--verbose=1"], true)
            || input.borrow().has_parameter_option(&["--verbose"], true)
            || input
                .borrow()
                .get_parameter_option(&["--verbose"], InputValue::Bool(false), true)
                .to_bool()
        {
            output
                .borrow()
                .set_verbosity(output_interface::VERBOSITY_VERBOSE);
            shell_verbosity = 1;
        }

        if shell_verbosity == -1 {
            input.borrow_mut().set_interactive(false);
        }

        let shell_verbosity_value = bconcat!(shell_verbosity);
        let shell_verbosity_value = os_str_from_bytes(&shell_verbosity_value);
        unsafe {
            putenv(os_str_from_bytes(&bconcat!(
                "SHELL_VERBOSITY=",
                shell_verbosity
            )))
        }
        .expect("a NAME=value assignment has a valid syntax");
        PHP_ENV.lock().unwrap().put(
            "SHELL_VERBOSITY".into(),
            shell_verbosity_value.to_os_string(),
        );
        PHP_SERVER.lock().unwrap().put(
            "SHELL_VERBOSITY".into(),
            shell_verbosity_value.to_os_string(),
        );

        let _ = &mut shell_verbosity;

        Ok(())
    }

    /// Gets the name of the command based on input.
    pub fn get_command_name(&self, input: &dyn InputInterface) -> Option<BString> {
        if self.single_command {
            Some(self.default_command.clone())
        } else {
            input.get_first_argument()
        }
    }

    /// Gets the default input definition (Symfony base; `parent::getDefaultInputDefinition`).
    pub fn base_get_default_input_definition(&self) -> InputDefinition {
        use shirabe_symfony_console::input::DefinitionItem;
        InputDefinition::new(vec![
            DefinitionItem::InputArgument(
                InputArgument::new(
                    BString::from("command"),
                    Some(InputArgument::REQUIRED),
                    "The command to execute",
                    InputValue::Null,
                )
                .unwrap(),
            ),
            DefinitionItem::InputOption(
                InputOption::new(
                    "--help",
                    Some("-h".into()),
                    Some(InputOption::VALUE_NONE),
                    bconcat!(
                        "Display help for the given command. When no command is given display help for the <info>",
                        self.default_command,
                        "</info> command"
                    ),
                    InputValue::Null,
                )
                .unwrap(),
            ),
            DefinitionItem::InputOption(
                InputOption::new(
                    "--quiet",
                    Some("-q".into()),
                    Some(InputOption::VALUE_NONE),
                    "Do not output any message",
                    InputValue::Null,
                )
                .unwrap(),
            ),
            DefinitionItem::InputOption(
                InputOption::new(
                    "--verbose",
                    Some("-v|vv|vvv".into()),
                    Some(InputOption::VALUE_NONE),
                    "Increase the verbosity of messages: 1 for normal output, 2 for more verbose output and 3 for debug",
                    InputValue::Null,
                )
                .unwrap(),
            ),
            DefinitionItem::InputOption(
                InputOption::new(
                    "--version",
                    Some("-V".into()),
                    Some(InputOption::VALUE_NONE),
                    "Display this application version",
                    InputValue::Null,
                )
                .unwrap(),
            ),
            DefinitionItem::InputOption(
                InputOption::new(
                    "--ansi",
                    Some("".into()),
                    Some(InputOption::VALUE_NEGATABLE),
                    "Force (or disable --no-ansi) ANSI output",
                    InputValue::Null,
                )
                .unwrap(),
            ),
            DefinitionItem::InputOption(
                InputOption::new(
                    "--no-interaction",
                    Some("-n".into()),
                    Some(InputOption::VALUE_NONE),
                    "Do not ask any interactive question",
                    InputValue::Null,
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
        use shirabe_symfony_console::command::{
            CompleteCommand, DumpCompletionCommand, ListCommand,
        };

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
        HelperSet::new(vec![
            (
                None,
                std::rc::Rc::new(std::cell::RefCell::new(FormatterHelper::default())),
            ),
            (
                None,
                std::rc::Rc::new(std::cell::RefCell::new(DebugFormatterHelper::default())),
            ),
            (
                None,
                std::rc::Rc::new(std::cell::RefCell::new(ProcessHelper::default())),
            ),
            (
                None,
                std::rc::Rc::new(std::cell::RefCell::new(QuestionHelper::default())),
            ),
        ])
    }

    /// Returns abbreviated suggestions in string format.
    fn get_abbreviation_suggestions(&self, abbrevs: &[BString]) -> BString {
        bconcat!("    ", implode("\n    ", abbrevs))
    }

    /// Returns the namespace part of the command name.
    ///
    /// This method is not part of public API and should not be used directly.
    pub fn extract_namespace(&self, name: &BStr, limit: Option<i64>) -> BString {
        let parts = explode_limit(":", name, -1);

        // implode(':', null === $limit ? $parts : array_slice($parts, 0, $limit))
        match limit {
            None => implode(":", &parts),
            Some(limit) => implode(":", &parts[..parts.len().min(limit.max(0) as usize)]),
        }
    }

    /// Finds alternative of $name among $collection, if nothing is found in
    /// $collection, try in $abbrevs.
    fn find_alternatives(&self, name: &BStr, collection: &[BString]) -> Vec<BString> {
        let threshold = 1e3;
        let mut alternatives: IndexMap<BString, f64> = IndexMap::new();

        let mut collection_parts: IndexMap<BString, Vec<BString>> = IndexMap::new();
        for item in collection {
            collection_parts.insert(item.clone(), explode(":", item));
        }

        for (i, subname) in explode(":", name).into_iter().enumerate() {
            for (collection_name, parts) in &collection_parts {
                let exists = alternatives.contains_key(collection_name);
                if parts.get(i).is_none() && exists {
                    *alternatives.get_mut(collection_name).unwrap() += threshold;
                    continue;
                } else if parts.get(i).is_none() {
                    continue;
                }

                let lev = levenshtein(&subname, &parts[i]) as f64;
                if lev <= strlen(&subname) as f64 / 3.0
                    || (!subname.is_empty() && parts[i].contains_str(&subname))
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
            let lev = levenshtein(name, item) as f64;
            if lev <= strlen(name) as f64 / 3.0 || item.contains_str(name) {
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
        let mut keys: Vec<BString> = alternatives.keys().cloned().collect();
        sort_natural_flag_case(&mut keys);

        keys
    }

    /// Sets the default SymfonyCommand name.
    pub fn set_default_command(
        &mut self,
        command_name: &BStr,
        is_single_command: bool,
    ) -> anyhow::Result<&mut Self> {
        self.default_command = explode("|", ltrim_chars(command_name, "|")).swap_remove(0);

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

    fn split_string_by_width(&self, string: &[u8], width: i64) -> Vec<BString> {
        // str_split is not suitable for multi-byte characters, we should use preg_split to get char array properly.
        let encoding = match mb_detect_encoding(string, None, true) {
            None => {
                return str_split(string, width);
            }
            Some(encoding) => encoding,
        };

        let utf8_string = mb_convert_encoding(string, "utf8", &encoding);
        let mut lines: Vec<BString> = Vec::new();
        let mut line = BString::default();

        let mut offset = 0usize;
        while let Some(m) = preg_match(php_regex!(r"/.{1,10000}/u"), &utf8_string[offset..]) {
            let m0 = m.get(0).unwrap_or_default();
            offset += strlen(m0) as usize;

            #[expect(
                clippy::disallowed_methods,
                reason = "the match of a /u pattern splits into UTF-8 characters"
            )]
            for (start, end, _) in m0.char_indices() {
                let char = &m0[start..end];
                // test if $char could be appended to current line
                if mb_strwidth(bconcat!(line, char), Some(bstr::BStr::new("utf8"))) <= width {
                    line.extend_from_slice(char);
                    continue;
                }
                // if not, push current line to array and make new line
                lines.push(str_pad(&line, width as usize, " ", STR_PAD_RIGHT));
                line = char.into();
            }
        }

        lines.push(if !lines.is_empty() {
            str_pad(&line, width as usize, " ", STR_PAD_RIGHT)
        } else {
            line.clone()
        });

        mb_convert_variables(&encoding, "utf8", &mut lines);

        lines
    }

    /// Returns all namespaces of the command name.
    fn extract_all_namespaces(&self, name: &BStr) -> Vec<BString> {
        // -1 as third argument is needed to skip the command short name when exploding
        let parts = explode_limit(":", name, -1);
        let mut namespaces: Vec<BString> = Vec::new();

        for part in parts {
            if !namespaces.is_empty() {
                let last = namespaces.last().unwrap().clone();
                namespaces.push(bconcat!(last, ":", part));
            } else {
                namespaces.push(part);
            }
        }

        namespaces
    }
}

/// Shared handle to an `Application`.
#[derive(Debug, Clone)]
pub struct ApplicationHandle(std::rc::Rc<std::cell::RefCell<Application>>);

impl ApplicationHandle {
    /// Builds the application inside a shared handle and registers the default commands.
    pub fn new(
        name: impl Into<BString>,
        version: impl Into<BString>,
    ) -> anyhow::Result<ApplicationHandle> {
        let name = name.into();
        let version = version.into();
        let application = ApplicationHandle(std::rc::Rc::new(std::cell::RefCell::new(
            Application::new(name, version),
        )));
        application.0.borrow_mut().me = std::rc::Rc::downgrade(&application.0);
        application.init()?;
        Ok(application)
    }

    /// Registers the default commands. It is called lazily with $this->initialized flag in PHP,
    /// but called once from new() in Rust.
    fn init(&self) -> anyhow::Result<()> {
        let commands = self.0.borrow().get_default_commands();
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
        &self,
        command: std::rc::Rc<std::cell::RefCell<dyn SymfonyCommand>>,
    ) -> anyhow::Result<Option<std::rc::Rc<std::cell::RefCell<dyn SymfonyCommand>>>> {
        let application = &self.0;
        command.borrow().set_application(Some(
            application.clone() as std::rc::Rc<std::cell::RefCell<dyn BaseApplication>>
        ));

        if !command.borrow().is_enabled() {
            command.borrow().set_application(None);

            return Ok(None);
        }

        // LazyCommand is intentionally not ported.
        command.borrow().get_definition();

        if command.borrow().get_name().is_none() {
            return Err(ConsoleLogicException::new(bconcat!(
                "The command defined in \"",
                get_debug_type_obj(&command),
                "\" cannot have an empty name."
            ))
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

    /// Mirrors a Rust-implemented command registered after the worker-side console application
    /// handoff was published into that handoff, so worker-hosted commands can `find()` it like
    /// the ones registered before. Before the handoff, publishing it mirrors every registered
    /// command anyway.
    fn register_worker_rust_command(
        &self,
        command: &std::rc::Rc<std::cell::RefCell<dyn SymfonyCommand>>,
    ) -> anyhow::Result<()> {
        if !self.0.borrow().published_worker_console_context {
            return Ok(());
        }
        let metadata = rust_command_metadata(&*command.borrow());
        extend_console_application_rust_command(metadata)
    }

    pub async fn run(
        &self,
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

        self.base_run(input, output).await
    }

    #[tracing::instrument(skip_all)]
    pub async fn do_run(
        &self,
        input: std::rc::Rc<std::cell::RefCell<dyn InputInterface>>,
        output: std::rc::Rc<std::cell::RefCell<dyn OutputInterface>>,
    ) -> anyhow::Result<i32> {
        let application = &self.0;
        application.borrow_mut().disable_plugins_by_default = input
            .borrow()
            .has_parameter_option(&["--no-plugins"], false);
        application.borrow_mut().disable_scripts_by_default = input
            .borrow()
            .has_parameter_option(&["--no-scripts"], false);

        let stdin = STDIN;
        if Platform::get_env("COMPOSER_TESTS_ARE_RUNNING").is_none_or(|v| v != "1")
            && (Platform::get_env("COMPOSER_NO_INTERACTION").is_some()
                || !Platform::is_tty(Some(stdin)))
        {
            input.borrow_mut().set_interactive(false);
        }

        application.borrow_mut().io = std::rc::Rc::new(std::cell::RefCell::new(ConsoleIO::new(
            input.clone(),
            output.clone(),
            QuestionHelper::default(),
        )));
        let io = application.borrow().io.clone();

        // Register error handler again to pass it the IO instance
        ErrorHandler::register(Some(io.clone()));

        if input.borrow().has_parameter_option(&["--no-cache"], false) {
            io.write_error_with_verbosity("Disabling cache usage", io_interface::DEBUG);
            Platform::put_env(
                "COMPOSER_CACHE_DIR",
                if Platform::is_windows() {
                    "nul"
                } else {
                    "/dev/null"
                },
            )?;
        }

        // switch working dir
        let new_work_dir = application.borrow().get_new_working_dir(input.clone())?;
        let mut old_working_dir: Option<PathBuf> = None;
        if let Some(ref nwd) = new_work_dir {
            old_working_dir = Some(Platform::get_cwd(true).unwrap_or_default());
            chdir(nwd);
            application.borrow_mut().initial_working_directory = getcwd();
            let cwd = Platform::get_cwd(true).unwrap_or_default();
            io.write_error_with_verbosity(
                bconcat!(
                    "Changed CWD to ",
                    if !cwd.as_os_str().is_empty() {
                        cwd
                    } else {
                        nwd.clone()
                    }
                ),
                io_interface::DEBUG,
            );
        }

        // determine command name to be executed without including plugin commands
        let mut command_name: Option<BString> = Some(BString::default());
        let raw_command_name = application
            .borrow_mut()
            .get_command_name_before_binding(input.clone())?;
        if let Some(ref raw) = raw_command_name {
            let find_result = application.borrow_mut().find(raw.as_ref());
            match find_result {
                Ok(cmd) => {
                    command_name = cmd.borrow().get_name();
                }
                Err(e) => {
                    if e.is_instanceof::<CommandNotFoundException>() {
                        // we'll check command validity again later after plugins are loaded
                        command_name = None;
                    }
                    // PHP also catches \InvalidArgumentException here without action
                }
            }
        }

        // prompt user for dir change if no composer.json is present in current dir
        let use_parent_dir_if_no_json_available;
        if new_work_dir.is_none()
            && !matches!(
                command_name
                    .as_ref()
                    .map_or(&b""[..], |name| name.as_slice()),
                b"" | b"list"
                    | b"init"
                    | b"about"
                    | b"help"
                    | b"diagnose"
                    | b"self-update"
                    | b"global"
                    | b"create-project"
                    | b"outdated"
            )
            && !file_exists(Factory::get_composer_file().unwrap_or_default())
            && {
                use_parent_dir_if_no_json_available =
                    application.borrow().get_use_parent_dir_config_value()?;
                use_parent_dir_if_no_json_available != UseParentDir::Bool(false)
            }
            && (command_name.as_ref().is_none_or(|name| name != "config")
                || (!input.borrow().has_parameter_option(&["--file"], true)
                    && !input.borrow().has_parameter_option(&["-f"], true)))
            && !input.borrow().has_parameter_option(&["--help"], true)
            && !input.borrow().has_parameter_option(&["-h"], true)
        {
            let mut dir = dirname(path_to_bytes(&Platform::get_cwd(true).unwrap_or_default()));
            let home_value = Platform::get_env("HOME")
                .or_else(|| Platform::get_env("USERPROFILE"))
                .unwrap_or_else(|| "/".into());
            let home = realpath(path_from_bytes(&home_value))
                .map(|p| BString::from(path_to_bytes(&p)))
                .unwrap_or_default();

            // abort when we reach the home dir or top of the filesystem
            while dirname(&dir) != dir && dir != home {
                if file_exists(path_from_bytes(&bconcat!(
                    dir,
                    "/",
                    Factory::get_composer_file().unwrap_or_default()
                ))) {
                    if use_parent_dir_if_no_json_available != UseParentDir::Bool(true)
                        && !io.is_interactive()
                    {
                        io.write_error(bconcat!(
                            "<info>No composer.json in current directory, to use the one at ",
                            dir,
                            " run interactively or set config.use-parent-dir to true</info>"
                        ));
                        break;
                    }
                    if use_parent_dir_if_no_json_available == UseParentDir::Bool(true)
                        || io.ask_confirmation(bconcat!("<info>No composer.json in current directory, do you want to use the one at ", dir, "?</info> [<comment>y,n</comment>]? "), true)?
                    {
                        if use_parent_dir_if_no_json_available == UseParentDir::Bool(true) {
                            io.write_error(bconcat!("<info>No composer.json in current directory, changing working directory to ", dir, "</info>"));
                        } else {
                            io.write_error("<info>Always want to use the parent dir? Use \"shirabe config --global use-parent-dir true\" to change the default.</info>");
                        }
                        old_working_dir = Some(Platform::get_cwd(true).unwrap_or_default());
                        chdir(path_from_bytes(&dir));
                    }
                    break;
                }
                dir = dirname(&dir);
            }
            drop((dir, home));
        }

        let needs_sudo_check = !Platform::is_windows()
            && Platform::get_env("COMPOSER_ALLOW_SUPERUSER").is_none()
            && !Platform::is_docker();
        let mut is_non_allowed_root = false;

        // Clobber sudo credentials if COMPOSER_ALLOW_SUPERUSER is not set before loading plugins
        if needs_sudo_check {
            is_non_allowed_root = application.borrow().is_running_as_root();

            if is_non_allowed_root {
                let uid: i64 = Platform::get_env("SUDO_UID")
                    .map(|v| intval(&PhpMixed::string(v)))
                    .unwrap_or(0);
                if uid != 0 {
                    // Silently clobber any sudo credentials on the invoking user to avoid privilege escalations later on
                    // ref. https://github.com/composer/composer/issues/5119
                    let _ = exec(
                        bconcat!("sudo -u \\#", uid, " sudo -K > /dev/null 2>&1"),
                        None,
                        None,
                    );
                }
            }

            // Silently clobber any remaining sudo leases on the current user as well to avoid privilege escalations
            let _ = exec("sudo -K > /dev/null 2>&1", None, None);
        }

        // avoid loading plugins/initializing the Composer instance earlier than necessary if no plugin command is needed
        // if showing the version, we never need plugin commands
        let may_need_plugin_command = !input
            .borrow()
            .has_parameter_option(&["--version", "-V"], false)
            && (command_name.is_none()
                || matches!(
                    command_name
                        .as_ref()
                        .map_or(&b""[..], |name| name.as_slice()),
                    b"" | b"list" | b"help"
                )
                || (command_name
                    .as_ref()
                    .is_some_and(|name| name == "_complete")
                    && !is_non_allowed_root));

        let may_need_script_command = may_need_plugin_command
            || command_name
                .as_ref()
                .is_some_and(|name| name == "run-script")
            || raw_command_name != command_name;

        if may_need_plugin_command
            && !application.borrow().disable_plugins_by_default
            && !application.borrow().has_plugin_commands
        {
            // at this point plugins are needed, so if we are running as root and it is not allowed we need to prompt
            // if interactive, and abort otherwise
            if is_non_allowed_root {
                io.write_error("<warning>Do not run Shirabe as root/super user! See https://getcomposer.org/root for details</warning>");

                if io.is_interactive()
                    && io.ask_confirmation(
                        "<info>Continue as root/super user</info> [<comment>yes</comment>]? ",
                        true,
                    )?
                {
                    // avoid a second prompt later
                    is_non_allowed_root = false;
                } else {
                    io.write_error("<warning>Aborting as no plugin should be loaded if running as super user is not explicitly allowed</warning>");

                    return Ok(1);
                }
            }

            match (|| -> anyhow::Result<()> {
                let plugin_commands = application.borrow_mut().get_plugin_commands()?;
                for command in plugin_commands {
                    let cmd_name = command.borrow().get_name().unwrap_or_default();
                    if application.borrow_mut().has(cmd_name.as_bstr()) {
                        let cls = command.borrow().php_class_name().to_owned();
                        io.write_error(bconcat!(
                            "<warning>Plugin command ",
                            cmd_name,
                            " (",
                            cls,
                            ") would override a Shirabe command and has been skipped</warning>"
                        ));
                    } else {
                        self.add(command)?;
                    }
                }
                Ok(())
            })() {
                Ok(_) => {}
                Err(e) => {
                    if e.is_instanceof::<NoSslException>() {
                        // suppress these as they are not relevant at this point
                    } else if let Some(pe) = e.catch::<ParsingException>() {
                        let details = pe.get_details();

                        let file = realpath(Factory::get_composer_file().unwrap_or_default());

                        let line = details.line;

                        let mut ghe = GithubActionError::new(io.clone());
                        ghe.emit(
                            pe.get_message(),
                            file.as_deref().map(|file| path_to_bytes(file).as_bstr()),
                            line,
                        );

                        return Err(e);
                    } else {
                        return Err(e);
                    }
                }
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
            let find_result = application.borrow_mut().find(name.as_ref());
            if let Ok(command) = find_result {
                command_name = command.borrow().get_name();
                // PHP: $command instanceof Command\BaseCommand && $command->isProxyCommand().
                // The Symfony `Command` trait exposes `is_proxy_command` (default false) so the
                // `dyn SymfonyCommand` registry can answer this; Composer proxy commands override it.
                is_proxy_command = command.borrow().is_proxy_command();
            }
        }

        if !is_proxy_command {
            io.write_error_with_verbosity(
                bconcat!(
                    "Running Shirabe ",
                    composer::SHIRABE_VERSION,
                    " (",
                    composer::SHIRABE_RELEASE_DATE,
                    ", based on Composer ",
                    composer::VERSION,
                    ") with PHP ",
                    shirabe_php_rpc::get_php_version()?.version,
                    " on ",
                    php_uname("s"),
                    " / ",
                    php_uname("r")
                ),
                io_interface::DEBUG,
            );

            if shirabe_php_rpc::get_php_version()?.version_id < 70205 {
                io.write_error(bconcat!("<warning>Shirabe supports PHP 7.2.5 and above, you will most likely encounter problems with your PHP ", shirabe_php_rpc::get_php_version()?.version, ". Upgrading is strongly recommended but you can use Composer 2.2.x LTS as a fallback.</warning>"));
            }

            if shirabe_php_rpc::xdebug::is_xdebug_active()?
                && Platform::get_env("COMPOSER_DISABLE_XDEBUG_WARN").is_none()
            {
                io.write_error("<warning>Shirabe is operating slower than normal because you have Xdebug enabled. See https://getcomposer.org/xdebug</warning>");
            }

            let dev_warning_time = application.borrow().dev_warning_time;
            if let Some(dev_warning_time) = dev_warning_time
                && command_name
                    .as_ref()
                    .is_none_or(|name| name != "self-update")
                && command_name
                    .as_ref()
                    .is_none_or(|name| name != "selfupdate")
                && time() > dev_warning_time
            {
                io.write_error(bconcat!("<warning>Warning: This development build of Shirabe is over 60 days old. It is recommended to update it by running \"", std::path::Path::new(
                        &PHP_SERVER
                            .lock()
                            .unwrap()
                            .get("PHP_SELF")
                            .unwrap_or_default()
                    ), " self-update\" to get the latest version.</warning>"));
            }

            if is_non_allowed_root
                && command_name
                    .as_ref()
                    .is_none_or(|name| name != "self-update")
                && command_name
                    .as_ref()
                    .is_none_or(|name| name != "selfupdate")
                && command_name.as_ref().is_none_or(|name| name != "_complete")
            {
                io.write_error("<warning>Do not run Shirabe as root/super user! See https://getcomposer.org/root for details</warning>");

                if io.is_interactive()
                    && !io.ask_confirmation(
                        "<info>Continue as root/super user</info> [<comment>yes</comment>]? ",
                        true,
                    )?
                {
                    return Ok(1);
                }
            }

            // Check system temp folder for usability as it can cause weird runtime issues otherwise
            let pid = bconcat!(getmypid(), "-");
            let tempfile = sys_get_temp_dir().join(path_from_bytes(&bconcat!(
                "temp-",
                pid,
                bin2hex(random_bytes(5))
            )));
            if !(file_put_contents(&tempfile, file!().as_bytes()).is_some_and(|n| n > 0)
                && file_get_contents(&tempfile).as_deref().ok() == Some(file!().as_bytes())
                && unlink(&tempfile).is_ok()
                && !file_exists(&tempfile))
            {
                io.write_error(bconcat!("<error>PHP temp directory (", sys_get_temp_dir(), ") does not exist or is not writable to Shirabe. Set sys_temp_dir in your php.ini</error>"));
            }

            // add non-standard scripts as own commands
            let file = Factory::get_composer_file().unwrap_or_default();
            if may_need_script_command && is_file(&file) && Filesystem::is_readable(&file) {
                let composer_json: PhpMixed =
                    json_decode_assoc(file_get_contents(&file).unwrap_or_default())
                        .unwrap_or(PhpMixed::Null);
                if let Some(arr) = composer_json.as_array()
                    && let Some(scripts) = arr.get(B("scripts")).and_then(|v| v.as_array())
                {
                    for (script, dummy) in scripts {
                        let script_event_const = bconcat!(
                            "Composer\\Script\\ScriptEvents::",
                            str_replace("-", "_", strtoupper(script))
                        );
                        if !defined(&script_event_const) {
                            if application.borrow_mut().has(script.as_ref()) {
                                io.write_error(bconcat!("<warning>A script named ", script, " would override a Shirabe command and has been skipped</warning>"));
                            } else {
                                let mut description: BString = bconcat!(
                                    "Runs the ",
                                    script,
                                    " script as defined in composer.json"
                                );

                                if let Some(desc) = arr
                                    .get(B("scripts-descriptions"))
                                    .and_then(|v| v.as_array())
                                    .and_then(|a| a.get(B(script)))
                                    .and_then(|v| v.as_string())
                                {
                                    description = desc.to_owned();
                                }

                                let aliases: Vec<BString> = arr
                                    .get(B("scripts-aliases"))
                                    .and_then(|v| v.as_array())
                                    .and_then(|a| a.get(B(script)))
                                    .and_then(|v| v.as_list())
                                    .map(|l| {
                                        l.iter()
                                            .filter_map(|v| v.as_string().map(BStr::to_owned))
                                            .collect()
                                    })
                                    .unwrap_or_default();

                                let composer_opt =
                                    application.borrow_mut().get_composer(false, None, None)?;
                                if let Some(ref composer_handle) = composer_opt {
                                    let composer = composer_full(composer_handle);
                                    let root_package = composer.get_package();
                                    let generator = composer.get_autoload_generator().clone();
                                    let generator = generator.borrow();

                                    let installation_manager = composer.get_installation_manager();
                                    let package_map = generator.build_package_map(
                                        installation_manager.clone(),
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
                                            .get_path("vendor-dir")
                                            .map(Some)?,
                                    )?;
                                    ClassLoader::register(&loader, false);
                                }

                                // if the command is not an array of commands, and points to a valid SymfonyCommand subclass, import its details directly
                                let dummy_str = dummy.as_string().unwrap_or_default();
                                // The class lives in the PHP worker, which cannot even declare
                                // a subclass of Symfony's Command before the Composer PHP
                                // runtime is loaded there. That load is skipped for a class the
                                // Rust-side ClassLoaders cannot resolve to a file, which is
                                // every plain shell-command script.
                                //
                                // TODO(php-runtime): the file lookup is narrower than PHP's
                                // `class_exists`, which is also true for a class already
                                // declared in the process. A script naming one of those (say
                                // `Composer\Command\AboutCommand`) is imported as a command by
                                // PHP but falls through to ScriptAliasCommand here.
                                let is_command_class = is_string(dummy)
                                    && find_file_in_registered_loaders(dummy_str)?.is_some()
                                    && {
                                        EventDispatcher::ensure_composer_php_runtime(|| {
                                            let cache_dir = match composer_opt {
                                                Some(ref composer_handle) => {
                                                    composer_full(composer_handle)
                                                        .get_config()
                                                        .borrow()
                                                        .get_path("cache-dir")?
                                                }
                                                None => {
                                                    Factory::create_config(Some(io.clone()), None)?
                                                        .get_path("cache-dir")?
                                                }
                                            };
                                            Ok(path_from_bytes(&cache_dir).to_path_buf())
                                        })?;
                                        php_class_query(
                                            BStr::new("class_exists"),
                                            vec![shirabe_php_rpc::PluginValue::string(dummy_str)],
                                        )? && php_class_query(
                                            BStr::new("is_subclass_of"),
                                            vec![
                                                shirabe_php_rpc::PluginValue::string(dummy_str),
                                                shirabe_php_rpc::PluginValue::string(
                                                    "Symfony\\Component\\Console\\Command\\Command",
                                                ),
                                                shirabe_php_rpc::PluginValue::Bool(true),
                                            ],
                                        )?
                                    };
                                let cmd: std::rc::Rc<std::cell::RefCell<dyn SymfonyCommand>> =
                                    if is_command_class {
                                        if php_class_query(
                                            BStr::new("is_subclass_of"),
                                            vec![
                                                shirabe_php_rpc::PluginValue::string(dummy_str),
                                                shirabe_php_rpc::PluginValue::string(
                                                    "Symfony\\Component\\Console\\SingleCommandApplication",
                                                ),
                                                shirabe_php_rpc::PluginValue::Bool(true),
                                            ],
                                        )? {
                                            io.write_error(bconcat!("<warning>The script named ", script, " extends SingleCommandApplication which is not compatible with Composer 2.9+, make sure you extend Symfony\\Component\\Console\\Command instead.</warning>"));
                                        }

                                        let handle = new_php_object(
                                            dummy_str.as_bstr(),
                                            vec![shirabe_php_rpc::PluginValue::string(
                                                script.clone(),
                                            )],
                                        )?;

                                        // makes sure the command is find()'able by the name defined in composer.json, and the name isn't overridden in its configure()
                                        let cmd_name: Option<BString> =
                                            PhpCall::Method(&handle, BStr::new("getName"))
                                                .run(Vec::new())?;
                                        if let Some(ref bytes) = cmd_name
                                            && !bytes.is_empty()
                                            && bytes != script
                                        {
                                            io.write_error(bconcat!("<warning>The script named ", script, " in composer.json has a mismatched name in its class definition. For consistency, either use the same name, or do not define one inside the class.</warning>"));
                                            // override it with the defined script name
                                            PhpCall::Method(&handle, BStr::new("setName"))
                                                .run::<shirabe_php_rpc::PluginValue>(vec![
                                                shirabe_php_rpc::PluginValue::string(
                                                    script.clone(),
                                                ),
                                            ])?;
                                        }

                                        let cmd_description: BString =
                                            PhpCall::Method(&handle, BStr::new("getDescription"))
                                                .run(Vec::new())?;
                                        if cmd_description.is_empty() {
                                            PhpCall::Method(&handle, BStr::new("setDescription"))
                                                .run::<shirabe_php_rpc::PluginValue>(vec![
                                                shirabe_php_rpc::PluginValue::string(description),
                                            ])?;
                                        }

                                        let cmd = PhpCommandProxy::new_script_command(handle)?;
                                        application.borrow_mut().register_worker_console_commands(
                                            composer_opt
                                                .as_ref()
                                                .and_then(|c| c.as_full())
                                                .as_ref(),
                                            take_pending_plugin_command_handles(),
                                        )?;
                                        std::rc::Rc::new(std::cell::RefCell::new(cmd))
                                    } else {
                                        // fallback to usual aliasing behavior
                                        std::rc::Rc::new(std::cell::RefCell::new(
                                            ScriptAliasCommand::new(
                                                script.as_ref(),
                                                Some(description),
                                                aliases,
                                            )?,
                                        ))
                                    };

                                // Compatibility layer for symfony/console <7.4
                                // (addCommand does not exist in the ported Application; add()
                                // is the only registration entry point.)
                                if let Some(cmd) = self.add(cmd)?
                                    && !is_command_class
                                {
                                    self.register_worker_rust_command(&cmd)?;
                                }
                            }
                        }
                    }
                }
            }
        }

        let mut start_time: Option<f64> = None;
        let result_outcome: anyhow::Result<i32> = async {
            if input.borrow().has_parameter_option(&["--profile"], false) {
                start_time = Some(microtime());
                io.borrow_mut().enable_debugging(start_time.unwrap());
            }

            let result = self.base_do_run(input.clone(), output.clone()).await?;

            if input
                .borrow()
                .has_parameter_option(&["--version", "-V"], true)
            {
                io.write_error(bconcat!(
                    "<info>PHP</info> version <comment>",
                    shirabe_php_rpc::get_php_version()?.version,
                    "</comment> (",
                    shirabe_php_rpc::get_php_binary()?,
                    ")"
                ));
                io.write_error(
                    "Run the \"diagnose\" command to get more detailed diagnostics output.",
                );
            }

            // chdir back to oldWorkingDir if set
            if let Some(ref owd) = old_working_dir
                && !owd.as_os_str().is_empty()
            {
                let _ = chdir(owd);
            }

            if let Some(st) = start_time {
                let mib = |memory: anyhow::Result<i64>| match memory {
                    Ok(memory) => bconcat!(
                        display_to_bytes(round((memory as f64) / 1024.0 / 1024.0, 2)),
                        "MiB"
                    ),
                    Err(_) => BString::from("<unknown>"),
                };
                io.write_error(bconcat!(
                    "<info>Memory usage: ",
                    mib(shirabe_php_rpc::memory_get_usage(false)),
                    " (peak: ",
                    mib(shirabe_php_rpc::memory_get_peak_usage(false)),
                    "), time: ",
                    display_to_bytes(round(microtime() - st, 2)),
                    "s</info>"
                ));
            }

            Ok(result)
        }
        .await;

        let outcome = match result_outcome {
            Ok(r) => Ok(r),
            Err(mut e) => {
                // PHP's `exit` bypasses parent::doRun()'s catch entirely; re-raise it untouched so
                // the GitHub Actions annotation and error hints below are skipped.
                if e.downcast_ref::<ExitException>().is_some() {
                    return Err(e);
                }
                if let Some(see) = e.catch::<ScriptExecutionException>() {
                    if application.borrow().get_disable_plugins_by_default()
                        && application.borrow().is_running_as_root()
                        && !io.is_interactive()
                    {
                        io.write_error_with_verbosity("<error>Plugins have been disabled automatically as you are running as root, this may be the cause of the script failure.</error>", io_interface::QUIET);
                        io.write_error_with_verbosity(
                            "<error>See also https://getcomposer.org/root</error>",
                            io_interface::QUIET,
                        );
                    }

                    Ok(see.get_code() as i32)
                } else {
                    let mut ghe = GithubActionError::new(io.clone());
                    ghe.emit(error_message(&e).as_bstr(), None, None);

                    application.borrow_mut().hint_common_errors(&e, output);

                    // override TransportException's code for the purpose of parent::run() using it as process exit code
                    // as http error codes are all beyond the 255 range of permitted exit codes
                    if let Some(te) = e.catch_mut::<TransportException>() {
                        te.set_code(Installer::ERROR_TRANSPORT_EXCEPTION);
                    }

                    Err(e)
                }
            }
        };

        restore_error_handler();

        outcome
    }

    pub fn set_catch_exceptions(&self, boolean: bool) {
        self.0.borrow_mut().set_catch_exceptions(boolean);
    }

    /// For testing only: simulating dynamically define COMPOSER_DEV_WARNING_TIME constant via
    /// `define()` in PHP.
    pub fn __set_dev_warning_time(&self, dev_warning_time: Option<i64>) {
        self.0.borrow_mut().dev_warning_time = dev_warning_time;
    }

    /// For testing only: exposes the application as a Symfony base-application handle, the way
    /// console commands receive it via `getApplication()`, so tests can build an
    /// `ApplicationDescription` over it.
    pub fn __base_application(&self) -> std::rc::Rc<std::cell::RefCell<dyn BaseApplication>> {
        self.0.clone()
    }

    /// For testing only: exposes the inner Application's `get_composer`, mirroring the public PHP
    /// `$application->getComposer($required, $disablePlugins, $disableScripts)` so tests can inspect
    /// the Composer instance created during do_run.
    pub fn __get_composer(
        &self,
        required: bool,
        disable_plugins: Option<bool>,
        disable_scripts: Option<bool>,
    ) -> anyhow::Result<Option<PartialComposerHandle>> {
        self.0
            .borrow_mut()
            .get_composer(required, disable_plugins, disable_scripts)
    }

    /// Runs the current application (Symfony base; `parent::run`).
    pub async fn base_run(
        &self,
        input: Option<std::rc::Rc<std::cell::RefCell<dyn InputInterface>>>,
        output: Option<std::rc::Rc<std::cell::RefCell<dyn OutputInterface>>>,
    ) -> anyhow::Result<i32> {
        let application = &self.0;
        let (height, width) = {
            let app = application.borrow();
            (app.terminal.get_height(), app.terminal.get_width())
        };
        unsafe { putenv(os_str_from_bytes(&bconcat!("LINES=", height))) }
            .expect("a NAME=value assignment has a valid syntax");
        unsafe { putenv(os_str_from_bytes(&bconcat!("COLUMNS=", width))) }
            .expect("a NAME=value assignment has a valid syntax");

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

        // TODO(php-runtime): PHP installs a temporary `set_exception_handler($renderException)` and cooperates
        // with Symfony's ErrorHandler to keep/restore it. PHP's process-global exception handler
        // stack has no Rust equivalent; the rendering itself is invoked directly in the catch
        // branch below. Review needed for the handler save/restore dance.
        let render_exception =
            |this: &Application,
             e: &anyhow::Error,
             output: &std::rc::Rc<std::cell::RefCell<dyn OutputInterface>>| {
                // if ($output instanceof ConsoleOutputInterface) render to its error output
                // ConsoleOutput is the only OutputInterface implementor that also implements
                // ConsoleOutputInterface, so `instanceof ConsoleOutputInterface` reduces to this
                // downcast to the concrete type.
                let error_output = AsAny::as_any(&*output.borrow())
                    .downcast_ref::<ConsoleOutput>()
                    .map(|console_output| console_output.get_error_output());
                match error_output {
                    Some(error_output) => this.render_throwable(e, error_output),
                    None => this.render_throwable(e, output.clone()),
                }
            };

        let result: anyhow::Result<i32> = async {
            application.borrow_mut().configure_io(&input, &output)?;

            let exit_code = self.do_run(input.clone(), output.clone()).await?;

            Ok(exit_code)
        }
        .await;

        let exit_code = match result {
            Ok(exit_code) => exit_code,
            Err(e) => {
                // PHP's `exit` bypasses Symfony's try/catch: it never renders and forces the exit
                // code it carries. The message, if any, has already been written at the exit site.
                if let Some(exit) = e.downcast_ref::<ExitException>() {
                    return Ok(exit.code as i32);
                }

                if !application.borrow().catch_exceptions {
                    return Err(e);
                }

                render_exception(&application.borrow(), &e, &output)?;

                // $exitCode = $e->getCode();
                // is_numeric($exitCode) ? max(1, (int) $exitCode) : 1
                let exit_code = AnyThrowable::of(&e).map_or(0, AnyThrowable::get_code) as i32;
                if is_numeric_string(bconcat!(exit_code)) {
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
    pub async fn base_do_run(
        &self,
        input: std::rc::Rc<std::cell::RefCell<dyn InputInterface>>,
        output: std::rc::Rc<std::cell::RefCell<dyn OutputInterface>>,
    ) -> anyhow::Result<i32> {
        let application = &self.0;
        if input
            .borrow()
            .has_parameter_option(&["--version", "-V"], true)
        {
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
                if !throwable_is_exception_interface(AnyThrowable::of(&e)) {
                    return Err(e);
                }
            }
        }

        let mut input = input;
        let mut name = application.borrow().get_command_name(&*input.borrow());
        if input.borrow().has_parameter_option(&["--help", "-h"], true) {
            if name.is_none() {
                name = Some("help".into());
                let default_command = application.borrow().default_command.clone();
                input = std::rc::Rc::new(std::cell::RefCell::new(ArrayInput::new(
                    vec![(
                        ParameterName::of("command_name"),
                        InputValue::from(default_command),
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
                    .get_argument(&ArgumentName::of("command"))?
                    .get_description()
                    .to_owned();
                let new_command_argument = InputArgument::new(
                    BString::from("command"),
                    Some(InputArgument::OPTIONAL),
                    command_description,
                    InputValue::from(name.clone()),
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
        let find_result = application.borrow_mut().find(name.as_ref());

        match find_result {
            Ok(c) => {
                command = c;
            }
            Err(e) => {
                // if (!($e instanceof CommandNotFoundException && !$e instanceof NamespaceNotFoundException)
                //     || 1 !== count($alternatives = $e->getAlternatives()) || !$input->isInteractive())
                let alternatives: Option<Vec<BString>> = e
                    .catch::<CommandNotFoundException>()
                    .filter(|_| !e.is_instanceof::<NamespaceNotFoundException>())
                    .map(|cnf| cnf.get_alternatives().clone());

                let single_alternative = match &alternatives {
                    Some(alts) if alts.len() == 1 => Some(alts[0].clone()),
                    _ => None,
                };

                if single_alternative.is_none() || !input.borrow().is_interactive() {
                    return Err(e);
                }

                let alternative = single_alternative.unwrap();

                let style = SymfonyStyle::new(input.clone(), output.clone());
                output
                    .borrow()
                    .writeln(&[BString::from("")], output_interface::OUTPUT_NORMAL);
                let formatted_block = FormatterHelper::default().format_block(
                    FormatBlockMessages::String(bconcat!("Command \"", name, "\" is not defined.")),
                    "error".into(),
                    true,
                );
                output
                    .borrow()
                    .writeln(&[formatted_block], output_interface::OUTPUT_NORMAL);
                if !style.borrow().confirm(
                    bconcat!("Do you want to run \"", alternative, "\" instead? ").as_ref(),
                    false,
                ) {
                    return Ok(1);
                }

                command = application.borrow_mut().find(alternative.as_bstr())?;
            }
        }

        // LazyCommand is intentionally not ported.

        application.borrow_mut().running_command = Some(command.clone());
        let exit_code = self
            .do_run_command(command.clone(), input.clone(), output.clone())
            .await?;
        application.borrow_mut().running_command = None;

        Ok(exit_code)
    }

    /// Adds an array of command objects.
    ///
    /// If a SymfonyCommand is not enabled it will not be added.
    pub fn add_commands(
        &self,
        commands: Vec<std::rc::Rc<std::cell::RefCell<dyn SymfonyCommand>>>,
    ) -> anyhow::Result<()> {
        for command in commands {
            self.add(command)?;
        }
        Ok(())
    }

    /// Runs the current command.
    ///
    /// If an event dispatcher has been attached to the application, events are also
    /// dispatched during the life-cycle of the command.
    ///
    /// Returns 0 if everything went fine, or an error code.
    pub async fn do_run_command(
        &self,
        command: std::rc::Rc<std::cell::RefCell<dyn SymfonyCommand>>,
        input: std::rc::Rc<std::cell::RefCell<dyn InputInterface>>,
        output: std::rc::Rc<std::cell::RefCell<dyn OutputInterface>>,
    ) -> anyhow::Result<i32> {
        let application = &self.0;
        // PHP: foreach ($command->getHelperSet() as $helper) { if ($helper instanceof
        // InputAwareInterface) $helper->setInput($input); }
        // TODO(port): HelperInterface exposes no InputAwareInterface view to test `instanceof`
        // against, and InputAwareInterface::set_input takes an owned input rather than the shared
        // handle, so the loop is not ported. None of the helpers in the default helper set
        // implement InputAwareInterface.
        let _ = command.borrow().get_helper_set();

        command
            .borrow()
            .run(input.clone(), output.clone())
            .await
            .map(|c| c as i32)
    }
}

impl BaseApplication for Application {
    fn get_name(&self) -> BString {
        Application::get_name(self)
    }

    fn get_version(&self) -> BString {
        Application::get_version(self)
    }

    fn get_help(&self) -> BString {
        Application::get_help(self)
    }

    fn is_single_command(&self) -> bool {
        Application::is_single_command(self)
    }

    fn extract_namespace(&self, name: &BStr, limit: Option<i64>) -> BString {
        Application::extract_namespace(self, name, limit)
    }

    fn find_namespace(&mut self, namespace: &BStr) -> anyhow::Result<BString> {
        Application::find_namespace(self, namespace)
    }

    fn all(
        &mut self,
        namespace: Option<&BStr>,
    ) -> anyhow::Result<IndexMap<BString, std::rc::Rc<std::cell::RefCell<dyn SymfonyCommand>>>>
    {
        Application::all(self, namespace)
    }

    fn find(
        &mut self,
        name: &BStr,
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

/// PHP's `$e instanceof ExceptionInterface`, enumerating the console exceptions that implement it.
// TODO(plugin): a plugin can throw an exception class of its own that implements
// ExceptionInterface, and no enumeration on this side can name it. Answering that needs the
// interfaces a thrown exception implements, not just its superclasses.
fn throwable_is_exception_interface(e: Option<&AnyThrowable>) -> bool {
    e.is_some_and(|e| {
        e.is_instanceof::<CommandNotFoundException>()
            || e.is_instanceof::<NamespaceNotFoundException>()
            || e.is_instanceof::<ConsoleLogicException>()
            || e.is_instanceof::<ConsoleRuntimeException>()
            || e.is_instanceof::<ConsoleInvalidArgumentException>()
            || e.is_instanceof::<InvalidOptionException>()
            || e.is_instanceof::<MissingInputException>()
    })
}

/// The replacement `doRenderThrowable` writes for an anonymous class name found in a message:
/// `(get_parent_class($class) ?: key(class_implements($class)) ?: 'class').'@anonymous'` when
/// `class_exists($class, false)`, else the name unchanged. Anonymous classes are declared only by
/// PHP code the worker ran, so the worker's class table answers, and without a worker no such
/// class exists.
fn anonymous_class_display_name(class: &BStr) -> anyhow::Result<BString> {
    let string = || shirabe_php_rpc::PluginValue::string(class);
    let exists = shirabe_php_rpc::worker_is_running()
        && PhpCall::Function(BStr::new("class_exists"))
            .run::<bool>(vec![string(), shirabe_php_rpc::PluginValue::Bool(false)])?;
    if !exists {
        return Ok(class.to_owned());
    }

    let get_parent_class = PhpCall::Function(BStr::new("get_parent_class"));
    let parent = match get_parent_class.run::<shirabe_php_rpc::PluginValue>(vec![string()])? {
        shirabe_php_rpc::PluginValue::Bool(false) => None,
        value => Some(
            <BString as shirabe_php_rpc::FromWire>::from_wire(&value)
                .map_err(|throw| get_parent_class.unsupported_shape(throw))?,
        ),
    };
    let name = match parent {
        Some(parent) => parent,
        None => PhpCall::Function(BStr::new("class_implements"))
            .run::<IndexMap<BString, shirabe_php_rpc::PluginValue>>(vec![string()])?
            .into_keys()
            .next()
            .unwrap_or_else(|| BString::from("class")),
    };
    Ok(bconcat!(name, "@anonymous"))
}

thread_local! {
    /// The application answering `__shirabe_run_rust_command` callbacks from the plugin
    /// worker's reverse command stubs; registered when plugin commands are collected.
    static WORKER_REVERSE_APPLICATION: std::cell::RefCell<
        Option<std::rc::Weak<std::cell::RefCell<Application>>>,
    > = const { std::cell::RefCell::new(None) };
}

/// The metadata a worker-side `\Shirabe\RustCommandStub` mirrors of a registered command.
fn rust_command_metadata(command: &dyn SymfonyCommand) -> RustCommandMetadata {
    RustCommandMetadata {
        name: command
            .get_name()
            .expect("a registered command always has a name"),
        description: command.get_description(),
        aliases: command.get_aliases(),
        hidden: command.is_hidden(),
    }
}

/// Registers the application the worker's reverse command stubs call back into.
fn register_worker_reverse_application(
    application: std::rc::Weak<std::cell::RefCell<Application>>,
) {
    WORKER_REVERSE_APPLICATION.with(|slot| *slot.borrow_mut() = Some(application));
}

/// Runs a built-in command on behalf of a plugin-provided command executing in the worker
/// (the reverse half of the two-world command split): the stringified input the plugin passed
/// is re-parsed here and the command runs against this side's application state — helper set
/// included — writing to the same stdio the worker inherited.
pub(crate) fn run_worker_reverse_command(name: &BStr, input_line: &BStr) -> anyhow::Result<i64> {
    let application = WORKER_REVERSE_APPLICATION
        .with(|slot| slot.borrow().as_ref().and_then(std::rc::Weak::upgrade))
        .ok_or_else(|| -> anyhow::Error {
            RuntimeException::new(bconcat!(
                "cannot run command ",
                name,
                ": no application is registered for worker callbacks"
            ))
            .into()
        })?;
    let command = application.borrow_mut().find(name)?;
    // PHP's `(string) $input` omits the command name when the input was built without an
    // explicit `command` entry, but the merged definition binds the first positional token to
    // the `command` argument, so the name is prepended when the first token is not this
    // command.
    let trimmed = trim(input_line);
    let first_token = trimmed
        .split(u8::is_ascii_whitespace)
        .find(|token| !token.is_empty())
        .unwrap_or_default();
    let is_named = {
        let command = command.borrow();
        command
            .get_name()
            .is_some_and(|name| name.as_slice() == first_token)
            || command
                .get_aliases()
                .iter()
                .any(|alias| alias.as_slice() == first_token)
    };
    let line = if is_named {
        trimmed
    } else if trimmed.is_empty() {
        BString::from(name)
    } else {
        bconcat!(name, " ", trimmed)
    };
    let input: std::rc::Rc<std::cell::RefCell<dyn InputInterface>> = std::rc::Rc::new(
        std::cell::RefCell::new(shirabe_symfony_console::input::StringInput::new(&line)?),
    );
    let output: std::rc::Rc<std::cell::RefCell<dyn OutputInterface>> = std::rc::Rc::new(
        std::cell::RefCell::new(ConsoleOutput::new(None, None, None)?),
    );
    // The worker's callback is a synchronous RPC request, answered on the thread serving it.
    block_on(command.borrow().run(input, output))
}
