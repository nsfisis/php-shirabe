//! ref: composer/src/Composer/Command/BaseConfigCommand.php sha1:b3b6b6fae4224776ba59b8fad4e26013467e6c73

use crate::command::{BaseCommand, base_command_initialize};
use crate::config::{Config, JsonConfigSource};
use crate::factory::Factory;
use crate::json::JsonFile;
use crate::util::Platform;
use bstr::BString;
use indexmap::IndexMap;
use shirabe_php_shim::{
    PhpMixed, RuntimeException, bformat, chmod, dirname, path_from_bytes, path_to_bytes, touch,
};
use shirabe_symfony_console::input::{InputInterface, InputInterfaceByName};
use shirabe_symfony_console::output::OutputInterface;

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
            return Err(RuntimeException::new(BString::from(
                "--file and --global can not be combined",
            ))
            .into());
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
            let home = config_rc.borrow().get_home()?;
            config_rc.borrow_mut().set_base_dir(Some(home));
        }

        let config_file = self.get_composer_config_file(input.clone(), &config_rc.borrow())?;

        // Create global composer.json if invoked using `composer global [config-cmd]`
        if (config_file.as_os_str() == "composer.json"
            || config_file.as_os_str() == "./composer.json")
            && !std::path::Path::new(&config_file).exists()
            && std::fs::canonicalize(Platform::get_cwd(false)?).ok()
                == std::fs::canonicalize(path_from_bytes(&config_rc.borrow().get_home()?)).ok()
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
            let path = self
                .config_file()
                .unwrap()
                .borrow()
                .get_path()
                .to_path_buf();
            touch(&path);
            self.config_file()
                .unwrap()
                .borrow()
                .write(PhpMixed::Array({
                    let mut m = IndexMap::new();
                    m.insert("config".into(), PhpMixed::Object(IndexMap::new()));
                    m
                }))?;
            chmod(&path, 0o600);
        }

        if !self.config_file().unwrap().borrow().exists() {
            return Err(RuntimeException::new(bformat!(
                "File \"{config_file}\" cannot be found in the current directory"
            ))
            .into());
        }

        Ok(())
    }

    /// Get the local composer.json, global config.json, or the file passed by the user
    fn get_composer_config_file(
        &self,
        input: std::rc::Rc<std::cell::RefCell<dyn InputInterface>>,
        config: &Config,
    ) -> anyhow::Result<std::path::PathBuf> {
        if input
            .borrow()
            .get_option("global")?
            .as_bool()
            .unwrap_or(false)
        {
            Ok(path_from_bytes(&config.get_home()?).join("config.json"))
        } else {
            match input
                .borrow()
                .get_option("file")?
                .as_string()
                .map(|s| path_from_bytes(s).to_path_buf())
            {
                Some(file) => Ok(file),
                None => Factory::get_composer_file(),
            }
        }
    }

    /// Get the local auth.json or global auth.json, or if the user passed in a file to use,
    /// the corresponding auth.json
    fn get_auth_config_file(
        &self,
        input: std::rc::Rc<std::cell::RefCell<dyn InputInterface>>,
        config: &Config,
    ) -> anyhow::Result<std::path::PathBuf> {
        if input
            .borrow()
            .get_option("global")?
            .as_bool()
            .unwrap_or(false)
        {
            Ok(path_from_bytes(&config.get_home()?).join("auth.json"))
        } else {
            let composer_config = self.get_composer_config_file(input, config)?;
            Ok(path_from_bytes(&bformat!(
                "{}/auth.json",
                dirname(path_to_bytes(&composer_config))
            ))
            .to_path_buf())
        }
    }
}
