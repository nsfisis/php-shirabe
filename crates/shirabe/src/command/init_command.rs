//! ref: composer/src/Composer/Command/InitCommand.php

use crate::command::base_command::base_command_initialize;
use crate::command::{BaseCommand, BaseCommandData, CompletionTrait, PackageDiscoveryTrait};
use crate::console::input::InputOption;
use crate::factory::Factory;
use crate::io::{IOInterfaceWrite, io_interface};
use crate::json::{JsonFile, JsonValidationException};
use crate::package::base_package::{self};
use crate::repository::{
    CompositeRepository, PlatformRepository, PlatformRepositoryHandle, RepositoryFactory,
};
use crate::util::{Filesystem, ProcessExecutor};
use bstr::{B, BStr, BString, ByteSlice};
use indexmap::IndexMap;
use shirabe_php_shim::{
    Catch as _, FILE_IGNORE_NEW_LINES, InvalidArgumentException, PHP_EOL, PHP_SERVER, PhpMixed,
    array_filter_map, array_flip_strings, array_intersect_key, array_map, basename, bconcat, empty,
    explode, file, file_exists, file_get_contents, file_put_contents, filter_var_email,
    get_current_user, impl_php_class, implode, is_dir, is_string, path_from_bytes, path_to_bytes,
    pcre_check_utf8_subject, php_regex, preg_is_match, preg_match, preg_match_all, preg_quote,
    preg_replace, realpath, str_replace, strpos, strtolower, trim, ucwords, unlink,
};
use shirabe_spdx_licenses::SpdxLicenses;
use shirabe_symfony_console::input::InputInterfaceByName;
use shirabe_symfony_console::{
    command::Command,
    helper::FormatBlockMessages,
    input::{ArrayInput, InputInterface, InputValue},
    output::OutputInterface,
};
use std::path::Path;

#[derive(Debug)]
pub struct InitCommand {
    base_command_data: BaseCommandData,

    git_config: std::cell::RefCell<Option<IndexMap<BString, BString>>>,
    repos: std::cell::RefCell<Option<crate::repository::RepositoryInterfaceHandle>>,
    repository_sets: std::cell::RefCell<
        IndexMap<String, std::rc::Rc<std::cell::RefCell<crate::repository::RepositorySet>>>,
    >,
}

impl_php_class!(InitCommand, r"Composer\Command\InitCommand");

impl PackageDiscoveryTrait for InitCommand {
    fn get_repos_mut(
        &self,
    ) -> std::cell::RefMut<'_, Option<crate::repository::RepositoryInterfaceHandle>> {
        self.repos.borrow_mut()
    }

    fn get_repository_sets_mut(
        &self,
    ) -> std::cell::RefMut<
        '_,
        IndexMap<String, std::rc::Rc<std::cell::RefCell<crate::repository::RepositorySet>>>,
    > {
        self.repository_sets.borrow_mut()
    }
}

impl Default for InitCommand {
    fn default() -> Self {
        Self::new()
    }
}

impl InitCommand {
    pub fn new() -> Self {
        let command = InitCommand {
            base_command_data: BaseCommandData::new(None),
            git_config: std::cell::RefCell::new(None),
            repos: std::cell::RefCell::new(None),
            repository_sets: std::cell::RefCell::new(IndexMap::new()),
        };
        command
            .configure()
            .expect("InitCommand::configure uses static, valid metadata");
        command
    }

    fn parse_author_string(
        &self,
        author: &BStr,
    ) -> anyhow::Result<IndexMap<String, Option<BString>>> {
        pcre_check_utf8_subject(
            "preg_match",
            r#"/^(?P<name>[- .,\p{L}\p{N}\p{Mn}\'’"()]+)(?:\s+<(?P<email>.+?)>)?$/u"#,
            author,
        )?;
        if let Some(m) = preg_match(
            php_regex!(r#"/^(?P<name>[- .,\p{L}\p{N}\p{Mn}\'’\"()]+)(?:\s+<(?P<email>.+?)>)?$/u"#),
            author,
        ) {
            let email = m.name("email").map(BString::from);
            if let Some(ref email) = email
                && !self.is_valid_email(email.as_bstr())
            {
                return Err(InvalidArgumentException::new(format!(
                    "Invalid email \"{}\"",
                    email.display_lossy()
                ))
                .into());
            }

            let mut result: IndexMap<String, Option<BString>> = IndexMap::new();
            result.insert(
                "name".to_string(),
                Some(trim(m.name("name").unwrap_or_default())),
            );
            result.insert("email".to_string(), email);

            return Ok(result);
        }

        Err(InvalidArgumentException::new("Invalid author string.  Must be in the formats: Jane Doe or John Smith <john@example.com>"
            .to_string())
        .into())
    }

    fn format_authors(&self, author: &BStr) -> anyhow::Result<Vec<IndexMap<BString, PhpMixed>>> {
        let parsed = self.parse_author_string(author)?;
        let mut author_map: IndexMap<BString, PhpMixed> = IndexMap::new();
        let name = parsed.get("name").cloned().unwrap_or(None);
        let email = parsed.get("email").cloned().unwrap_or(None);
        if let Some(name) = name {
            author_map.insert("name".into(), PhpMixed::string(name));
        }
        if let Some(email) = email {
            author_map.insert("email".into(), PhpMixed::string(email));
        }

        Ok(vec![author_map])
    }

    /// Extract namespace from package's vendor name.
    ///
    /// new_projects.acme-extra/package-name becomes "NewProjectsAcmeExtra\PackageName"
    pub fn namespace_from_package_name(&self, package_name: impl AsRef<[u8]>) -> Option<BString> {
        let package_name = package_name.as_ref();
        if package_name.is_empty() || strpos(package_name, "/").is_none() {
            return None;
        }

        let namespace: Vec<BString> = array_map(
            |part: &BString| {
                let part = preg_replace(php_regex!(r"/[^a-z0-9]/i"), " ", part);
                let part = ucwords(&part);
                str_replace(" ", "", &part)
            },
            &explode("/", package_name),
        );

        Some(implode("\\", &namespace))
    }

    fn get_git_config(&self) -> anyhow::Result<IndexMap<BString, BString>> {
        if self.git_config.borrow().is_some() {
            return Ok(self.git_config.borrow().clone().unwrap_or_default());
        }

        let process = ProcessExecutor::new(Some(self.get_io().clone()));

        let mut output = BString::default();
        if process.execute(
            &["git".to_string(), "config".to_string(), "-l".to_string()],
            &mut output,
            None,
        )? == 0
        {
            *self.git_config.borrow_mut() = Some(IndexMap::new());
            for m in preg_match_all(php_regex!(r"{^([^=]+)=(.*)$}m"), &output) {
                self.git_config.borrow_mut().as_mut().unwrap().insert(
                    BString::from(
                        m.get(1)
                            .expect("group 1 participates whenever the pattern matches"),
                    ),
                    BString::from(
                        m.get(2)
                            .expect("group 2 participates whenever the pattern matches"),
                    ),
                );
            }

            return Ok(self.git_config.borrow().clone().unwrap_or_default());
        }

        *self.git_config.borrow_mut() = Some(IndexMap::new());
        Ok(IndexMap::new())
    }

    /// Checks the local .gitignore file for the Composer vendor directory.
    ///
    /// Tested patterns include:
    ///  "/$vendor"
    ///  "$vendor"
    ///  "$vendor/"
    ///  "/$vendor/"
    ///  "/$vendor/*"
    ///  "$vendor/*"
    fn has_vendor_ignore(&self, ignore_file: impl AsRef<Path>, vendor: &str) -> bool {
        let ignore_file = ignore_file.as_ref();
        if !file_exists(ignore_file) {
            return false;
        }

        let pattern = format!("{{^/?{}(/\\*?)?$}}", preg_quote(vendor, None));

        let lines = file(ignore_file, FILE_IGNORE_NEW_LINES).unwrap_or_default();
        for line in &lines {
            if preg_is_match(&pattern, line) {
                return true;
            }
        }

        false
    }

    fn add_vendor_ignore(&self, ignore_file: impl AsRef<Path>, vendor: &str) {
        let ignore_file = ignore_file.as_ref();
        let mut contents: Vec<u8> = vec![];
        if file_exists(ignore_file) {
            contents = file_get_contents(ignore_file).unwrap_or_default();

            if strpos(&contents, "\n") != Some(0) {
                contents.push(b'\n');
            }
        }

        file_put_contents(ignore_file, bconcat!(contents, vendor, "\n"));
    }

    /// For testing only: invoke the private `parse_author_string`.
    pub fn __parse_author_string(
        &self,
        author: &str,
    ) -> anyhow::Result<IndexMap<String, Option<BString>>> {
        self.parse_author_string(author.into())
    }

    /// For testing only: invoke the crate-private `format_authors`.
    pub fn __format_authors(
        &self,
        author: &str,
    ) -> anyhow::Result<Vec<IndexMap<BString, PhpMixed>>> {
        self.format_authors(author.into())
    }

    /// For testing only: invoke the crate-private `get_git_config`.
    pub fn __get_git_config(&self) -> anyhow::Result<IndexMap<BString, BString>> {
        self.get_git_config()
    }

    /// For testing only: invoke the crate-private `has_vendor_ignore`.
    pub fn __has_vendor_ignore(&self, ignore_file: impl AsRef<Path>, vendor: &str) -> bool {
        self.has_vendor_ignore(ignore_file, vendor)
    }

    /// For testing only: invoke the crate-private `add_vendor_ignore`.
    pub fn __add_vendor_ignore(&self, ignore_file: impl AsRef<Path>, vendor: &str) {
        self.add_vendor_ignore(ignore_file, vendor)
    }

    fn is_valid_email(&self, email: &BStr) -> bool {
        filter_var_email(email)
    }

    async fn update_dependencies(
        &self,
        output: std::rc::Rc<std::cell::RefCell<dyn OutputInterface>>,
    ) {
        let result: anyhow::Result<i64> = async {
            let application = self
                .get_application()
                .expect("a Composer command's application is always set");
            let update_command = application.borrow_mut().find(bstr::BStr::new("update"))?;
            self.reset_composer()?;
            let input: std::rc::Rc<std::cell::RefCell<dyn InputInterface>> =
                std::rc::Rc::new(std::cell::RefCell::new(ArrayInput::new(vec![], None)?));
            let command = update_command.borrow();
            command.run(input, output).await
        }
        .await;

        if result.is_err() {
            self.get_io().borrow().write_error(
                "Could not update dependencies. Run `shirabe update` to see more information.",
            );
        }
    }

    async fn run_dump_autoload_command(
        &self,
        output: std::rc::Rc<std::cell::RefCell<dyn OutputInterface>>,
    ) {
        let result: anyhow::Result<i64> = async {
            let application = self
                .get_application()
                .expect("a Composer command's application is always set");
            let command = application
                .borrow_mut()
                .find(bstr::BStr::new("dump-autoload"))?;
            self.reset_composer()?;
            let input: std::rc::Rc<std::cell::RefCell<dyn InputInterface>> =
                std::rc::Rc::new(std::cell::RefCell::new(ArrayInput::new(vec![], None)?));
            let command = command.borrow();
            command.run(input, output).await
        }
        .await;

        if result.is_err() {
            self.get_io()
                .borrow()
                .write_error("Could not run dump-autoload.");
        }
    }

    fn has_dependencies(&self, options: &IndexMap<BString, PhpMixed>) -> bool {
        let requires = options.get(B("require")).cloned().unwrap_or(PhpMixed::Null);
        let requires_arr_empty = match &requires {
            PhpMixed::Array(m) => m.is_empty(),
            PhpMixed::List(l) => l.is_empty(),
            PhpMixed::Null => true,
            _ => false,
        };
        let dev_requires = options.get(B("require-dev")).cloned();
        let dev_requires_arr_empty = match &dev_requires {
            Some(PhpMixed::Array(m)) => m.is_empty(),
            Some(PhpMixed::List(l)) => l.is_empty(),
            Some(PhpMixed::Null) | None => true,
            _ => false,
        };

        !requires_arr_empty || !dev_requires_arr_empty
    }

    fn sanitize_package_name_component(&self, name: &BStr) -> anyhow::Result<BString> {
        let name = preg_replace(
            php_regex!(r"{(?:([a-z])([A-Z])|([A-Z])([A-Z][a-z]))}"),
            "$1$3-$2$4",
            name,
        );
        let name = strtolower(&name);
        pcre_check_utf8_subject("preg_replace", r"{^[_.-]+|[_.-]+$|[^a-z0-9_.-]}u", &name)?;
        let name = preg_replace(php_regex!(r"{^[_.-]+|[_.-]+$|[^a-z0-9_.-]}u"), "", &name);

        Ok(preg_replace(php_regex!(r"{([_.-]){2,}}u"), "$1", &name))
    }

    fn get_default_package_name(&self) -> anyhow::Result<BString> {
        let git = self.get_git_config()?;
        let cwd = realpath(".").unwrap_or_default();
        let name = basename(path_to_bytes(&cwd));
        let name = self.sanitize_package_name_component(name.as_bstr())?;

        let mut vendor = name.clone();
        let composer_default_vendor = server_value("COMPOSER_DEFAULT_VENDOR");
        let server_username = server_value("USERNAME");
        let server_user = server_value("USER");
        if !empty(
            &composer_default_vendor
                .clone()
                .map(PhpMixed::string)
                .unwrap_or(PhpMixed::Null),
        ) {
            vendor = composer_default_vendor.unwrap_or_default();
        } else if git.contains_key(B("github.user")) {
            vendor = git.get(B("github.user")).cloned().unwrap_or_default();
        } else if !empty(
            &server_username
                .clone()
                .map(PhpMixed::string)
                .unwrap_or(PhpMixed::Null),
        ) {
            vendor = server_username.unwrap_or_default();
        } else if !empty(
            &server_user
                .clone()
                .map(PhpMixed::string)
                .unwrap_or(PhpMixed::Null),
        ) {
            vendor = server_user.unwrap_or_default();
        } else if !get_current_user().is_empty() {
            vendor = get_current_user().into();
        }

        let vendor = self.sanitize_package_name_component(vendor.as_bstr())?;

        Ok(bconcat!(vendor, "/", name))
    }

    fn get_default_author(&self) -> anyhow::Result<Option<BString>> {
        let git = self.get_git_config()?;

        let mut author_name: Option<BString> = None;
        let composer_default_author = server_value("COMPOSER_DEFAULT_AUTHOR");
        if !empty(
            &composer_default_author
                .clone()
                .map(PhpMixed::string)
                .unwrap_or(PhpMixed::Null),
        ) {
            author_name = composer_default_author;
        } else if git.contains_key(B("user.name")) {
            author_name = git.get(B("user.name")).cloned();
        }

        let mut author_email: Option<BString> = None;
        let composer_default_email = server_value("COMPOSER_DEFAULT_EMAIL");
        if !empty(
            &composer_default_email
                .clone()
                .map(PhpMixed::string)
                .unwrap_or(PhpMixed::Null),
        ) {
            author_email = composer_default_email;
        } else if git.contains_key(B("user.email")) {
            author_email = git.get(B("user.email")).cloned();
        }

        if let (Some(name), Some(email)) = (author_name, author_email) {
            return Ok(Some(bconcat!(name, " <", email, ">")));
        }

        Ok(None)
    }
}

/// `$_SERVER[$name]`, holding the bytes of the environment.
fn server_value(name: &str) -> Option<BString> {
    PHP_SERVER
        .lock()
        .unwrap()
        .get(name)
        .map(|value| BString::from(value.into_encoded_bytes()))
}

#[async_trait::async_trait(?Send)]
impl Command for InitCommand {
    fn configure(&self) -> anyhow::Result<()> {
        self.set_name("init".into())?;
        self.set_description("Creates a basic composer.json file in current directory".into());
        self.set_definition(&[
                InputOption::new("name", None, Some(InputOption::VALUE_REQUIRED), "Name of the package", None).unwrap().into(),
        InputOption::new("description", None, Some(InputOption::VALUE_REQUIRED), "Description of package", None).unwrap().into(),
        InputOption::new("author", None, Some(InputOption::VALUE_REQUIRED), "Author name of package", None).unwrap().into(),
        InputOption::new("type", None, Some(InputOption::VALUE_REQUIRED), "Type of package (e.g. library, project, metapackage, composer-plugin)", None).unwrap().into(),
        InputOption::new("homepage", None, Some(InputOption::VALUE_REQUIRED), "Homepage of package", None).unwrap().into(),
        InputOption::new6("require", None, Some(InputOption::VALUE_IS_ARRAY | InputOption::VALUE_REQUIRED), "Package to require with a version constraint, e.g. foo/bar:1.0.0 or foo/bar=1.0.0 or \"foo/bar 1.0.0\"", None, self.suggest_available_package_incl_platform()).unwrap().into(),
        InputOption::new6("require-dev", None, Some(InputOption::VALUE_IS_ARRAY | InputOption::VALUE_REQUIRED), "Package to require for development with a version constraint, e.g. foo/bar:1.0.0 or foo/bar=1.0.0 or \"foo/bar 1.0.0\"", None, self.suggest_available_package_incl_platform()).unwrap().into(),
        InputOption::new("stability", Some("s".into()), Some(InputOption::VALUE_REQUIRED), format!("Minimum stability (empty or one of: {})", implode(", ", &base_package::STABILITIES.keys().map(|k| k.to_string()).collect::<Vec<_>>()).display_lossy()), None).unwrap().into(),
        InputOption::new("license", Some("l".into()), Some(InputOption::VALUE_REQUIRED), "License of package", None).unwrap().into(),
        InputOption::new("repository", None, Some(InputOption::VALUE_REQUIRED | InputOption::VALUE_IS_ARRAY), "Add custom repositories, either by URL or using JSON arrays", None).unwrap().into(),
        InputOption::new("autoload", Some("a".into()), Some(InputOption::VALUE_REQUIRED), "Add PSR-4 autoload mapping. Maps your package's namespace to the provided directory. (Expects a relative path, e.g. src/)", None).unwrap().into(),
            ]);
        self.set_help(
            "The <info>init</info> command creates a basic composer.json file\n\
                in the current directory.\n\
                \n\
                <info>shirabe init</info>\n\
                \n\
                Read more at https://getcomposer.org/doc/03-cli.md#init"
                .into(),
        );
        Ok(())
    }

    async fn execute(
        &self,
        input: std::rc::Rc<std::cell::RefCell<dyn InputInterface>>,
        output: std::rc::Rc<std::cell::RefCell<dyn OutputInterface>>,
    ) -> anyhow::Result<i64> {
        let io = self.get_io();

        let allowlist: Vec<String> = vec![
            "name".to_string(),
            "description".to_string(),
            "author".to_string(),
            "type".to_string(),
            "homepage".to_string(),
            "require".to_string(),
            "require-dev".to_string(),
            "stability".to_string(),
            "license".to_string(),
            "autoload".to_string(),
        ];
        let options: IndexMap<BString, PhpMixed> = input
            .borrow()
            .get_options()
            .into_iter()
            .map(|(key, value)| (key, value.to_php_mixed()))
            .collect();
        let filtered_input: IndexMap<BString, PhpMixed> =
            array_intersect_key(&options, &array_flip_strings(&allowlist))
                .into_iter()
                .collect();
        let mut options = array_filter_map(&filtered_input, |val: &PhpMixed| {
            !matches!(val, PhpMixed::Null) && !matches!(val, PhpMixed::List(l) if l.is_empty())
        });

        if options.contains_key(B("name"))
            && !preg_is_match(
                php_regex!(r"{^[a-z0-9]([_.-]?[a-z0-9]+)*\/[a-z0-9](([_.]|-{1,2})?[a-z0-9]+)*$}D"),
                options
                    .get(B("name"))
                    .and_then(|v| v.as_string())
                    .unwrap_or_default(),
            )
        {
            return Err(InvalidArgumentException::new(format!(
                "The package name {} is invalid, it should be lowercase and have a vendor name, a forward slash, and a package name, matching: [a-z0-9_.-]+/[a-z0-9_.-]+",
                options.get(B("name")).and_then(|v| v.as_string()).unwrap_or_default().display_lossy()
            ))
            .into());
        }

        if options.contains_key(B("author")) {
            let authors = self.format_authors(
                options
                    .get(B("author"))
                    .and_then(|v| v.as_string())
                    .unwrap_or_default(),
            )?;
            options.insert(
                "authors".into(),
                PhpMixed::List(
                    authors
                        .into_iter()
                        .map(|m| PhpMixed::Array(m.into_iter().collect()))
                        .collect(),
                ),
            );
            options.shift_remove(B("author"));
        }

        let repositories: Vec<bstr::BString> = input
            .borrow()
            .get_option("repository")?
            .as_array()
            .map(<[bstr::BString]>::to_vec)
            .unwrap_or_default();
        if (repositories.len() as i64) > 0 {
            let config = std::rc::Rc::new(std::cell::RefCell::new(Factory::create_config(
                Some(io.clone()),
                None,
            )?));
            for repo in &repositories {
                let repo_config =
                    RepositoryFactory::config_from_string(io.clone(), &config, repo, true)?;
                let entry = options
                    .entry("repositories".into())
                    .or_insert_with(|| PhpMixed::List(vec![]));
                if let PhpMixed::List(list) = entry {
                    list.push(PhpMixed::Array(repo_config.into_iter().collect()));
                }
            }
        }

        if options.contains_key(B("stability")) {
            let stab = options
                .shift_remove(B("stability"))
                .unwrap_or(PhpMixed::Null);
            options.insert("minimum-stability".into(), stab);
        }

        let require_value = if options.contains_key(B("require")) {
            let req_list: Vec<BString> = options
                .get(B("require"))
                .and_then(|v| v.as_list())
                .map(|l| {
                    l.iter()
                        .filter_map(|v| v.as_string().map(BString::from))
                        .collect()
                })
                .unwrap_or_default();
            let formatted = self.format_requirements(req_list)?;
            if formatted.is_empty() {
                // PHP: new \stdClass — empty JSON object
                PhpMixed::Object(IndexMap::new())
            } else {
                PhpMixed::Array(
                    formatted
                        .into_iter()
                        .map(|(k, v)| (k, PhpMixed::string(v)))
                        .collect(),
                )
            }
        } else {
            // PHP: new \stdClass
            PhpMixed::Object(IndexMap::new())
        };
        options.insert("require".into(), require_value);

        if options.contains_key(B("require-dev")) {
            let req_list: Vec<BString> = options
                .get(B("require-dev"))
                .and_then(|v| v.as_list())
                .map(|l| {
                    l.iter()
                        .filter_map(|v| v.as_string().map(BString::from))
                        .collect()
                })
                .unwrap_or_default();
            let formatted = self.format_requirements(req_list)?;
            let value = if formatted.is_empty() {
                PhpMixed::Object(IndexMap::new())
            } else {
                PhpMixed::Array(
                    formatted
                        .into_iter()
                        .map(|(k, v)| (k, PhpMixed::string(v)))
                        .collect(),
                )
            };
            options.insert("require-dev".into(), value);
        }

        // --autoload - create autoload object
        let mut autoload_path: Option<BString> = None;
        if options.contains_key(B("autoload")) {
            let ap = BString::from(
                options
                    .get(B("autoload"))
                    .and_then(|v| v.as_string())
                    .unwrap_or_default(),
            );
            autoload_path = Some(ap.clone());
            let name = input.borrow().get_option("name")?;
            let namespace = self
                .namespace_from_package_name(name.as_string().unwrap_or_default())
                .unwrap_or_default();
            let mut psr4 = IndexMap::new();
            psr4.insert(bconcat!(namespace, "\\"), PhpMixed::string(ap));
            let mut autoload_obj = IndexMap::new();
            autoload_obj.insert("psr-4".into(), PhpMixed::Array(psr4));
            options.insert("autoload".into(), PhpMixed::Array(autoload_obj));
        }

        let file_obj = JsonFile::new(Factory::get_composer_file()?, None, None)?;
        let options_for_encode: IndexMap<BString, PhpMixed> = options.clone().into_iter().collect();
        let json = JsonFile::encode(&PhpMixed::Array(options_for_encode.clone()))?;

        if input.borrow().is_interactive() {
            io.write_error3(format!("\n{}\n", json), true, io_interface::NORMAL);
            if !io.ask_confirmation("Do you confirm generation [<comment>yes</comment>]? ", true)? {
                io.write_error3("<error>Command aborted</error>", true, io_interface::NORMAL);

                return Ok(1);
            }
        } else {
            io.write_error3(
                format!("Writing {}", file_obj.get_path().display()),
                true,
                io_interface::NORMAL,
            );
        }

        file_obj.write(PhpMixed::Array(options_for_encode))?;
        let validate_result = file_obj.validate_schema(JsonFile::LAX_SCHEMA, None);
        if let Err(e) = validate_result {
            // try to downcast to JsonValidationException
            if let Some(json_err) = e.catch::<JsonValidationException>() {
                io.write_error3(
                    "<error>Schema validation error, aborting</error>",
                    true,
                    io_interface::NORMAL,
                );
                let errors = format!(
                    " - {}",
                    (implode(format!("{} - ", PHP_EOL), json_err.get_errors())).display_lossy()
                );
                io.write_error3(
                    format!("{}:{}{}", json_err.get_message(), PHP_EOL, errors),
                    true,
                    io_interface::NORMAL,
                );
                let _ = unlink(file_obj.get_path());

                return Ok(1);
            }
            return Err(e);
        }

        // --autoload - Create src folder
        if let Some(ref ap) = autoload_path {
            let mut filesystem = Filesystem::new(None);
            filesystem.ensure_directory_exists(path_from_bytes(ap));

            // dump-autoload only for projects without added dependencies.
            if !self.has_dependencies(&options) {
                self.run_dump_autoload_command(output.clone()).await;
            }
        }

        if input.borrow().is_interactive() && is_dir(".git") {
            let ignore_file = realpath(".gitignore").unwrap_or_else(|| {
                path_from_bytes(&bconcat!(realpath(".").unwrap_or_default(), "/.gitignore"))
                    .to_path_buf()
            });

            if !self.has_vendor_ignore(&ignore_file, "vendor") {
                let question = "Would you like the <info>vendor</info> directory added to your <info>.gitignore</info> [<comment>yes</comment>]? ".to_string();

                if io.ask_confirmation(question, true)? {
                    self.add_vendor_ignore(&ignore_file, "/vendor/");
                }
            }
        }

        let question =
            "Would you like to install dependencies now [<comment>yes</comment>]? ".to_string();
        if input.borrow().is_interactive()
            && self.has_dependencies(&options)
            && io.ask_confirmation(question, true)?
        {
            self.update_dependencies(output).await;
        }

        // --autoload - Show post-install configuration info
        if autoload_path.is_some() {
            let name = input.borrow().get_option("name")?;
            let namespace = self
                .namespace_from_package_name(name.as_string().unwrap_or_default())
                .unwrap_or_default();

            io.write_error3(
                bconcat!(
                    "PSR-4 autoloading configured. Use \"<comment>namespace ",
                    namespace,
                    ";</comment>\" in ",
                    autoload_path.clone().unwrap_or_default()
                ),
                true,
                io_interface::NORMAL,
            );
            io.write_error3("Include the Composer autoloader with: <comment>require 'vendor/autoload.php';</comment>", true, io_interface::NORMAL);
        }

        Ok(0)
    }

    fn initialize(
        &self,
        input: std::rc::Rc<std::cell::RefCell<dyn InputInterface>>,
        output: std::rc::Rc<std::cell::RefCell<dyn OutputInterface>>,
    ) -> anyhow::Result<()> {
        base_command_initialize(self, input.clone(), output.clone())?;

        if !input.borrow().is_interactive() {
            if input.borrow().get_option("name")?.is_null() {
                let name = self.get_default_package_name()?;
                input
                    .borrow_mut()
                    .set_option("name", InputValue::from(name))
                    .expect("name option is defined");
            }

            if input.borrow().get_option("author")?.is_null() {
                let author = self.get_default_author()?;
                input
                    .borrow_mut()
                    .set_option("author", InputValue::from(author))
                    .expect("author option is defined");
            }
        }

        Ok(())
    }

    fn interact(
        &self,
        input: std::rc::Rc<std::cell::RefCell<dyn InputInterface>>,
        output: std::rc::Rc<std::cell::RefCell<dyn OutputInterface>>,
    ) {
        let _ = (|| -> anyhow::Result<()> {
            let io = self.get_io();
            // @var FormatterHelper $formatter — PHP: $this->getHelperSet()->get('formatter')
            let formatter = self
                .get_helper_set()
                .expect("the init command is registered on an application before interact() runs")
                .borrow()
                .get_formatter();

            // initialize repos if configured
            let repositories: Vec<bstr::BString> = input
                .borrow()
                .get_option("repository")?
                .as_array()
                .map(<[bstr::BString]>::to_vec)
                .unwrap_or_default();
            if (repositories.len() as i64) > 0 {
                let config = std::rc::Rc::new(std::cell::RefCell::new(Factory::create_config(
                    Some(io.clone()),
                    None,
                )?));
                io.borrow_mut()
                    .load_configuration(&mut config.borrow_mut())?;
                let mut repo_manager =
                    RepositoryFactory::manager(io.clone(), &config, None, None, None)?;

                let mut repos: Vec<crate::repository::RepositoryInterfaceHandle> =
                    vec![crate::repository::RepositoryInterfaceHandle::new(
                        PlatformRepository::new(vec![], IndexMap::new(), None, None)?,
                    )];
                let mut create_default_packagist_repo = true;
                for repo in &repositories {
                    let repo_config =
                        RepositoryFactory::config_from_string(io.clone(), &config, repo, true)?;
                    let is_packagist_false = repo_config
                        .get(B("packagist"))
                        .map(|v| v.as_bool() == Some(false))
                        .unwrap_or(false)
                        && repo_config.len() == 1;
                    let is_packagist_org_false = repo_config
                        .get(B("packagist.org"))
                        .map(|v| v.as_bool() == Some(false))
                        .unwrap_or(false)
                        && repo_config.len() == 1;
                    if is_packagist_false || is_packagist_org_false {
                        create_default_packagist_repo = false;
                        continue;
                    }
                    repos.push(RepositoryFactory::create_repo(
                        io.clone(),
                        &config,
                        repo_config,
                        Some(&mut repo_manager),
                    )?);
                }

                if create_default_packagist_repo {
                    let mut default_config: IndexMap<BString, PhpMixed> = IndexMap::new();
                    default_config.insert("type".into(), PhpMixed::string("composer".to_string()));
                    default_config.insert(
                        "url".into(),
                        PhpMixed::string("https://repo.packagist.org".to_string()),
                    );
                    repos.push(RepositoryFactory::create_repo(
                        io.clone(),
                        &config,
                        default_config,
                        Some(&mut repo_manager),
                    )?);
                }

                *self.get_repos_mut() = Some(crate::repository::RepositoryInterfaceHandle::new(
                    CompositeRepository::new(repos),
                ));
                // unset($repos, $config, $repositories);
            }

            io.write_error3(
                bconcat!(
                    "\n",
                    formatter.borrow().format_block(
                        FormatBlockMessages::String(
                            "Welcome to the Shirabe config generator".into(),
                        ),
                        "bg=blue;fg=white".into(),
                        true,
                    ),
                    "\n"
                ),
                true,
                io_interface::NORMAL,
            );

            // namespace
            io.write_error3(
                "\nThis command will guide you through creating your composer.json config.\n",
                true,
                io_interface::NORMAL,
            );

            let name_option = input
                .borrow()
                .get_option("name")?
                .as_string()
                .map(BString::from);
            let mut name = match name_option {
                Some(name) => name,
                None => self.get_default_package_name()?,
            };

            let name_default = name.clone();
            let name_for_validate = name.clone();
            name = BString::from(io
            .ask_and_validate(
                format!(
                    "Package name (<vendor>/<name>) [<comment>{}</comment>]: ",
                    name_default.display_lossy()
                ),
                Box::new(move |value: PhpMixed| -> anyhow::Result<PhpMixed> {
                    if value.is_null() {
                        return Ok(PhpMixed::string(name_for_validate.clone()));
                    }

                    if !preg_is_match(
                        php_regex!(
                            r"{^[a-z0-9]([_.-]?[a-z0-9]+)*\/[a-z0-9](([_.]|-{1,2})?[a-z0-9]+)*$}D"
                        ),
                        value.as_string().unwrap_or_default(),
                    ) {
                        return Err(InvalidArgumentException::new(format!(
                            "The package name {} is invalid, it should be lowercase and have a vendor name, a forward slash, and a package name, matching: [a-z0-9_.-]+/[a-z0-9_.-]+",
                            value.as_string().unwrap_or_default().display_lossy()
                        ))
                        .into());
                    }

                    Ok(value)
                }),
                None,
                PhpMixed::string(name.clone()),
            )?
            .as_string()
            .unwrap_or_default());
            input
                .borrow_mut()
                .set_option("name", InputValue::String(name));

            let description = input
                .borrow()
                .get_option("description")?
                .as_string()
                .map(BString::from);
            let description_default = description.clone();
            let description = io.ask(
                format!(
                    "Description [<comment>{}</comment>]: ",
                    description.unwrap_or_default().display_lossy()
                ),
                description_default
                    .map(PhpMixed::string)
                    .unwrap_or(PhpMixed::Null),
            )?;
            input
                .borrow_mut()
                .set_option("description", InputValue::from_php_mixed(&description));

            let author_option = input
                .borrow()
                .get_option("author")?
                .as_string()
                .map(BString::from);
            let author = match author_option {
                Some(author) => author,
                None => self.get_default_author()?.unwrap_or_default(),
            };

            let author_for_validate = author.clone();
            let author_default = author.clone();
            // PHP: $this->parseAuthorString is called inside a closure. We approximate by binding.
            let author_value = io.ask_and_validate(
                format!(
                    "Author [{} n to skip]: ",
                    if is_string(&PhpMixed::string(author.clone())) {
                        format!("<comment>{}</comment>, ", author.display_lossy())
                    } else {
                        String::new()
                    }
                ),
                // PHP: function ($value) use ($self, $author) { ... $author = $self->parseAuthorString($value); ... }
                // The validator is a `'static` closure and cannot borrow `&self`, but
                // parse_author_string is stateless (it only delegates to the stateless
                // is_valid_email), so a fresh InitCommand stands in for `$self` here.
                Box::new(move |value: PhpMixed| -> anyhow::Result<PhpMixed> {
                    let value_str = value.as_string();
                    if value_str == Some("n".into()) || value_str == Some("no".into()) {
                        return Ok(PhpMixed::Null);
                    }
                    // PHP: $value = $value ?: $author
                    let value = match value_str {
                        Some(s) if !s.is_empty() => s,
                        _ => author_for_validate.as_bstr(),
                    };
                    let parsed = InitCommand::new().parse_author_string(value)?;
                    let name = parsed.get("name").cloned().flatten().unwrap_or_default();
                    match parsed.get("email").cloned().flatten() {
                        None => Ok(PhpMixed::string(name)),
                        Some(email) => Ok(PhpMixed::string(bconcat!(name, " <", email, ">"))),
                    }
                }),
                None,
                PhpMixed::string(author_default),
            )?;
            input
                .borrow_mut()
                .set_option("author", InputValue::from_php_mixed(&author_value));

            let minimum_stability = input
                .borrow()
                .get_option("stability")?
                .as_string()
                .map(BString::from);
            let minimum_stability_default = minimum_stability.clone();
            let minimum_stability_for_validate = minimum_stability.clone();
            let minimum_stability_value = io.ask_and_validate(
                format!(
                    "Minimum Stability [<comment>{}</comment>]: ",
                    minimum_stability.unwrap_or_default().display_lossy()
                ),
                Box::new(move |value: PhpMixed| -> anyhow::Result<PhpMixed> {
                    if value.is_null() {
                        return Ok(minimum_stability_for_validate
                            .clone()
                            .map(PhpMixed::string)
                            .unwrap_or(PhpMixed::Null));
                    }

                    if base_package::stability_priority(value.as_string().unwrap_or_default())
                        .is_none()
                    {
                        return Err(InvalidArgumentException::new(format!(
                            "Invalid minimum stability \"{}\". Must be empty or one of: {}",
                            value.as_string().unwrap_or_default().display_lossy(),
                            implode(
                                ", ",
                                &base_package::STABILITIES
                                    .keys()
                                    .map(|k| k.to_string())
                                    .collect::<Vec<_>>()
                            )
                            .display_lossy()
                        ))
                        .into());
                    }

                    Ok(value)
                }),
                None,
                minimum_stability_default
                    .clone()
                    .map(PhpMixed::string)
                    .unwrap_or(PhpMixed::Null),
            )?;
            input.borrow_mut().set_option(
                "stability",
                InputValue::from_php_mixed(&minimum_stability_value),
            );

            let type_val = input.borrow().get_option("type")?;
            let type_str = BString::from(type_val.as_string().unwrap_or_default());
            let mut type_value = io.ask(
            format!(
                "Package Type (e.g. library, project, metapackage, composer-plugin) [<comment>{}</comment>]: ",
                type_str.display_lossy()
            ),
            type_val.into(),
        )?;
            if type_value.as_string() == Some("".into())
                || matches!(type_value, PhpMixed::Bool(false))
            {
                type_value = PhpMixed::Null;
            }
            input
                .borrow_mut()
                .set_option("type", InputValue::from_php_mixed(&type_value));

            let mut license = input
                .borrow()
                .get_option("license")?
                .as_string()
                .map(BString::from);
            if license.is_none() {
                let default_license = server_value("COMPOSER_DEFAULT_LICENSE");
                if !empty(
                    &default_license
                        .clone()
                        .map(PhpMixed::string)
                        .unwrap_or(PhpMixed::Null),
                ) {
                    license = default_license;
                }
            }

            let license = io.ask(
                format!(
                    "License [<comment>{}</comment>]: ",
                    license.clone().unwrap_or_default().display_lossy()
                ),
                license.map(PhpMixed::string).unwrap_or(PhpMixed::Null),
            )?;
            let spdx = SpdxLicenses::new();
            if !license.is_null()
                && !spdx.validate(license.as_string().unwrap_or_default())
                && license.as_string() != Some("proprietary".into())
            {
                return Err(InvalidArgumentException::new(format!(
                    "Invalid license provided: {}. Only SPDX license identifiers (https://spdx.org/licenses/) or \"proprietary\" are accepted.",
                    license.as_string().unwrap_or_default().display_lossy()
                ))
            .into());
            }
            input
                .borrow_mut()
                .set_option("license", InputValue::from_php_mixed(&license));

            io.write_error3("\nDefine your dependencies.\n", true, io_interface::NORMAL);

            // prepare to resolve dependencies
            let repos = self.get_repos();
            let preferred_stability =
                if let Some(s) = minimum_stability_default.filter(|s: &BString| !s.is_empty()) {
                    s
                } else {
                    BString::from("stable")
                };
            let platform_repo: Option<PlatformRepositoryHandle> =
                if repos.is::<CompositeRepository>() {
                    let borrowed = repos.borrow();
                    let composite = borrowed
                        .as_any()
                        .downcast_ref::<CompositeRepository>()
                        .expect("is::<CompositeRepository>() checked above");
                    composite
                        .get_repositories()
                        .iter()
                        .find(|candidate| candidate.is::<PlatformRepository>())
                        .and_then(|candidate| candidate.as_platform_repository())
                } else {
                    None
                };

            let question = "Would you like to define your dependencies (require) interactively [<comment>yes</comment>]? ".to_string();
            let require: Vec<bstr::BString> = input
                .borrow()
                .get_option("require")?
                .as_array()
                .map(<[bstr::BString]>::to_vec)
                .unwrap_or_default();
            let requirements =
                if (require.len() as i64) > 0 || io.ask_confirmation(question, true)? {
                    self.determine_requirements(
                        input.clone(),
                        output.clone(),
                        require,
                        platform_repo.as_ref(),
                        bstr::BStr::new(&preferred_stability),
                        false,
                        false,
                    )?
                } else {
                    vec![]
                };
            input
                .borrow_mut()
                .set_option("require", InputValue::from(requirements));

            let question = "Would you like to define your dev dependencies (require-dev) interactively [<comment>yes</comment>]? ".to_string();
            let require_dev: Vec<bstr::BString> = input
                .borrow()
                .get_option("require-dev")?
                .as_array()
                .map(<[bstr::BString]>::to_vec)
                .unwrap_or_default();
            let dev_requirements =
                if (require_dev.len() as i64) > 0 || io.ask_confirmation(question, true)? {
                    self.determine_requirements(
                        input.clone(),
                        output.clone(),
                        require_dev,
                        platform_repo.as_ref(),
                        bstr::BStr::new(&preferred_stability),
                        false,
                        false,
                    )?
                } else {
                    vec![]
                };
            input
                .borrow_mut()
                .set_option("require-dev", InputValue::from(dev_requirements));

            // --autoload - input and validation
            let autoload = input
                .borrow()
                .get_option("autoload")?
                .as_string()
                .map(BString::from)
                .filter(|s: &BString| !s.is_empty())
                .unwrap_or_else(|| "src/".into());
            let name_str = input.borrow().get_option("name")?;
            let namespace = self
                .namespace_from_package_name(name_str.as_string().unwrap_or_default())
                .unwrap_or_default();
            let autoload_for_validate = autoload.clone();
            let autoload_default = autoload.clone();
            let autoload_value = io.ask_and_validate(
            format!(
                "Add PSR-4 autoload mapping? Maps namespace \"{}\" to the entered relative path. [<comment>{}</comment>, n to skip]: ",
                namespace.display_lossy(),
                autoload.display_lossy()
            ),
            Box::new(move |value: PhpMixed| -> anyhow::Result<PhpMixed> {
                if value.is_null() {
                    return Ok(PhpMixed::string(autoload_for_validate.clone()));
                }

                let value_str = BString::from(value.as_string().unwrap_or_default());
                if value_str == "n" || value_str == "no" {
                    return Ok(PhpMixed::Null);
                }

                let value_or_default = if value_str.is_empty() {
                    autoload_for_validate.clone()
                } else {
                    value_str
                };

                if !preg_is_match(php_regex!(r"{^[^/][A-Za-z0-9\-_/]+/$}"), &value_or_default)
                {
                    return Err(InvalidArgumentException::new(format!(
                        "The src folder name \"{}\" is invalid. Please add a relative path with tailing forward slash. [A-Za-z0-9_-/]+/",
                        value_or_default.display_lossy(),
                    ))
                    .into());
                }

                Ok(PhpMixed::string(value_or_default))
            }),
            None,
            PhpMixed::string(autoload_default),
        )?;
            input
                .borrow_mut()
                .set_option("autoload", InputValue::from_php_mixed(&autoload_value));

            Ok(())
        })();
    }

    fn complete(
        &self,
        input: &shirabe_symfony_console::completion::CompletionInput,
        suggestions: &mut shirabe_symfony_console::completion::CompletionSuggestions,
    ) -> anyhow::Result<()> {
        crate::command::base_command::base_command_complete(self, input, suggestions)
    }

    shirabe_symfony_console::delegate_command_trait_impls_to_inner!(base_command_data);
}

impl BaseCommand for InitCommand {
    fn base_command_data(&self) -> &crate::command::BaseCommandData {
        &self.base_command_data
    }

    crate::delegate_base_command_trait_impls_to_inner!(base_command_data);
}
