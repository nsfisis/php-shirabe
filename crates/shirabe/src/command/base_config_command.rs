//! ref: composer/src/Composer/Command/BaseConfigCommand.php

use crate::command::base_command::BaseCommand;
use crate::config::Config;
use crate::config::json_config_source::JsonConfigSource;
use crate::factory::Factory;
use crate::json::json_file::JsonFile;
use crate::util::platform::Platform;
use crate::util::silencer::Silencer;
use indexmap::IndexMap;
use shirabe_external_packages::symfony::console::input::input_interface::InputInterface;
use shirabe_external_packages::symfony::console::output::output_interface::OutputInterface;
use shirabe_php_shim::{PhpMixed, chmod, touch};

#[derive(Debug)]
pub struct BaseConfigCommand {
    inner: BaseCommand,
    pub(crate) config: Option<Config>,
    pub(crate) config_file: Option<JsonFile>,
    pub(crate) config_source: Option<JsonConfigSource>,
}

impl BaseConfigCommand {
    pub fn initialize(
        &mut self,
        input: &dyn InputInterface,
        output: &dyn OutputInterface,
    ) -> anyhow::Result<()> {
        self.inner.initialize(input, output)?;

        if input.get_option("global").as_bool() && input.get_option("file").is_not_null() {
            return Err(anyhow::anyhow!("--file and --global can not be combined"));
        }

        let io = self.inner.get_io();
        self.config = Some(Factory::create_config(io)?);
        let config = self.config.as_mut().unwrap();

        // When using --global flag, set baseDir to home directory for correct absolute path resolution
        if input.get_option("global").as_bool() {
            let home = config.get("home").to_string();
            config.set_base_dir(home);
        }

        let config_file = self.get_composer_config_file(input, config);

        // Create global composer.json if invoked using `composer global [config-cmd]`
        if (config_file == "composer.json" || config_file == "./composer.json")
            && !std::path::Path::new(&config_file).exists()
            && std::fs::canonicalize(Platform::get_cwd()).ok()
                == std::fs::canonicalize(config.get("home").to_string()).ok()
        {
            std::fs::write(&config_file, "{\n}\n")?;
        }

        let config = self.config.as_ref().unwrap();
        self.config_file = Some(JsonFile::new(config_file.clone(), None, Some(io)));
        self.config_source = Some(JsonConfigSource::new(self.config_file.as_ref().unwrap()));

        // Initialize the global file if it's not there, ignoring any warnings or notices
        if input.get_option("global").as_bool() && !self.config_file.as_ref().unwrap().exists() {
            let path = self.config_file.as_ref().unwrap().get_path().to_string();
            touch(&path);
            self.config_file.as_mut().unwrap().write(PhpMixed::Array({
                let mut m = IndexMap::new();
                m.insert(
                    "config".to_string(),
                    Box::new(PhpMixed::Array(IndexMap::new())),
                );
                m
            }))?;
            let _ = Silencer::call(|| {
                chmod(&path, 0o600);
                Ok(())
            });
        }

        if !self.config_file.as_ref().unwrap().exists() {
            return Err(anyhow::anyhow!(
                "File \"{}\" cannot be found in the current directory",
                config_file
            ));
        }

        Ok(())
    }

    /// Get the local composer.json, global config.json, or the file passed by the user
    pub(crate) fn get_composer_config_file(
        &self,
        input: &dyn InputInterface,
        config: &Config,
    ) -> String {
        if input.get_option("global").as_bool() {
            format!("{}/config.json", config.get("home"))
        } else {
            input
                .get_option("file")
                .as_string_opt()
                .map(|s| s.to_string())
                .unwrap_or_else(|| Factory::get_composer_file())
        }
    }

    /// Get the local auth.json or global auth.json, or if the user passed in a file to use,
    /// the corresponding auth.json
    pub(crate) fn get_auth_config_file(
        &self,
        input: &dyn InputInterface,
        config: &Config,
    ) -> String {
        if input.get_option("global").as_bool() {
            format!("{}/auth.json", config.get("home"))
        } else {
            let composer_config = self.get_composer_config_file(input, config);
            let parent = std::path::Path::new(&composer_config)
                .parent()
                .map(|p| p.to_string_lossy().to_string())
                .unwrap_or_default();
            format!("{}/auth.json", parent)
        }
    }
}
