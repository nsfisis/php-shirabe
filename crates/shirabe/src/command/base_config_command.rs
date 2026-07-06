//! ref: composer/src/Composer/Command/BaseConfigCommand.php

use crate::command::BaseCommand;
use crate::command::base_command::base_command_initialize;
use crate::config::Config;
use crate::config::JsonConfigSource;
use crate::factory::Factory;
use crate::json::JsonFile;
use crate::util::Platform;
use crate::util::Silencer;
use indexmap::IndexMap;
use shirabe_external_packages::symfony::console::input::InputInterface;
use shirabe_external_packages::symfony::console::output::OutputInterface;
use shirabe_php_shim::{PhpMixed, chmod, touch};

pub trait BaseConfigCommand: BaseCommand {
    fn config(&self) -> Option<std::rc::Rc<std::cell::RefCell<Config>>>;
    fn set_config(&self, config: Option<std::rc::Rc<std::cell::RefCell<Config>>>);
    fn config_file(&self) -> Option<std::rc::Rc<std::cell::RefCell<JsonFile>>>;
    fn set_config_file(&self, file: Option<std::rc::Rc<std::cell::RefCell<JsonFile>>>);
    fn set_config_source(&self, source: Option<JsonConfigSource>);

    fn initialize(
        &self,
        input: std::rc::Rc<std::cell::RefCell<dyn InputInterface>>,
        output: std::rc::Rc<std::cell::RefCell<dyn OutputInterface>>,
    ) -> anyhow::Result<()>
    where
        Self: Sized,
    {
        base_command_initialize(self, input.clone(), output.clone())?;

        if input
            .borrow()
            .get_option("global")?
            .as_bool()
            .unwrap_or(false)
            && !input.borrow().get_option("file")?.is_null()
        {
            return Err(anyhow::anyhow!("--file and --global can not be combined"));
        }

        let io = self.get_io();
        self.set_config(Some(std::rc::Rc::new(std::cell::RefCell::new(
            Factory::create_config(Some(io.clone()), None)?,
        ))));
        let config_rc = self.config().unwrap();

        // When using --global flag, set baseDir to home directory for correct absolute path resolution
        if input
            .borrow()
            .get_option("global")?
            .as_bool()
            .unwrap_or(false)
        {
            let home = config_rc.borrow_mut().get("home").to_string();
            config_rc.borrow_mut().set_base_dir(Some(home));
        }

        let config_file = self.get_composer_config_file(input.clone(), &config_rc.borrow())?;

        // Create global composer.json if invoked using `composer global [config-cmd]`
        if (config_file == "composer.json" || config_file == "./composer.json")
            && !std::path::Path::new(&config_file).exists()
            && std::fs::canonicalize(Platform::get_cwd(false)?).ok()
                == std::fs::canonicalize(config_rc.borrow_mut().get("home").to_string()).ok()
        {
            std::fs::write(&config_file, "{\n}\n")?;
        }
        let config_file_jf = std::rc::Rc::new(std::cell::RefCell::new(JsonFile::new(
            config_file.clone(),
            None,
            Some(io.clone()),
        )?));
        self.set_config_file(Some(config_file_jf.clone()));
        self.set_config_source(Some(JsonConfigSource::new(config_file_jf, false)));

        // Initialize the global file if it's not there, ignoring any warnings or notices
        if input
            .borrow()
            .get_option("global")?
            .as_bool()
            .unwrap_or(false)
            && !self.config_file().unwrap().borrow().exists()
        {
            let path = self.config_file().unwrap().borrow().get_path().to_string();
            touch(&path);
            self.config_file()
                .unwrap()
                .borrow()
                .write(PhpMixed::Array({
                    let mut m = IndexMap::new();
                    m.insert("config".to_string(), PhpMixed::Object(IndexMap::new()));
                    m
                }))?;
            let _ = Silencer::call(|| {
                chmod(&path, 0o600);
                Ok(())
            });
        }

        if !self.config_file().unwrap().borrow().exists() {
            return Err(anyhow::anyhow!(
                "File \"{}\" cannot be found in the current directory",
                config_file
            ));
        }

        Ok(())
    }

    /// Get the local composer.json, global config.json, or the file passed by the user
    fn get_composer_config_file(
        &self,
        input: std::rc::Rc<std::cell::RefCell<dyn InputInterface>>,
        config: &Config,
    ) -> anyhow::Result<String> {
        if input
            .borrow()
            .get_option("global")?
            .as_bool()
            .unwrap_or(false)
        {
            Ok(format!("{}/config.json", config.get("home")))
        } else {
            Ok(input
                .borrow()
                .get_option("file")?
                .as_string()
                .map(|s| s.to_string())
                .unwrap_or_else(|| Factory::get_composer_file().unwrap_or_default()))
        }
    }

    /// Get the local auth.json or global auth.json, or if the user passed in a file to use,
    /// the corresponding auth.json
    fn get_auth_config_file(
        &self,
        input: std::rc::Rc<std::cell::RefCell<dyn InputInterface>>,
        config: &Config,
    ) -> anyhow::Result<String> {
        if input
            .borrow()
            .get_option("global")?
            .as_bool()
            .unwrap_or(false)
        {
            Ok(format!("{}/auth.json", config.get("home")))
        } else {
            let composer_config = self.get_composer_config_file(input, config)?;
            let parent = std::path::Path::new(&composer_config)
                .parent()
                .map(|p| p.to_string_lossy().to_string())
                .unwrap_or_default();
            Ok(format!("{}/auth.json", parent))
        }
    }
}
