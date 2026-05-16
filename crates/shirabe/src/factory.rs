//! ref: composer/src/Composer/Factory.php

use indexmap::IndexMap;

use shirabe_external_packages::symfony::component::console::formatter::output_formatter::OutputFormatter;
use shirabe_external_packages::symfony::component::console::formatter::output_formatter_style::OutputFormatterStyle;
use shirabe_external_packages::symfony::component::console::output::console_output::ConsoleOutput;
use shirabe_php_shim::{
    array_keys, array_replace_recursive, class_exists, dirname, extension_loaded, file_exists,
    file_get_contents, file_put_contents, implode, in_array, is_array, is_dir, is_file, is_string,
    json_decode, pathinfo, realpath, str_replace, strpos, strtr, substr, trim, InvalidArgumentException,
    Phar, PhpMixed, RuntimeException, UnexpectedValueException, ZipArchive, PATHINFO_EXTENSION,
    PHP_EOL,
};

use crate::autoload::autoload_generator::AutoloadGenerator;
use crate::composer::Composer;
use crate::config::Config;
use crate::config::json_config_source::JsonConfigSource;
use crate::downloader::download_manager::DownloadManager;
use crate::downloader::file_downloader::FileDownloader;
use crate::downloader::fossil_downloader::FossilDownloader;
use crate::downloader::git_downloader::GitDownloader;
use crate::downloader::gzip_downloader::GzipDownloader;
use crate::downloader::hg_downloader::HgDownloader;
use crate::downloader::path_downloader::PathDownloader;
use crate::downloader::perforce_downloader::PerforceDownloader;
use crate::downloader::phar_downloader::PharDownloader;
use crate::downloader::rar_downloader::RarDownloader;
use crate::downloader::svn_downloader::SvnDownloader;
use crate::downloader::tar_downloader::TarDownloader;
use crate::downloader::transport_exception::TransportException;
use crate::downloader::xz_downloader::XzDownloader;
use crate::downloader::zip_downloader::ZipDownloader;
use crate::event_dispatcher::event::Event;
use crate::event_dispatcher::event_dispatcher::EventDispatcher;
use crate::exception::no_ssl_exception::NoSslException;
use crate::installer::binary_installer::BinaryInstaller;
use crate::installer::installation_manager::InstallationManager;
use crate::installer::library_installer::LibraryInstaller;
use crate::installer::metapackage_installer::MetapackageInstaller;
use crate::installer::plugin_installer::PluginInstaller;
use crate::io::io_interface::IOInterface;
use crate::json::json_file::JsonFile;
use crate::json::json_validation_exception::JsonValidationException;
use crate::package::archiver::archive_manager::ArchiveManager;
use crate::package::archiver::phar_archiver::PharArchiver;
use crate::package::archiver::zip_archiver::ZipArchiver;
use crate::package::locker::Locker;
use crate::package::loader::root_package_loader::RootPackageLoader;
use crate::package::root_package_interface::RootPackageInterface;
use crate::package::version::version_guesser::VersionGuesser;
use crate::package::version::version_parser::VersionParser;
use crate::partial_composer::PartialComposer;
use crate::plugin::plugin_events::PluginEvents;
use crate::plugin::plugin_manager::PluginManager;
use crate::repository::filesystem_repository::FilesystemRepository;
use crate::repository::installed_filesystem_repository::InstalledFilesystemRepository;
use crate::repository::installed_repository_interface::InstalledRepositoryInterface;
use crate::repository::repository_factory::RepositoryFactory;
use crate::repository::repository_manager::RepositoryManager;
use crate::util::cache::Cache;
use crate::util::filesystem::Filesystem;
use crate::util::http_downloader::HttpDownloader;
use crate::util::r#loop::Loop;
use crate::util::platform::Platform;
use crate::util::process_executor::ProcessExecutor;
use crate::util::silencer::Silencer;

/// Either a configuration array or a filename to read from. PHP's `$localConfig` accepts both.
pub enum LocalConfigInput {
    Path(String),
    Data(IndexMap<String, PhpMixed>),
}

/// PHP's `$disablePlugins` accepts `bool|'local'|'global'`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DisablePlugins {
    None,
    All,
    Local,
    Global,
}

impl DisablePlugins {
    fn is_disabled_at_all(self) -> bool {
        !matches!(self, DisablePlugins::None)
    }
}

/// Creates a configured instance of composer.
pub struct Factory;

impl Factory {
    fn get_home_dir() -> anyhow::Result<String> {
        let home = Platform::get_env("COMPOSER_HOME");
        if let Some(h) = home {
            if !h.is_empty() {
                return Ok(h);
            }
        }

        if Platform::is_windows() {
            if Platform::get_env("APPDATA").map(|s| s.is_empty()).unwrap_or(true) {
                return Err(anyhow::anyhow!(RuntimeException {
                    message:
                        "The APPDATA or COMPOSER_HOME environment variable must be set for composer to run correctly"
                            .to_string(),
                    code: 0,
                }));
            }

            let appdata = Platform::get_env("APPDATA").unwrap_or_default();
            return Ok(format!(
                "{}/Composer",
                trim(&strtr(&appdata, "\\", "/"), "/")
            ));
        }

        let user_dir = Self::get_user_dir()?;
        let mut dirs: Vec<String> = Vec::new();

        if Self::use_xdg() {
            // XDG Base Directory Specifications
            let mut xdg_config = Platform::get_env("XDG_CONFIG_HOME").unwrap_or_default();
            if xdg_config.is_empty() {
                xdg_config = format!("{}/.config", user_dir);
            }

            dirs.push(format!("{}/composer", xdg_config));
        }

        dirs.push(format!("{}/.composer", user_dir));

        // select first dir which exists of: $XDG_CONFIG_HOME/composer or ~/.composer
        for dir in &dirs {
            let dir_copy = dir.clone();
            let exists = Silencer::call(|| Ok::<bool, anyhow::Error>(is_dir(&dir_copy)))
                .unwrap_or(false);
            if exists {
                return Ok(dir.clone());
            }
        }

        // if none exists, we default to first defined one (XDG one if system uses it, or ~/.composer otherwise)
        Ok(dirs[0].clone())
    }

    fn get_cache_dir(home: &str) -> anyhow::Result<String> {
        let cache_dir = Platform::get_env("COMPOSER_CACHE_DIR").unwrap_or_default();
        if !cache_dir.is_empty() {
            return Ok(cache_dir);
        }

        let home_env = Platform::get_env("COMPOSER_HOME").unwrap_or_default();
        if !home_env.is_empty() {
            return Ok(format!("{}/cache", home_env));
        }

        if Platform::is_windows() {
            let mut cache_dir = Platform::get_env("LOCALAPPDATA").unwrap_or_default();
            if !cache_dir.is_empty() {
                cache_dir = format!("{}/Composer", cache_dir);
            } else {
                cache_dir = format!("{}/cache", home);
            }

            return Ok(trim(&strtr(&cache_dir, "\\", "/"), "/"));
        }

        let user_dir = Self::get_user_dir()?;
        if Platform::php_os() == "Darwin" {
            // Migrate existing cache dir in old location if present
            if is_dir(&format!("{}/cache", home))
                && !is_dir(&format!("{}/Library/Caches/composer", user_dir))
            {
                let from = format!("{}/cache", home);
                let to = format!("{}/Library/Caches/composer", user_dir);
                let _ = Silencer::call(|| {
                    Ok::<bool, anyhow::Error>(Platform::rename(&from, &to))
                });
            }

            return Ok(format!("{}/Library/Caches/composer", user_dir));
        }

        if home == format!("{}/.composer", user_dir).as_str()
            && is_dir(&format!("{}/cache", home))
        {
            return Ok(format!("{}/cache", home));
        }

        if Self::use_xdg() {
            let xdg_cache = Platform::get_env("XDG_CACHE_HOME").unwrap_or_default();
            let xdg_cache = if xdg_cache.is_empty() {
                format!("{}/.cache", user_dir)
            } else {
                xdg_cache
            };

            return Ok(format!("{}/composer", xdg_cache));
        }

        Ok(format!("{}/cache", home))
    }

    fn get_data_dir(home: &str) -> anyhow::Result<String> {
        let home_env = Platform::get_env("COMPOSER_HOME").unwrap_or_default();
        if !home_env.is_empty() {
            return Ok(home_env);
        }

        if Platform::is_windows() {
            return Ok(strtr(home, "\\", "/"));
        }

        let user_dir = Self::get_user_dir()?;
        if home != format!("{}/.composer", user_dir) && Self::use_xdg() {
            let xdg_data = Platform::get_env("XDG_DATA_HOME").unwrap_or_default();
            let xdg_data = if xdg_data.is_empty() {
                format!("{}/.local/share", user_dir)
            } else {
                xdg_data
            };

            return Ok(format!("{}/composer", xdg_data));
        }

        Ok(home.to_string())
    }

    pub fn create_config(io: Option<&dyn IOInterface>, cwd: Option<&str>) -> anyhow::Result<Config> {
        let cwd = cwd.map(|s| s.to_string()).unwrap_or_else(|| Platform::get_cwd(true));

        let mut config = Config::new(true, cwd);

        // determine and add main dirs to the config
        let home = Self::get_home_dir()?;
        let mut defaults: IndexMap<String, PhpMixed> = IndexMap::new();
        let mut inner: IndexMap<String, PhpMixed> = IndexMap::new();
        inner.insert("home".to_string(), PhpMixed::String(home.clone()));
        inner.insert(
            "cache-dir".to_string(),
            PhpMixed::String(Self::get_cache_dir(&home)?),
        );
        inner.insert(
            "data-dir".to_string(),
            PhpMixed::String(Self::get_data_dir(&home)?),
        );
        defaults.insert(
            "config".to_string(),
            PhpMixed::Array(inner.into_iter().map(|(k, v)| (k, Box::new(v))).collect()),
        );
        config.merge(defaults, Config::SOURCE_DEFAULT);

        // load global config
        let file = JsonFile::new(format!("{}/config.json", config.get_str("home")?), None, io);
        if file.exists() {
            if let Some(io_ref) = io {
                io_ref.write_error(
                    PhpMixed::String(format!("Loading config file {}", file.get_path())),
                    true,
                    <dyn IOInterface>::DEBUG,
                );
            }
            Self::validate_json_schema(io, ValidateJsonInput::File(file.clone()), JsonFile::LAX_SCHEMA, None)?;
            config.merge(file.read()?, file.get_path().to_string());
        }
        config.set_config_source(JsonConfigSource::new(file.clone(), false));

        let htaccess_protect = config
            .get("htaccess-protect")
            .and_then(|v| v.as_bool())
            .unwrap_or(false);
        if htaccess_protect {
            // Protect directory against web access. Since HOME could be
            // the www-data's user home and be web-accessible it is a
            // potential security risk
            let dirs = [
                config.get_str("home")?,
                config.get_str("cache-dir")?,
                config.get_str("data-dir")?,
            ];
            for dir in &dirs {
                if !file_exists(&format!("{}/.htaccess", dir)) {
                    if !is_dir(dir) {
                        let dir_owned = dir.clone();
                        let _ = Silencer::call(|| {
                            Ok::<bool, anyhow::Error>(Platform::mkdir(&dir_owned, 0o777, true))
                        });
                    }
                    let path = format!("{}/.htaccess", dir);
                    let _ = Silencer::call(|| {
                        Ok::<Option<i64>, anyhow::Error>(file_put_contents(&path, b"Deny from all"))
                    });
                }
            }
        }

        // load global auth file
        let auth_file = JsonFile::new(
            format!("{}/auth.json", config.get_str("home")?),
            None,
            io,
        );
        if auth_file.exists() {
            if let Some(io_ref) = io {
                io_ref.write_error(
                    PhpMixed::String(format!("Loading config file {}", auth_file.get_path())),
                    true,
                    <dyn IOInterface>::DEBUG,
                );
            }
            Self::validate_json_schema(
                io,
                ValidateJsonInput::File(auth_file.clone()),
                JsonFile::AUTH_SCHEMA,
                None,
            )?;
            let mut wrapped: IndexMap<String, PhpMixed> = IndexMap::new();
            wrapped.insert("config".to_string(), PhpMixed::Array(auth_file.read()?
                .into_iter().map(|(k, v)| (k, Box::new(v))).collect()));
            config.merge(wrapped, auth_file.get_path().to_string());
        }
        config.set_auth_config_source(JsonConfigSource::new(auth_file, true));

        Self::load_composer_auth_env(&mut config, io)?;

        Ok(config)
    }

    pub fn get_composer_file() -> anyhow::Result<String> {
        let env = Platform::get_env("COMPOSER");
        if let Some(env_str) = env {
            let env_trimmed = trim(&env_str, " \t\n\r\0\u{0B}");
            if env_trimmed != "" {
                if is_dir(&env_trimmed) {
                    return Err(anyhow::anyhow!(RuntimeException {
                        message: format!(
                            "The COMPOSER environment variable is set to {} which is a directory, this variable should point to a composer.json or be left unset.",
                            env_trimmed
                        ),
                        code: 0,
                    }));
                }

                return Ok(env_trimmed);
            }
        }

        Ok("./composer.json".to_string())
    }

    pub fn get_lock_file(composer_file: &str) -> String {
        let ext = pathinfo(PhpMixed::String(composer_file.to_string()), PATHINFO_EXTENSION);
        let is_json = match ext {
            PhpMixed::String(s) => s == "json",
            _ => false,
        };
        if is_json {
            format!("{}lock", substr(composer_file, 0, Some(composer_file.len() as i64 - 4)))
        } else {
            format!("{}.lock", composer_file)
        }
    }

    pub fn create_additional_styles() -> IndexMap<String, OutputFormatterStyle> {
        let mut styles: IndexMap<String, OutputFormatterStyle> = IndexMap::new();
        styles.insert(
            "highlight".to_string(),
            OutputFormatterStyle::new(Some("red".to_string()), None, Vec::new()),
        );
        styles.insert(
            "warning".to_string(),
            OutputFormatterStyle::new(Some("black".to_string()), Some("yellow".to_string()), Vec::new()),
        );
        styles
    }

    pub fn create_output() -> ConsoleOutput {
        let styles = Self::create_additional_styles();
        let formatter = OutputFormatter::new(false, styles);

        ConsoleOutput::new_with_formatter(ConsoleOutput::VERBOSITY_NORMAL, None, formatter)
    }

    /// Creates a Composer instance
    pub fn create_composer(
        &self,
        io: &dyn IOInterface,
        local_config: Option<LocalConfigInput>,
        disable_plugins: DisablePlugins,
        cwd: Option<&str>,
        full_load: bool,
        disable_scripts: bool,
    ) -> anyhow::Result<PartialComposerOrComposer> {
        // if a custom composer.json path is given, we change the default cwd to be that file's directory
        let mut local_config = local_config;
        let mut cwd = cwd.map(|s| s.to_string());
        if let Some(LocalConfigInput::Path(ref s)) = local_config {
            if is_file(s) && cwd.is_none() {
                cwd = Some(dirname(s));
            }
        }

        let cwd = cwd.unwrap_or_else(|| Platform::get_cwd(true));

        // load Composer configuration
        if local_config.is_none() {
            local_config = Some(LocalConfigInput::Path(Self::get_composer_file()?));
        }

        let mut local_config_source = Config::SOURCE_UNKNOWN.to_string();
        let mut composer_file: Option<String> = None;
        let mut local_config_data: IndexMap<String, PhpMixed> = IndexMap::new();
        if let Some(LocalConfigInput::Path(path)) = &local_config {
            composer_file = Some(path.clone());

            let file = JsonFile::new(path.clone(), None, Some(io));

            if !file.exists() {
                let message = if path == "./composer.json" || path == "composer.json" {
                    format!("Composer could not find a composer.json file in {}", cwd)
                } else {
                    format!("Composer could not find the config file: {}", path)
                };
                let instructions = if full_load {
                    "To initialize a project, please create a composer.json file. See https://getcomposer.org/basic-usage"
                } else {
                    ""
                };
                return Err(anyhow::anyhow!(InvalidArgumentException {
                    message: format!("{}{}{}", message, PHP_EOL, instructions),
                    code: 0,
                }));
            }

            if !Platform::is_input_completion_process() {
                if let Err(e) = file.validate_schema(JsonFile::LAX_SCHEMA) {
                    if let Some(jve) = e.downcast_ref::<JsonValidationException>() {
                        let errors =
                            format!(" - {}", implode(&format!("{} - ", PHP_EOL), jve.get_errors()));
                        let message =
                            format!("{}:{}{}", jve.get_message(), PHP_EOL, errors);
                        return Err(anyhow::anyhow!(JsonValidationException::new(
                            message,
                            jve.get_errors().clone(),
                        )));
                    }
                    return Err(e);
                }
            }

            local_config_data = file.read()?;
            local_config_source = file.get_path().to_string();
        } else if let Some(LocalConfigInput::Data(data)) = local_config {
            local_config_data = data;
        }

        // Load config and override with local config/auth config
        let mut config = Self::create_config(Some(io), Some(&cwd))?;
        let is_global = local_config_source != Config::SOURCE_UNKNOWN
            && realpath(&config.get_str("home")?) == realpath(&dirname(&local_config_source));
        config.merge(local_config_data.clone(), local_config_source.clone());

        if let Some(ref composer_file_path) = composer_file {
            io.write_error(
                PhpMixed::String(format!(
                    "Loading config file {} ({})",
                    composer_file_path,
                    realpath(composer_file_path).unwrap_or_default()
                )),
                true,
                <dyn IOInterface>::DEBUG,
            );
            config.set_config_source(JsonConfigSource::new(
                JsonFile::new(realpath(composer_file_path).unwrap_or_default(), None, Some(io)),
                false,
            ));

            let local_auth_file = JsonFile::new(
                format!(
                    "{}/auth.json",
                    dirname(&realpath(composer_file_path).unwrap_or_default())
                ),
                None,
                Some(io),
            );
            if local_auth_file.exists() {
                io.write_error(
                    PhpMixed::String(format!(
                        "Loading config file {}",
                        local_auth_file.get_path()
                    )),
                    true,
                    <dyn IOInterface>::DEBUG,
                );
                Self::validate_json_schema(
                    Some(io),
                    ValidateJsonInput::File(local_auth_file.clone()),
                    JsonFile::AUTH_SCHEMA,
                    None,
                )?;
                let mut wrapped: IndexMap<String, PhpMixed> = IndexMap::new();
                wrapped.insert(
                    "config".to_string(),
                    PhpMixed::Array(
                        local_auth_file
                            .read()?
                            .into_iter()
                            .map(|(k, v)| (k, Box::new(v)))
                            .collect(),
                    ),
                );
                config.merge(wrapped, local_auth_file.get_path().to_string());
                config.set_local_auth_config_source(JsonConfigSource::new(local_auth_file, true));
            }
        }

        // make sure we load the auth env again over the local auth.json + composer.json config
        Self::load_composer_auth_env(&mut config, Some(io))?;

        let vendor_dir = config.get_str("vendor-dir")?;

        // initialize composer
        let mut composer: PartialComposerOrComposer = if full_load {
            PartialComposerOrComposer::Full(Composer::new())
        } else {
            PartialComposerOrComposer::Partial(PartialComposer::default())
        };
        composer.set_config(config.clone());
        if is_global {
            composer.set_global();
        }

        if full_load {
            // load auth configs into the IO instance
            io.load_configuration(&config);

            // load existing Composer\InstalledVersions instance if available and scripts/plugins are allowed, as they might need it
            // we only load if the InstalledVersions class wasn't defined yet so that this is only loaded once
            let installed_versions_path =
                format!("{}/composer/installed.php", config.get_str("vendor-dir")?);
            if !disable_plugins.is_disabled_at_all()
                && !disable_scripts
                && !class_exists("Composer\\InstalledVersions")
                && file_exists(&installed_versions_path)
            {
                // force loading the class at this point so it is loaded from the composer phar and not from the vendor dir
                // as we cannot guarantee integrity of that file
                if class_exists("Composer\\InstalledVersions") {
                    FilesystemRepository::safely_load_installed_versions(&installed_versions_path);
                }
            }
        }

        let http_downloader = Self::create_http_downloader(io, &config, IndexMap::new())?;
        let process = ProcessExecutor::new(io);
        let r#loop = Loop::new(http_downloader.clone(), process.clone());
        composer.set_loop(r#loop.clone());

        // initialize event dispatcher
        let mut dispatcher = EventDispatcher::new(
            composer.as_partial(),
            io.clone_box(),
            Some(process.clone()),
        );
        dispatcher.set_run_scripts(!disable_scripts);
        composer.set_event_dispatcher(dispatcher.clone());

        // initialize repository manager
        let rm = RepositoryFactory::manager(
            io,
            &config,
            &http_downloader,
            &dispatcher,
            &process,
        )?;
        composer.set_repository_manager(rm.clone());

        // force-set the version of the global package if not defined as
        // guessing it adds no value and only takes time
        if !full_load && !local_config_data.contains_key("version") {
            local_config_data.insert(
                "version".to_string(),
                PhpMixed::String("1.0.0".to_string()),
            );
        }

        // load package
        let parser = VersionParser::new();
        let guesser = VersionGuesser::new(&config, process.clone(), parser.clone());
        let mut loader = self.load_root_package(rm.clone(), config.clone(), parser, guesser, io.clone_box());
        let package = loader.load(
            local_config_data
                .iter()
                .map(|(k, v)| (k.clone(), Box::new(v.clone())))
                .collect(),
            "Composer\\Package\\RootPackage",
            Some(&cwd),
        )?;
        composer.set_package(package);

        // load local repository
        self.add_local_repository(io, rm.clone(), &vendor_dir, composer.get_package(), Some(&process));

        // initialize installation manager
        let im = self.create_installation_manager(r#loop.clone(), io.clone_box(), Some(dispatcher.clone()));
        composer.set_installation_manager(im.clone());

        if let PartialComposerOrComposer::Full(ref mut composer_full) = composer {
            // initialize download manager
            let dm = self.create_download_manager(io, &config, &http_downloader, &process, Some(&dispatcher))?;
            composer_full.set_download_manager(dm.clone());

            // initialize autoload generator
            let generator = AutoloadGenerator::new(&dispatcher, io.clone_box());
            composer_full.set_autoload_generator(generator);

            // initialize archive manager
            let am = self.create_archive_manager(&config, &dm, &r#loop)?;
            composer_full.set_archive_manager(am);
        }

        // add installers to the manager (must happen after download manager is created since they read it out of $composer)
        self.create_default_installers(&im, &composer, io, Some(&process));

        // init locker if possible
        if let PartialComposerOrComposer::Full(ref mut composer_full) = composer {
            if let Some(ref composer_file_path) = composer_file {
                let lock_file = Self::get_lock_file(composer_file_path);
                let lock_enabled =
                    config.get("lock").and_then(|v| v.as_bool()).unwrap_or(true);
                if !lock_enabled && file_exists(&lock_file) {
                    io.write_error(
                        PhpMixed::String(format!(
                            "<warning>{} is present but ignored as the \"lock\" config option is disabled.</warning>",
                            lock_file
                        )),
                        true,
                        <dyn IOInterface>::NORMAL,
                    );
                }

                let locker = Locker::new(
                    io.clone_box(),
                    JsonFile::new(
                        if lock_enabled {
                            lock_file
                        } else {
                            Platform::get_dev_null()
                        },
                        None,
                        Some(io),
                    ),
                    im.clone(),
                    file_get_contents(composer_file_path).unwrap_or_default(),
                    process.clone(),
                );
                composer_full.set_locker(locker);
            } else {
                let locker = Locker::new(
                    io.clone_box(),
                    JsonFile::new(Platform::get_dev_null(), None, Some(io)),
                    im.clone(),
                    JsonFile::encode(&PhpMixed::Array(
                        local_config_data
                            .iter()
                            .map(|(k, v)| (k.clone(), Box::new(v.clone())))
                            .collect(),
                    )),
                    process.clone(),
                );
                composer_full.set_locker(locker);
            }
        }

        if let PartialComposerOrComposer::Full(ref mut composer_full) = composer {
            let mut global_composer: Option<PartialComposer> = None;
            if !composer_full.is_global() {
                global_composer = self.create_global_composer(
                    io,
                    &config,
                    disable_plugins,
                    disable_scripts,
                    false,
                );
            }

            let mut pm = self.create_plugin_manager(io, composer_full, global_composer.as_ref(), disable_plugins);
            composer_full.set_plugin_manager(pm.clone());

            if composer_full.is_global() {
                pm.set_running_in_global_dir(true);
            }

            pm.load_installed_plugins();
        }

        if full_load {
            let init_event = Event::from_name(PluginEvents::INIT.to_string());
            composer
                .get_event_dispatcher_mut()
                .dispatch(Some(init_event.get_name()), Some(init_event))?;

            // once everything is initialized we can
            // purge packages from local repos if they have been deleted on the filesystem
            self.purge_packages(rm.get_local_repository(), &im);
        }

        Ok(composer)
    }

    pub fn create_global(
        io: &dyn IOInterface,
        disable_plugins: DisablePlugins,
        disable_scripts: bool,
    ) -> Option<Composer> {
        let factory = Self;

        let config = Self::create_config(Some(io), None).ok()?;
        factory
            .create_global_composer(io, &config, disable_plugins, disable_scripts, true)
            .and_then(|pc| match pc {
                _ => None, // TODO(phase-b): downcast PartialComposer to Composer when fullLoad=true
            })
    }

    fn add_local_repository(
        &self,
        io: &dyn IOInterface,
        mut rm: RepositoryManager,
        vendor_dir: &str,
        root_package: &dyn RootPackageInterface,
        process: Option<&ProcessExecutor>,
    ) {
        let fs = process.map(|p| Filesystem::new(Some(p.clone())));

        rm.set_local_repository(Box::new(InstalledFilesystemRepository::new(
            JsonFile::new(
                format!("{}/composer/installed.json", vendor_dir),
                None,
                Some(io),
            ),
            true,
            root_package.clone_box(),
            fs,
        )));
    }

    fn create_global_composer(
        &self,
        io: &dyn IOInterface,
        config: &Config,
        disable_plugins: DisablePlugins,
        disable_scripts: bool,
        full_load: bool,
    ) -> Option<PartialComposer> {
        // make sure if disable plugins was 'local' it is now turned off
        let disable_plugins = if matches!(disable_plugins, DisablePlugins::Global | DisablePlugins::All) {
            DisablePlugins::All
        } else {
            DisablePlugins::None
        };

        let composer = match self.create_composer(
            io,
            Some(LocalConfigInput::Path(format!(
                "{}/composer.json",
                config.get_str("home").ok()?
            ))),
            disable_plugins,
            Some(&config.get_str("home").ok()?),
            full_load,
            disable_scripts,
        ) {
            Ok(c) => Some(c.into_partial()),
            Err(e) => {
                io.write_error(
                    PhpMixed::String(format!(
                        "Failed to initialize global composer: {}",
                        e
                    )),
                    true,
                    <dyn IOInterface>::DEBUG,
                );
                None
            }
        };

        composer
    }

    pub fn create_download_manager(
        &self,
        io: &dyn IOInterface,
        config: &Config,
        http_downloader: &HttpDownloader,
        process: &ProcessExecutor,
        event_dispatcher: Option<&EventDispatcher>,
    ) -> anyhow::Result<DownloadManager> {
        let mut cache: Option<Cache> = None;
        if config
            .get("cache-files-ttl")
            .and_then(|v| v.as_int())
            .unwrap_or(0)
            > 0
        {
            let mut c = Cache::new(io, &config.get_str("cache-files-dir")?, "a-z0-9_./");
            c.set_read_only(
                config
                    .get("cache-read-only")
                    .and_then(|v| v.as_bool())
                    .unwrap_or(false),
            );
            cache = Some(c);
        }

        let fs = Filesystem::new(Some(process.clone()));

        let mut dm = DownloadManager::new(io.clone_box(), false, fs.clone());
        let preferred = config.get("preferred-install").cloned();
        match preferred.as_ref().and_then(|v| v.as_string()) {
            Some("dist") => {
                dm.set_prefer_dist(true);
            }
            Some("source") => {
                dm.set_prefer_source(true);
            }
            Some("auto") | _ => {
                // noop
            }
        }

        if let Some(PhpMixed::Array(prefs)) = preferred {
            dm.set_preferences(
                prefs
                    .into_iter()
                    .map(|(k, v)| {
                        (
                            k,
                            match *v {
                                PhpMixed::String(s) => s,
                                _ => String::new(),
                            },
                        )
                    })
                    .collect(),
            );
        }

        dm.set_downloader(
            "git",
            Box::new(GitDownloader::new(io.clone_box(), config.clone(), process.clone(), fs.clone())),
        );
        dm.set_downloader(
            "svn",
            Box::new(SvnDownloader::new(io.clone_box(), config.clone(), process.clone(), fs.clone())),
        );
        dm.set_downloader(
            "fossil",
            Box::new(FossilDownloader::new(io.clone_box(), config.clone(), process.clone(), fs.clone())),
        );
        dm.set_downloader(
            "hg",
            Box::new(HgDownloader::new(io.clone_box(), config.clone(), process.clone(), fs.clone())),
        );
        dm.set_downloader(
            "perforce",
            Box::new(PerforceDownloader::new(io.clone_box(), config.clone(), process.clone(), fs.clone())),
        );
        dm.set_downloader(
            "zip",
            Box::new(ZipDownloader::new(
                io.clone_box(),
                config.clone(),
                http_downloader.clone(),
                event_dispatcher.cloned(),
                cache.clone(),
                fs.clone(),
                process.clone(),
            )),
        );
        dm.set_downloader(
            "rar",
            Box::new(RarDownloader::new(
                io.clone_box(),
                config.clone(),
                http_downloader.clone(),
                event_dispatcher.cloned(),
                cache.clone(),
                fs.clone(),
                process.clone(),
            )),
        );
        dm.set_downloader(
            "tar",
            Box::new(TarDownloader::new(
                io.clone_box(),
                config.clone(),
                http_downloader.clone(),
                event_dispatcher.cloned(),
                cache.clone(),
                fs.clone(),
                process.clone(),
            )),
        );
        dm.set_downloader(
            "gzip",
            Box::new(GzipDownloader::new(
                io.clone_box(),
                config.clone(),
                http_downloader.clone(),
                event_dispatcher.cloned(),
                cache.clone(),
                fs.clone(),
                process.clone(),
            )),
        );
        dm.set_downloader(
            "xz",
            Box::new(XzDownloader::new(
                io.clone_box(),
                config.clone(),
                http_downloader.clone(),
                event_dispatcher.cloned(),
                cache.clone(),
                fs.clone(),
                process.clone(),
            )),
        );
        dm.set_downloader(
            "phar",
            Box::new(PharDownloader::new(
                io.clone_box(),
                config.clone(),
                http_downloader.clone(),
                event_dispatcher.cloned(),
                cache.clone(),
                fs.clone(),
                process.clone(),
            )),
        );
        dm.set_downloader(
            "file",
            Box::new(FileDownloader::new(
                io.clone_box(),
                config.clone(),
                http_downloader.clone(),
                event_dispatcher.cloned(),
                cache.clone(),
                fs.clone(),
                process.clone(),
            )),
        );
        dm.set_downloader(
            "path",
            Box::new(PathDownloader::new(
                io.clone_box(),
                config.clone(),
                http_downloader.clone(),
                event_dispatcher.cloned(),
                cache.clone(),
                fs.clone(),
                process.clone(),
            )),
        );

        Ok(dm)
    }

    pub fn create_archive_manager(
        &self,
        _config: &Config,
        dm: &DownloadManager,
        r#loop: &Loop,
    ) -> anyhow::Result<ArchiveManager> {
        let mut am = ArchiveManager::new(dm.clone(), r#loop.clone());
        if class_exists("ZipArchive") {
            am.add_archiver(Box::new(ZipArchiver::new()));
        }
        if class_exists("Phar") {
            am.add_archiver(Box::new(PharArchiver::new()));
        }

        Ok(am)
    }

    fn create_plugin_manager(
        &self,
        io: &dyn IOInterface,
        composer: &Composer,
        global_composer: Option<&PartialComposer>,
        disable_plugins: DisablePlugins,
    ) -> PluginManager {
        PluginManager::new(io.clone_box(), composer.clone(), global_composer.cloned(), disable_plugins)
    }

    pub fn create_installation_manager(
        &self,
        r#loop: Loop,
        io: Box<dyn IOInterface>,
        event_dispatcher: Option<EventDispatcher>,
    ) -> InstallationManager {
        InstallationManager::new(r#loop, io, event_dispatcher)
    }

    fn create_default_installers(
        &self,
        im: &InstallationManager,
        composer: &PartialComposerOrComposer,
        io: &dyn IOInterface,
        process: Option<&ProcessExecutor>,
    ) {
        let fs = Filesystem::new(process.cloned());
        let bin_dir = trim(
            &composer
                .get_config()
                .get_str("bin-dir")
                .unwrap_or_default(),
            "/",
        );
        let bin_compat = composer
            .get_config()
            .get_str("bin-compat")
            .unwrap_or_default();
        let vendor_dir = trim(
            &composer
                .get_config()
                .get_str("vendor-dir")
                .unwrap_or_default(),
            "/",
        );
        let binary_installer = BinaryInstaller::new(
            io.clone_box(),
            bin_dir,
            bin_compat,
            fs.clone(),
            vendor_dir,
        );

        let mut im = im.clone();
        im.add_installer(Box::new(LibraryInstaller::new(
            io.clone_box(),
            composer.as_partial(),
            None,
            fs.clone(),
            binary_installer.clone(),
        )));
        im.add_installer(Box::new(PluginInstaller::new(
            io.clone_box(),
            composer.as_partial(),
            fs.clone(),
            binary_installer.clone(),
        )));
        im.add_installer(Box::new(MetapackageInstaller::new(io.clone_box())));
    }

    fn purge_packages(
        &self,
        repo: &dyn InstalledRepositoryInterface,
        im: &InstallationManager,
    ) {
        for package in repo.get_packages() {
            if !im.is_package_installed(repo, package.as_ref()) {
                // TODO(phase-b): mutable access on repo trait object
                let _ = package;
            }
        }
    }

    fn load_root_package(
        &self,
        rm: RepositoryManager,
        config: Config,
        parser: VersionParser,
        guesser: VersionGuesser,
        io: Box<dyn IOInterface>,
    ) -> RootPackageLoader {
        RootPackageLoader::new(rm, config, Some(parser), Some(guesser), Some(io))
    }

    pub fn create(
        io: &dyn IOInterface,
        config: Option<LocalConfigInput>,
        disable_plugins: DisablePlugins,
        disable_scripts: bool,
    ) -> anyhow::Result<Composer> {
        let factory = Self;

        // for BC reasons, if a config is passed in either as array or a path that is not the default composer.json path
        // we disable local plugins as they really should not be loaded from CWD
        // If you want to avoid this behavior, you should be calling createComposer directly with a $cwd arg set correctly
        // to the path where the composer.json being loaded resides
        let default_composer_file = Self::get_composer_file()?;
        let config_is_default = matches!(
            config.as_ref(),
            Some(LocalConfigInput::Path(p)) if *p == default_composer_file
        );
        let disable_plugins = if config.is_some()
            && !config_is_default
            && matches!(disable_plugins, DisablePlugins::None)
        {
            DisablePlugins::Local
        } else {
            disable_plugins
        };

        match factory.create_composer(io, config, disable_plugins, None, true, disable_scripts)? {
            PartialComposerOrComposer::Full(c) => Ok(c),
            PartialComposerOrComposer::Partial(_) => {
                // TODO(phase-b): unreachable when fullLoad=true; downcasting needs design.
                Err(anyhow::anyhow!(RuntimeException {
                    message: "Composer expected with fullLoad=true".to_string(),
                    code: 0,
                }))
            }
        }
    }

    /// If you are calling this in a plugin, you probably should instead use `$composer->getLoop()->getHttpDownloader()`
    pub fn create_http_downloader(
        io: &dyn IOInterface,
        config: &Config,
        options: IndexMap<String, PhpMixed>,
    ) -> anyhow::Result<HttpDownloader> {
        // TODO(plugin): static `$warned` flag — port as a OnceCell or atomic in Phase B.
        static mut WARNED: bool = false;
        let mut disable_tls = false;
        // allow running the config command if disable-tls is in the arg list, even if openssl is missing, to allow disabling it via the config command
        let argv = Platform::server_argv().unwrap_or_default();
        if !argv.is_empty()
            && argv.contains(&"disable-tls".to_string())
            && (argv.contains(&"conf".to_string()) || argv.contains(&"config".to_string()))
        {
            unsafe { WARNED = true };
            disable_tls = !extension_loaded("openssl");
        } else if config
            .get("disable-tls")
            .and_then(|v| v.as_bool())
            .unwrap_or(false)
        {
            if !unsafe { WARNED } {
                io.write_error(
                    PhpMixed::String(
                        "<warning>You are running Composer with SSL/TLS protection disabled.</warning>"
                            .to_string(),
                    ),
                    true,
                    <dyn IOInterface>::NORMAL,
                );
            }
            unsafe { WARNED = true };
            disable_tls = true;
        } else if !extension_loaded("openssl") {
            return Err(anyhow::anyhow!(NoSslException(RuntimeException {
                message:
                    "The openssl extension is required for SSL/TLS protection but is not available. If you can not enable the openssl extension, you can disable this error, at your own risk, by setting the 'disable-tls' option to true."
                        .to_string(),
                code: 0,
            })));
        }
        let mut http_downloader_options: IndexMap<String, PhpMixed> = IndexMap::new();
        if !disable_tls {
            if "" != config.get_str("cafile").unwrap_or_default() {
                let mut ssl_map: IndexMap<String, PhpMixed> = IndexMap::new();
                ssl_map.insert(
                    "cafile".to_string(),
                    PhpMixed::String(config.get_str("cafile").unwrap_or_default()),
                );
                http_downloader_options.insert(
                    "ssl".to_string(),
                    PhpMixed::Array(ssl_map.into_iter().map(|(k, v)| (k, Box::new(v))).collect()),
                );
            }
            if "" != config.get_str("capath").unwrap_or_default() {
                let existing_ssl = http_downloader_options
                    .get("ssl")
                    .and_then(|v| v.as_array())
                    .cloned()
                    .unwrap_or_default();
                let mut ssl_map: IndexMap<String, Box<PhpMixed>> = existing_ssl;
                ssl_map.insert(
                    "capath".to_string(),
                    Box::new(PhpMixed::String(config.get_str("capath").unwrap_or_default())),
                );
                http_downloader_options.insert("ssl".to_string(), PhpMixed::Array(ssl_map));
            }
            http_downloader_options =
                array_replace_recursive(http_downloader_options, options.clone());
        }
        let http_downloader = match HttpDownloader::new_full(io.clone_box(), config.clone(), http_downloader_options, disable_tls) {
            Ok(h) => h,
            Err(e) => {
                if let Some(te) = e.downcast_ref::<TransportException>() {
                    if strpos(&te.get_message(), "cafile").is_some() {
                        io.write(
                            PhpMixed::String(
                                "<error>Unable to locate a valid CA certificate file. You must set a valid 'cafile' option.</error>"
                                    .to_string(),
                            ),
                            true,
                            <dyn IOInterface>::NORMAL,
                        );
                        io.write(
                            PhpMixed::String(
                                "<error>A valid CA certificate file is required for SSL/TLS protection.</error>"
                                    .to_string(),
                            ),
                            true,
                            <dyn IOInterface>::NORMAL,
                        );
                        io.write(
                            PhpMixed::String(
                                "<error>You can disable this error, at your own risk, by setting the 'disable-tls' option to true.</error>"
                                    .to_string(),
                            ),
                            true,
                            <dyn IOInterface>::NORMAL,
                        );
                    }
                }
                return Err(e);
            }
        };

        Ok(http_downloader)
    }

    fn load_composer_auth_env(
        config: &mut Config,
        io: Option<&dyn IOInterface>,
    ) -> anyhow::Result<()> {
        let composer_auth_env = Platform::get_env("COMPOSER_AUTH");
        let composer_auth_env_str = match composer_auth_env {
            Some(s) if !s.is_empty() => s,
            _ => return Ok(()),
        };

        let auth_data = json_decode(&composer_auth_env_str, false)?;
        if matches!(auth_data, PhpMixed::Null) {
            return Err(anyhow::anyhow!(UnexpectedValueException {
                message:
                    "COMPOSER_AUTH environment variable is malformed, should be a valid JSON object"
                        .to_string(),
                code: 0,
            }));
        }

        if let Some(io_ref) = io {
            io_ref.write_error(
                PhpMixed::String("Loading auth config from COMPOSER_AUTH".to_string()),
                true,
                <dyn IOInterface>::DEBUG,
            );
        }
        Self::validate_json_schema(
            io,
            ValidateJsonInput::Data(auth_data.clone()),
            JsonFile::AUTH_SCHEMA,
            Some("COMPOSER_AUTH"),
        )?;
        let auth_data_assoc = json_decode(&composer_auth_env_str, true)?;
        if !matches!(auth_data_assoc, PhpMixed::Null) {
            let mut wrapped: IndexMap<String, PhpMixed> = IndexMap::new();
            wrapped.insert("config".to_string(), auth_data_assoc);
            config.merge(wrapped, "COMPOSER_AUTH".to_string());
        }
        Ok(())
    }

    fn use_xdg() -> bool {
        for key in array_keys(&Platform::server_env()) {
            if strpos(&key, "XDG_") == Some(0) {
                return true;
            }
        }

        Silencer::call(|| Ok::<bool, anyhow::Error>(is_dir("/etc/xdg"))).unwrap_or(false)
    }

    fn get_user_dir() -> anyhow::Result<String> {
        let home = Platform::get_env("HOME").unwrap_or_default();
        if home.is_empty() {
            return Err(anyhow::anyhow!(RuntimeException {
                message:
                    "The HOME or COMPOSER_HOME environment variable must be set for composer to run correctly"
                        .to_string(),
                code: 0,
            }));
        }

        Ok(trim(&strtr(&home, "\\", "/"), "/"))
    }

    fn validate_json_schema(
        io: Option<&dyn IOInterface>,
        file_or_data: ValidateJsonInput,
        schema: i64,
        source: Option<&str>,
    ) -> anyhow::Result<()> {
        if Platform::is_input_completion_process() {
            return Ok(());
        }

        let result = match file_or_data {
            ValidateJsonInput::File(file) => file.validate_schema(schema),
            ValidateJsonInput::Data(data) => {
                let source = source.ok_or_else(|| {
                    anyhow::anyhow!(InvalidArgumentException {
                        message:
                            "$source is required to be provided if $fileOrData is arbitrary data"
                                .to_string(),
                        code: 0,
                    })
                })?;
                JsonFile::validate_json_schema(source, &data, schema)
            }
        };

        if let Err(e) = result {
            if let Some(jve) = e.downcast_ref::<JsonValidationException>() {
                let msg = format!(
                    "{}, this may result in errors and should be resolved:{} - {}",
                    jve.get_message(),
                    PHP_EOL,
                    implode(&format!("{} - ", PHP_EOL), jve.get_errors())
                );
                if let Some(io_ref) = io {
                    io_ref.write_error(
                        PhpMixed::String(format!("<warning>{}</>", msg)),
                        true,
                        <dyn IOInterface>::NORMAL,
                    );
                } else {
                    return Err(anyhow::anyhow!(UnexpectedValueException { message: msg, code: 0 }));
                }
            } else {
                return Err(e);
            }
        }
        Ok(())
    }
}

enum ValidateJsonInput {
    File(JsonFile),
    Data(PhpMixed),
}

/// `Factory::createComposer` returns either a `Composer` (`$fullLoad=true`) or a `PartialComposer`.
pub enum PartialComposerOrComposer {
    Full(Composer),
    Partial(PartialComposer),
}

impl PartialComposerOrComposer {
    fn set_config(&mut self, config: Config) {
        match self {
            Self::Full(c) => c.set_config(config),
            Self::Partial(p) => p.set_config(config),
        }
    }
    fn set_global(&mut self) {
        match self {
            Self::Full(c) => c.set_global(),
            Self::Partial(p) => p.set_global(),
        }
    }
    fn set_loop(&mut self, r#loop: Loop) {
        match self {
            Self::Full(c) => c.set_loop(r#loop),
            Self::Partial(p) => p.set_loop(r#loop),
        }
    }
    fn set_event_dispatcher(&mut self, dispatcher: EventDispatcher) {
        match self {
            Self::Full(c) => c.set_event_dispatcher(dispatcher),
            Self::Partial(p) => p.set_event_dispatcher(dispatcher),
        }
    }
    fn set_repository_manager(&mut self, rm: RepositoryManager) {
        match self {
            Self::Full(c) => c.set_repository_manager(rm),
            Self::Partial(p) => p.set_repository_manager(rm),
        }
    }
    fn set_installation_manager(&mut self, im: InstallationManager) {
        match self {
            Self::Full(c) => c.set_installation_manager(im),
            Self::Partial(p) => p.set_installation_manager(im),
        }
    }
    fn set_package(&mut self, package: Box<dyn RootPackageInterface>) {
        match self {
            Self::Full(c) => c.set_package(package),
            Self::Partial(p) => p.set_package(package),
        }
    }
    fn get_package(&self) -> &dyn RootPackageInterface {
        match self {
            Self::Full(c) => c.get_package(),
            Self::Partial(p) => p.get_package(),
        }
    }
    fn get_config(&self) -> &Config {
        match self {
            Self::Full(c) => c.get_config(),
            Self::Partial(p) => p.get_config(),
        }
    }
    fn get_event_dispatcher_mut(&mut self) -> &mut EventDispatcher {
        match self {
            Self::Full(c) => c.get_event_dispatcher_mut(),
            Self::Partial(p) => p.get_event_dispatcher_mut(),
        }
    }
    fn as_partial(&self) -> PartialComposer {
        // TODO(phase-b): exact clone semantics differ across Composer/PartialComposer.
        match self {
            Self::Full(_) => PartialComposer::default(),
            Self::Partial(p) => p.clone(),
        }
    }
    fn into_partial(self) -> PartialComposer {
        match self {
            Self::Full(_) => PartialComposer::default(),
            Self::Partial(p) => p,
        }
    }
}
