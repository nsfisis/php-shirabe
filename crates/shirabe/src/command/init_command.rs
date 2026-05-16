//! ref: composer/src/Composer/Command/InitCommand.php

use anyhow::Result;
use indexmap::IndexMap;
use shirabe_external_packages::composer::pcre::preg::Preg;
use shirabe_external_packages::composer::spdx_licenses::spdx_licenses::SpdxLicenses;
use shirabe_external_packages::symfony::component::console::helper::formatter_helper::FormatterHelper;
use shirabe_external_packages::symfony::component::console::input::array_input::ArrayInput;
use shirabe_external_packages::symfony::component::console::input::input_interface::InputInterface;
use shirabe_external_packages::symfony::component::console::output::output_interface::OutputInterface;
use shirabe_php_shim::{
    FILE_IGNORE_NEW_LINES, FILTER_VALIDATE_EMAIL, InvalidArgumentException, PHP_EOL, PhpMixed,
    array_filter, array_flip, array_intersect_key, array_keys, array_map, basename, empty, explode,
    file, file_exists, file_get_contents, file_put_contents, function_exists, get_current_user,
    implode, is_dir, is_string, preg_quote, realpath, server_get, sprintf, str_replace, strpos,
    strtolower, trim, ucwords,
};

use crate::command::base_command::BaseCommand;
use crate::command::completion_trait::CompletionTrait;
use crate::command::package_discovery_trait::PackageDiscoveryTrait;
use crate::console::input::input_option::InputOption;
use crate::factory::Factory;
use crate::io::io_interface::IOInterface;
use crate::json::json_file::JsonFile;
use crate::json::json_validation_exception::JsonValidationException;
use crate::package::base_package::BasePackage;
use crate::repository::composite_repository::CompositeRepository;
use crate::repository::platform_repository::PlatformRepository;
use crate::repository::repository_factory::RepositoryFactory;
use crate::util::filesystem::Filesystem;
use crate::util::process_executor::ProcessExecutor;
use crate::util::silencer::Silencer;

#[derive(Debug)]
pub struct InitCommand {
    inner: BaseCommand,
    /// @var array<string, string>
    git_config: Option<IndexMap<String, String>>,
}

impl CompletionTrait for InitCommand {}
impl PackageDiscoveryTrait for InitCommand {}

impl InitCommand {
    pub fn configure(&mut self) {
        let suggest_available_package_incl_platform =
            self.suggest_available_package_incl_platform();
        let suggest_available_package_incl_platform2 =
            self.suggest_available_package_incl_platform();
        self.inner
            .set_name("init")
            .set_description("Creates a basic composer.json file in current directory")
            .set_definition(vec![
                InputOption::new("name", None, Some(InputOption::VALUE_REQUIRED), "Name of the package", None, vec![]),
                InputOption::new("description", None, Some(InputOption::VALUE_REQUIRED), "Description of package", None, vec![]),
                InputOption::new("author", None, Some(InputOption::VALUE_REQUIRED), "Author name of package", None, vec![]),
                InputOption::new("type", None, Some(InputOption::VALUE_REQUIRED), "Type of package (e.g. library, project, metapackage, composer-plugin)", None, vec![]),
                InputOption::new("homepage", None, Some(InputOption::VALUE_REQUIRED), "Homepage of package", None, vec![]),
                InputOption::new("require", None, Some(InputOption::VALUE_IS_ARRAY | InputOption::VALUE_REQUIRED), "Package to require with a version constraint, e.g. foo/bar:1.0.0 or foo/bar=1.0.0 or \"foo/bar 1.0.0\"", None, suggest_available_package_incl_platform),
                InputOption::new("require-dev", None, Some(InputOption::VALUE_IS_ARRAY | InputOption::VALUE_REQUIRED), "Package to require for development with a version constraint, e.g. foo/bar:1.0.0 or foo/bar=1.0.0 or \"foo/bar 1.0.0\"", None, suggest_available_package_incl_platform2),
                InputOption::new("stability", Some(PhpMixed::String("s".to_string())), Some(InputOption::VALUE_REQUIRED), &format!("Minimum stability (empty or one of: {})", implode(", ", &array_keys(&BasePackage::stabilities()))), None, vec![]),
                InputOption::new("license", Some(PhpMixed::String("l".to_string())), Some(InputOption::VALUE_REQUIRED), "License of package", None, vec![]),
                InputOption::new("repository", None, Some(InputOption::VALUE_REQUIRED | InputOption::VALUE_IS_ARRAY), "Add custom repositories, either by URL or using JSON arrays", None, vec![]),
                InputOption::new("autoload", Some(PhpMixed::String("a".to_string())), Some(InputOption::VALUE_REQUIRED), "Add PSR-4 autoload mapping. Maps your package's namespace to the provided directory. (Expects a relative path, e.g. src/)", None, vec![]),
            ])
            .set_help(
                "The <info>init</info> command creates a basic composer.json file\n\
                in the current directory.\n\
                \n\
                <info>php composer.phar init</info>\n\
                \n\
                Read more at https://getcomposer.org/doc/03-cli.md#init"
            );
    }

    /// @throws \Seld\JsonLint\ParsingException
    pub fn execute(
        &mut self,
        input: &dyn InputInterface,
        output: &dyn OutputInterface,
    ) -> Result<i64> {
        let io = self.inner.get_io();

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
        let mut options = array_filter(
            &array_intersect_key(&input.get_options(), &array_flip(&allowlist)),
            |val: &PhpMixed| {
                !matches!(val, PhpMixed::Null) && !matches!(val, PhpMixed::List(l) if l.is_empty())
            },
        );

        if options.contains_key("name")
            && !Preg::is_match(
                r"{^[a-z0-9]([_.-]?[a-z0-9]+)*\/[a-z0-9](([_.]|-{1,2})?[a-z0-9]+)*$}D",
                options
                    .get("name")
                    .and_then(|v| v.as_string())
                    .unwrap_or(""),
            )
            .unwrap_or(false)
        {
            return Err(InvalidArgumentException {
                message: format!(
                    "The package name {} is invalid, it should be lowercase and have a vendor name, a forward slash, and a package name, matching: [a-z0-9_.-]+/[a-z0-9_.-]+",
                    options.get("name").and_then(|v| v.as_string()).unwrap_or("")
                ),
                code: 0,
            }
            .into());
        }

        if options.contains_key("author") {
            let author = options
                .get("author")
                .and_then(|v| v.as_string())
                .unwrap_or("")
                .to_string();
            options.insert(
                "authors".to_string(),
                PhpMixed::List(
                    self.format_authors(&author)?
                        .into_iter()
                        .map(|m| {
                            Box::new(PhpMixed::Array(
                                m.into_iter().map(|(k, v)| (k, Box::new(v))).collect(),
                            ))
                        })
                        .collect(),
                ),
            );
            options.shift_remove("author");
        }

        let repositories: Vec<String> = input
            .get_option("repository")
            .as_list()
            .map(|l| {
                l.iter()
                    .filter_map(|v| v.as_string().map(|s| s.to_string()))
                    .collect()
            })
            .unwrap_or_default();
        if (repositories.len() as i64) > 0 {
            let config = Factory::create_config(Some(io), None)?;
            for repo in &repositories {
                let repo_config =
                    RepositoryFactory::config_from_string(io, &config, repo, Some(true))?;
                let entry = options
                    .entry("repositories".to_string())
                    .or_insert_with(|| PhpMixed::List(vec![]));
                if let PhpMixed::List(list) = entry {
                    list.push(Box::new(PhpMixed::Array(
                        repo_config
                            .into_iter()
                            .map(|(k, v)| (k, Box::new(v)))
                            .collect(),
                    )));
                }
            }
        }

        if options.contains_key("stability") {
            let stab = options.shift_remove("stability").unwrap_or(PhpMixed::Null);
            options.insert("minimum-stability".to_string(), stab);
        }

        let require_value = if options.contains_key("require") {
            let req_list: Vec<String> = options
                .get("require")
                .and_then(|v| v.as_list())
                .map(|l| {
                    l.iter()
                        .filter_map(|v| v.as_string().map(|s| s.to_string()))
                        .collect()
                })
                .unwrap_or_default();
            let formatted = self.inner.format_requirements(req_list)?;
            if formatted.is_empty() {
                // PHP: new \stdClass — represented as an empty IndexMap (JSON object)
                PhpMixed::Array(IndexMap::new())
            } else {
                PhpMixed::Array(
                    formatted
                        .into_iter()
                        .map(|(k, v)| (k, Box::new(PhpMixed::String(v))))
                        .collect(),
                )
            }
        } else {
            // PHP: new \stdClass
            PhpMixed::Array(IndexMap::new())
        };
        options.insert("require".to_string(), require_value);

        if options.contains_key("require-dev") {
            let req_list: Vec<String> = options
                .get("require-dev")
                .and_then(|v| v.as_list())
                .map(|l| {
                    l.iter()
                        .filter_map(|v| v.as_string().map(|s| s.to_string()))
                        .collect()
                })
                .unwrap_or_default();
            let formatted = self.inner.format_requirements(req_list)?;
            let value = if formatted.is_empty() {
                PhpMixed::Array(IndexMap::new())
            } else {
                PhpMixed::Array(
                    formatted
                        .into_iter()
                        .map(|(k, v)| (k, Box::new(PhpMixed::String(v))))
                        .collect(),
                )
            };
            options.insert("require-dev".to_string(), value);
        }

        // --autoload - create autoload object
        let mut autoload_path: Option<String> = None;
        if options.contains_key("autoload") {
            let ap = options
                .get("autoload")
                .and_then(|v| v.as_string())
                .unwrap_or("")
                .to_string();
            autoload_path = Some(ap.clone());
            let name = input
                .get_option("name")
                .as_string()
                .unwrap_or("")
                .to_string();
            let namespace = self.namespace_from_package_name(&name).unwrap_or_default();
            let mut psr4: IndexMap<String, Box<PhpMixed>> = IndexMap::new();
            psr4.insert(format!("{}\\", namespace), Box::new(PhpMixed::String(ap)));
            let mut autoload_obj: IndexMap<String, Box<PhpMixed>> = IndexMap::new();
            autoload_obj.insert("psr-4".to_string(), Box::new(PhpMixed::Array(psr4)));
            options.insert("autoload".to_string(), PhpMixed::Array(autoload_obj));
        }

        let file_obj = JsonFile::new(&Factory::get_composer_file(), None, None);
        let options_for_encode: IndexMap<String, Box<PhpMixed>> = options
            .clone()
            .into_iter()
            .map(|(k, v)| (k, Box::new(v)))
            .collect();
        let json = JsonFile::encode(&options_for_encode);

        if input.is_interactive() {
            io.write_error(
                PhpMixed::List(vec![
                    Box::new(PhpMixed::String(String::new())),
                    Box::new(PhpMixed::String(json)),
                    Box::new(PhpMixed::String(String::new())),
                ]),
                true,
                IOInterface::NORMAL,
            );
            if !io.ask_confirmation(
                "Do you confirm generation [<comment>yes</comment>]? ".to_string(),
                true,
            ) {
                io.write_error(
                    PhpMixed::String("<error>Command aborted</error>".to_string()),
                    true,
                    IOInterface::NORMAL,
                );

                return Ok(1);
            }
        } else {
            io.write_error(
                PhpMixed::String(format!("Writing {}", file_obj.get_path())),
                true,
                IOInterface::NORMAL,
            );
        }

        file_obj.write(&PhpMixed::Array(options_for_encode.clone()))?;
        let validate_result = file_obj.validate_schema(JsonFile::LAX_SCHEMA, None);
        if let Err(e) = validate_result {
            // try to downcast to JsonValidationException
            if let Some(json_err) = e.downcast_ref::<JsonValidationException>() {
                io.write_error(
                    PhpMixed::String(
                        "<error>Schema validation error, aborting</error>".to_string(),
                    ),
                    true,
                    IOInterface::NORMAL,
                );
                let errors = format!(
                    " - {}",
                    implode(&format!("{} - ", PHP_EOL), &json_err.get_errors())
                );
                io.write_error(
                    PhpMixed::String(format!("{}:{}{}", json_err.message, PHP_EOL, errors)),
                    true,
                    IOInterface::NORMAL,
                );
                Silencer::call(
                    "unlink",
                    &[PhpMixed::String(file_obj.get_path().to_string())],
                );

                return Ok(1);
            }
            return Err(e);
        }

        // --autoload - Create src folder
        if let Some(ref ap) = autoload_path {
            let filesystem = Filesystem::new();
            filesystem.ensure_directory_exists(ap);

            // dump-autoload only for projects without added dependencies.
            if !self.has_dependencies(&options) {
                self.run_dump_autoload_command(output);
            }
        }

        if input.is_interactive() && is_dir(".git") {
            let mut ignore_file = realpath(".gitignore").unwrap_or_default();

            if ignore_file.is_empty() {
                ignore_file = format!("{}/.gitignore", realpath(".").unwrap_or_default());
            }

            if !self.has_vendor_ignore(&ignore_file, "vendor") {
                let question = "Would you like the <info>vendor</info> directory added to your <info>.gitignore</info> [<comment>yes</comment>]? ".to_string();

                if io.ask_confirmation(question, true) {
                    self.add_vendor_ignore(&ignore_file, "/vendor/");
                }
            }
        }

        let question =
            "Would you like to install dependencies now [<comment>yes</comment>]? ".to_string();
        if input.is_interactive()
            && self.has_dependencies(&options)
            && io.ask_confirmation(question, true)
        {
            self.update_dependencies(output);
        }

        // --autoload - Show post-install configuration info
        if autoload_path.is_some() {
            let name = input
                .get_option("name")
                .as_string()
                .unwrap_or("")
                .to_string();
            let namespace = self.namespace_from_package_name(&name).unwrap_or_default();

            io.write_error(
                PhpMixed::String(format!(
                    "PSR-4 autoloading configured. Use \"<comment>namespace {};</comment>\" in {}",
                    namespace,
                    autoload_path.as_deref().unwrap_or("")
                )),
                true,
                IOInterface::NORMAL,
            );
            io.write_error(
                PhpMixed::String(
                    "Include the Composer autoloader with: <comment>require 'vendor/autoload.php';</comment>"
                        .to_string(),
                ),
                true,
                IOInterface::NORMAL,
            );
        }

        Ok(0)
    }

    pub(crate) fn initialize(&mut self, input: &dyn InputInterface, output: &dyn OutputInterface) {
        self.inner.initialize(input, output);

        if !input.is_interactive() {
            if input.get_option("name").is_null() {
                input.set_option("name", PhpMixed::String(self.get_default_package_name()));
            }

            if input.get_option("author").is_null() {
                input.set_option(
                    "author",
                    self.get_default_author()
                        .map(PhpMixed::String)
                        .unwrap_or(PhpMixed::Null),
                );
            }
        }
    }

    pub(crate) fn interact(
        &mut self,
        input: &dyn InputInterface,
        _output: &dyn OutputInterface,
    ) -> Result<()> {
        let io = self.inner.get_io();
        // @var FormatterHelper $formatter
        let formatter: &FormatterHelper = self.inner.get_helper_set().get("formatter");

        // initialize repos if configured
        let repositories: Vec<String> = input
            .get_option("repository")
            .as_list()
            .map(|l| {
                l.iter()
                    .filter_map(|v| v.as_string().map(|s| s.to_string()))
                    .collect()
            })
            .unwrap_or_default();
        if (repositories.len() as i64) > 0 {
            let config = Factory::create_config(Some(io), None)?;
            io.load_configuration(&config);
            let mut repo_manager = RepositoryFactory::manager(io, &config, None, None);

            let mut repos: Vec<
                Box<dyn crate::repository::repository_interface::RepositoryInterface>,
            > = vec![Box::new(PlatformRepository::new(vec![], PhpMixed::Null))];
            let mut create_default_packagist_repo = true;
            for repo in &repositories {
                let repo_config =
                    RepositoryFactory::config_from_string(io, &config, repo, Some(true))?;
                let is_packagist_false = repo_config
                    .get("packagist")
                    .map(|v| v.as_bool() == Some(false))
                    .unwrap_or(false)
                    && repo_config.len() == 1;
                let is_packagist_org_false = repo_config
                    .get("packagist.org")
                    .map(|v| v.as_bool() == Some(false))
                    .unwrap_or(false)
                    && repo_config.len() == 1;
                if is_packagist_false || is_packagist_org_false {
                    create_default_packagist_repo = false;
                    continue;
                }
                repos.push(RepositoryFactory::create_repo(
                    io,
                    &config,
                    &repo_config,
                    Some(&mut repo_manager),
                )?);
            }

            if create_default_packagist_repo {
                let mut default_config: IndexMap<String, PhpMixed> = IndexMap::new();
                default_config.insert("type".to_string(), PhpMixed::String("composer".to_string()));
                default_config.insert(
                    "url".to_string(),
                    PhpMixed::String("https://repo.packagist.org".to_string()),
                );
                repos.push(RepositoryFactory::create_repo(
                    io,
                    &config,
                    &default_config,
                    Some(&mut repo_manager),
                )?);
            }

            *self.get_repos_mut() = Some(CompositeRepository::new(repos));
            // unset($repos, $config, $repositories);
        }

        io.write_error(
            PhpMixed::List(vec![
                Box::new(PhpMixed::String(String::new())),
                Box::new(PhpMixed::String(formatter.format_block(
                    "Welcome to the Composer config generator",
                    "bg=blue;fg=white",
                    true,
                ))),
                Box::new(PhpMixed::String(String::new())),
            ]),
            true,
            IOInterface::NORMAL,
        );

        // namespace
        io.write_error(
            PhpMixed::List(vec![
                Box::new(PhpMixed::String(String::new())),
                Box::new(PhpMixed::String(
                    "This command will guide you through creating your composer.json config."
                        .to_string(),
                )),
                Box::new(PhpMixed::String(String::new())),
            ]),
            true,
            IOInterface::NORMAL,
        );

        let mut name = input
            .get_option("name")
            .as_string()
            .map(|s| s.to_string())
            .unwrap_or_else(|| self.get_default_package_name());

        let name_default = name.clone();
        let name_for_validate = name.clone();
        name = io
            .ask_and_validate(
                format!(
                    "Package name (<vendor>/<name>) [<comment>{}</comment>]: ",
                    name_default
                ),
                Box::new(move |value: PhpMixed| -> PhpMixed {
                    if value.is_null() {
                        return PhpMixed::String(name_for_validate.clone());
                    }

                    if !Preg::is_match(
                        r"{^[a-z0-9]([_.-]?[a-z0-9]+)*\/[a-z0-9](([_.]|-{1,2})?[a-z0-9]+)*$}D",
                        value.as_string().unwrap_or(""),
                    )
                    .unwrap_or(false)
                    {
                        // TODO(phase-b): closure returning PhpMixed cannot throw — needs Result type
                        panic!(
                            "{}",
                            format!(
                                "The package name {} is invalid, it should be lowercase and have a vendor name, a forward slash, and a package name, matching: [a-z0-9_.-]+/[a-z0-9_.-]+",
                                value.as_string().unwrap_or("")
                            )
                        );
                    }

                    value
                }),
                None,
                PhpMixed::String(name.clone()),
            )
            .as_string()
            .unwrap_or("")
            .to_string();
        input.set_option("name", PhpMixed::String(name));

        let description = input
            .get_option("description")
            .as_string()
            .map(|s| s.to_string());
        let description_default = description.clone();
        let description = io.ask(
            format!(
                "Description [<comment>{}</comment>]: ",
                description.clone().unwrap_or_default()
            ),
            description_default
                .map(PhpMixed::String)
                .unwrap_or(PhpMixed::Null),
        );
        input.set_option("description", description);

        let author = input
            .get_option("author")
            .as_string()
            .map(|s| s.to_string())
            .unwrap_or_else(|| self.get_default_author().unwrap_or_default());

        let author_for_validate = author.clone();
        let author_default = author.clone();
        // PHP: $this->parseAuthorString is called inside a closure. We approximate by binding.
        let author_value = io.ask_and_validate(
            format!(
                "Author [{} n to skip]: ",
                if is_string(&PhpMixed::String(author.clone())) {
                    format!("<comment>{}</comment>, ", author)
                } else {
                    String::new()
                }
            ),
            // TODO(phase-b): closure cannot call &self.parse_author_string; needs &self capture
            Box::new(move |value: PhpMixed| -> PhpMixed {
                let value_str = value.as_string().unwrap_or("").to_string();
                if value_str == "n" || value_str == "no" {
                    return PhpMixed::Null;
                }
                let value_or_default = if value_str.is_empty() {
                    author_for_validate.clone()
                } else {
                    value_str
                };
                // TODO(phase-b): would call self.parse_author_string(value_or_default)
                let _ = value_or_default;
                PhpMixed::Null
            }),
            None,
            PhpMixed::String(author_default),
        );
        input.set_option("author", author_value);

        let minimum_stability = input
            .get_option("stability")
            .as_string()
            .map(|s| s.to_string());
        let minimum_stability_default = minimum_stability.clone();
        let minimum_stability_for_validate = minimum_stability.clone();
        let minimum_stability_value = io.ask_and_validate(
            format!(
                "Minimum Stability [<comment>{}</comment>]: ",
                minimum_stability.clone().unwrap_or_default()
            ),
            Box::new(move |value: PhpMixed| -> PhpMixed {
                if value.is_null() {
                    return minimum_stability_for_validate
                        .clone()
                        .map(PhpMixed::String)
                        .unwrap_or(PhpMixed::Null);
                }

                if !BasePackage::stabilities().contains_key(value.as_string().unwrap_or("")) {
                    // TODO(phase-b): closure cannot throw
                    panic!(
                        "{}",
                        format!(
                            "Invalid minimum stability \"{}\". Must be empty or one of: {}",
                            value.as_string().unwrap_or(""),
                            implode(", ", &array_keys(&BasePackage::stabilities()))
                        )
                    );
                }

                value
            }),
            None,
            minimum_stability_default
                .map(PhpMixed::String)
                .unwrap_or(PhpMixed::Null),
        );
        input.set_option("stability", minimum_stability_value);

        let type_val = input.get_option("type");
        let type_str = type_val.as_string().unwrap_or("").to_string();
        let mut type_value = io.ask(
            format!(
                "Package Type (e.g. library, project, metapackage, composer-plugin) [<comment>{}</comment>]: ",
                type_str
            ),
            type_val,
        );
        if type_value.as_string() == Some("") || matches!(type_value, PhpMixed::Bool(false)) {
            type_value = PhpMixed::Null;
        }
        input.set_option("type", type_value);

        let mut license = input
            .get_option("license")
            .as_string()
            .map(|s| s.to_string());
        if license.is_none() {
            let default_license = server_get("COMPOSER_DEFAULT_LICENSE");
            if !empty(
                &default_license
                    .clone()
                    .map(PhpMixed::String)
                    .unwrap_or(PhpMixed::Null),
            ) {
                license = default_license;
            }
        }

        let license = io.ask(
            format!(
                "License [<comment>{}</comment>]: ",
                license.clone().unwrap_or_default()
            ),
            license.map(PhpMixed::String).unwrap_or(PhpMixed::Null),
        );
        let spdx = SpdxLicenses::new();
        if !license.is_null()
            && !spdx.validate(license.as_string().unwrap_or(""))
            && license.as_string() != Some("proprietary")
        {
            return Err(InvalidArgumentException {
                message: format!(
                    "Invalid license provided: {}. Only SPDX license identifiers (https://spdx.org/licenses/) or \"proprietary\" are accepted.",
                    license.as_string().unwrap_or("")
                ),
                code: 0,
            }
            .into());
        }
        input.set_option("license", license);

        io.write_error(
            PhpMixed::List(vec![
                Box::new(PhpMixed::String(String::new())),
                Box::new(PhpMixed::String("Define your dependencies.".to_string())),
                Box::new(PhpMixed::String(String::new())),
            ]),
            true,
            IOInterface::NORMAL,
        );

        // prepare to resolve dependencies
        let repos = self.get_repos();
        let preferred_stability =
            if let Some(s) = minimum_stability_default.clone().filter(|s| !s.is_empty()) {
                s
            } else {
                "stable".to_string()
            };
        // TODO(phase-b): repos instanceof CompositeRepository downcast
        let _platform_repo: Option<&PlatformRepository> = None;

        // (omitted: iterate repos to find PlatformRepository instance)

        let question = "Would you like to define your dependencies (require) interactively [<comment>yes</comment>]? ".to_string();
        let require: Vec<String> = input
            .get_option("require")
            .as_list()
            .map(|l| {
                l.iter()
                    .filter_map(|v| v.as_string().map(|s| s.to_string()))
                    .collect()
            })
            .unwrap_or_default();
        let requirements = if (require.len() as i64) > 0 || io.ask_confirmation(question, true) {
            self.determine_requirements(
                input,
                _output,
                require,
                _platform_repo.unwrap_or(&PlatformRepository::new(vec![], PhpMixed::Null)),
                &preferred_stability,
                false,
                false,
            )?
        } else {
            vec![]
        };
        input.set_option(
            "require",
            PhpMixed::List(
                requirements
                    .into_iter()
                    .map(|s| Box::new(PhpMixed::String(s)))
                    .collect(),
            ),
        );

        let question = "Would you like to define your dev dependencies (require-dev) interactively [<comment>yes</comment>]? ".to_string();
        let require_dev: Vec<String> = input
            .get_option("require-dev")
            .as_list()
            .map(|l| {
                l.iter()
                    .filter_map(|v| v.as_string().map(|s| s.to_string()))
                    .collect()
            })
            .unwrap_or_default();
        let dev_requirements =
            if (require_dev.len() as i64) > 0 || io.ask_confirmation(question, true) {
                self.determine_requirements(
                    input,
                    _output,
                    require_dev,
                    _platform_repo.unwrap_or(&PlatformRepository::new(vec![], PhpMixed::Null)),
                    &preferred_stability,
                    false,
                    false,
                )?
            } else {
                vec![]
            };
        input.set_option(
            "require-dev",
            PhpMixed::List(
                dev_requirements
                    .into_iter()
                    .map(|s| Box::new(PhpMixed::String(s)))
                    .collect(),
            ),
        );

        // --autoload - input and validation
        let mut autoload = input
            .get_option("autoload")
            .as_string()
            .map(|s| s.to_string())
            .filter(|s| !s.is_empty())
            .unwrap_or_else(|| "src/".to_string());
        let name_str = input
            .get_option("name")
            .as_string()
            .unwrap_or("")
            .to_string();
        let namespace = self
            .namespace_from_package_name(&name_str)
            .unwrap_or_default();
        let autoload_for_validate = autoload.clone();
        let autoload_default = autoload.clone();
        let autoload_value = io.ask_and_validate(
            format!(
                "Add PSR-4 autoload mapping? Maps namespace \"{}\" to the entered relative path. [<comment>{}</comment>, n to skip]: ",
                namespace, autoload
            ),
            Box::new(move |value: PhpMixed| -> PhpMixed {
                if value.is_null() {
                    return PhpMixed::String(autoload_for_validate.clone());
                }

                let value_str = value.as_string().unwrap_or("").to_string();
                if value_str == "n" || value_str == "no" {
                    return PhpMixed::Null;
                }

                let value_or_default = if value_str.is_empty() {
                    autoload_for_validate.clone()
                } else {
                    value_str
                };

                if !Preg::is_match(r"{^[^/][A-Za-z0-9\-_/]+/$}", &value_or_default).unwrap_or(false)
                {
                    // TODO(phase-b): closure cannot throw
                    panic!(
                        "{}",
                        sprintf(
                            "The src folder name \"%s\" is invalid. Please add a relative path with tailing forward slash. [A-Za-z0-9_-/]+/",
                            &[PhpMixed::String(value_or_default.clone())],
                        )
                    );
                }

                PhpMixed::String(value_or_default)
            }),
            None,
            PhpMixed::String(autoload_default),
        );
        input.set_option("autoload", autoload_value);

        Ok(())
    }

    /// @return array{name: string, email: string|null}
    fn parse_author_string(&self, author: &str) -> Result<IndexMap<String, Option<String>>> {
        if let Some(m) = Preg::is_match_strict_groups(
            r#"/^(?P<name>[- .,\p{L}\p{N}\p{Mn}\'’\"()]+)(?:\s+<(?P<email>.+?)>)?$/u"#,
            author,
        ) {
            let email = m.get("email").cloned();
            if let Some(ref email) = email {
                if !self.is_valid_email(email) {
                    return Err(InvalidArgumentException {
                        message: format!("Invalid email \"{}\"", email),
                        code: 0,
                    }
                    .into());
                }
            }

            let mut result: IndexMap<String, Option<String>> = IndexMap::new();
            result.insert(
                "name".to_string(),
                Some(trim(&m.get("name").cloned().unwrap_or_default(), None)),
            );
            result.insert("email".to_string(), email);

            return Ok(result);
        }

        Err(InvalidArgumentException {
            message: "Invalid author string.  Must be in the formats: Jane Doe or John Smith <john@example.com>"
                .to_string(),
            code: 0,
        }
        .into())
    }

    /// @return array<int, array{name: string, email?: string}>
    pub(crate) fn format_authors(&self, author: &str) -> Result<Vec<IndexMap<String, PhpMixed>>> {
        let parsed = self.parse_author_string(author)?;
        let mut author_map: IndexMap<String, PhpMixed> = IndexMap::new();
        let name = parsed.get("name").cloned().unwrap_or(None);
        let email = parsed.get("email").cloned().unwrap_or(None);
        if let Some(name) = name {
            author_map.insert("name".to_string(), PhpMixed::String(name));
        }
        if let Some(email) = email {
            author_map.insert("email".to_string(), PhpMixed::String(email));
        }

        Ok(vec![author_map])
    }

    /// Extract namespace from package's vendor name.
    ///
    /// new_projects.acme-extra/package-name becomes "NewProjectsAcmeExtra\PackageName"
    pub fn namespace_from_package_name(&self, package_name: &str) -> Option<String> {
        if package_name.is_empty() || strpos(package_name, "/").is_none() {
            return None;
        }

        let namespace: Vec<String> = array_map(
            |part: &String| {
                let part = Preg::replace(r"/[^a-z0-9]/i", " ", part.clone());
                let part = ucwords(&part);
                str_replace(" ", "", &part)
            },
            &explode("/", package_name),
        );

        Some(implode("\\", &namespace))
    }

    /// @return array<string, string>
    pub(crate) fn get_git_config(&mut self) -> IndexMap<String, String> {
        if self.git_config.is_some() {
            return self.git_config.clone().unwrap_or_default();
        }

        let mut process = ProcessExecutor::new(self.inner.get_io());

        let mut output = String::new();
        if process.execute(
            &vec!["git".to_string(), "config".to_string(), "-l".to_string()],
            &mut output,
            None,
        ) == 0
        {
            self.git_config = Some(IndexMap::new());
            let matches = Preg::is_match_all_strict_groups(r"{^([^=]+)=(.*)$}m", &output);
            if let Some(m) = matches {
                let keys: Vec<String> = m.get(1).cloned().unwrap_or_default();
                let values: Vec<String> = m.get(2).cloned().unwrap_or_default();
                for (key, value) in keys.iter().zip(values.iter()) {
                    self.git_config
                        .as_mut()
                        .unwrap()
                        .insert(key.clone(), value.clone());
                }
            }

            return self.git_config.clone().unwrap_or_default();
        }

        self.git_config = Some(IndexMap::new());
        IndexMap::new()
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
    pub(crate) fn has_vendor_ignore(&self, ignore_file: &str, vendor: &str) -> bool {
        if !file_exists(ignore_file) {
            return false;
        }

        let pattern = sprintf(
            "{^/?%s(/\\*?)?$}",
            &[PhpMixed::String(preg_quote(vendor, None))],
        );

        let lines = file(ignore_file, FILE_IGNORE_NEW_LINES).unwrap_or_default();
        for line in &lines {
            if Preg::is_match(&pattern, line).unwrap_or(false) {
                return true;
            }
        }

        false
    }

    pub(crate) fn add_vendor_ignore(&self, ignore_file: &str, vendor: &str) {
        let mut contents = String::new();
        if file_exists(ignore_file) {
            contents = file_get_contents(ignore_file).unwrap_or_default();

            if strpos(&contents, "\n") != Some(0) {
                contents.push('\n');
            }
        }

        file_put_contents(ignore_file, &format!("{}{}\n", contents, vendor));
    }

    pub(crate) fn is_valid_email(&self, email: &str) -> bool {
        // assume it's valid if we can't validate it
        if !function_exists("filter_var") {
            return true;
        }

        shirabe_php_shim::filter_var(email, FILTER_VALIDATE_EMAIL)
    }

    fn update_dependencies(&self, output: &dyn OutputInterface) {
        // PHP try/catch: catch \Exception
        let result = self.inner.get_application().and_then(|app| {
            let update_command = app.find("update")?;
            app.reset_composer()?;
            update_command.run(ArrayInput::new(IndexMap::new()), output)?;
            Ok(())
        });
        if let Err(_e) = result {
            self.inner.get_io().write_error(
                PhpMixed::String(
                    "Could not update dependencies. Run `composer update` to see more information."
                        .to_string(),
                ),
                true,
                IOInterface::NORMAL,
            );
        }
    }

    fn run_dump_autoload_command(&self, output: &dyn OutputInterface) {
        let result = self.inner.get_application().and_then(|app| {
            let command = app.find("dump-autoload")?;
            app.reset_composer()?;
            command.run(ArrayInput::new(IndexMap::new()), output)?;
            Ok(())
        });
        if let Err(_e) = result {
            self.inner.get_io().write_error(
                PhpMixed::String("Could not run dump-autoload.".to_string()),
                true,
                IOInterface::NORMAL,
            );
        }
    }

    /// @param array<string, string|array<string>> $options
    fn has_dependencies(&self, options: &IndexMap<String, PhpMixed>) -> bool {
        let requires = options.get("require").cloned().unwrap_or(PhpMixed::Null);
        let requires_arr_empty = match &requires {
            PhpMixed::Array(m) => m.is_empty(),
            PhpMixed::List(l) => l.is_empty(),
            PhpMixed::Null => true,
            _ => false,
        };
        let dev_requires = options.get("require-dev").cloned();
        let dev_requires_arr_empty = match &dev_requires {
            Some(PhpMixed::Array(m)) => m.is_empty(),
            Some(PhpMixed::List(l)) => l.is_empty(),
            Some(PhpMixed::Null) | None => true,
            _ => false,
        };

        !requires_arr_empty || !dev_requires_arr_empty
    }

    fn sanitize_package_name_component(&self, name: &str) -> String {
        let name = Preg::replace(
            r"{(?:([a-z])([A-Z])|([A-Z])([A-Z][a-z]))}",
            "$1$3-$2$4",
            name.to_string(),
        );
        let name = strtolower(&name);
        let name = Preg::replace(r"{^[_.-]+|[_.-]+$|[^a-z0-9_.-]}u", "", name);
        let name = Preg::replace(r"{([_.-]){2,}}u", "$1", name);

        name
    }

    fn get_default_package_name(&mut self) -> String {
        let git = self.get_git_config();
        let cwd = realpath(".").unwrap_or_default();
        let name = basename(&cwd);
        let name = self.sanitize_package_name_component(&name);

        let mut vendor = name.clone();
        let composer_default_vendor = server_get("COMPOSER_DEFAULT_VENDOR");
        if !empty(
            &composer_default_vendor
                .clone()
                .map(PhpMixed::String)
                .unwrap_or(PhpMixed::Null),
        ) {
            vendor = composer_default_vendor.unwrap_or_default();
        } else if git.contains_key("github.user") {
            vendor = git.get("github.user").cloned().unwrap_or_default();
        } else if !empty(
            &server_get("USERNAME")
                .clone()
                .map(PhpMixed::String)
                .unwrap_or(PhpMixed::Null),
        ) {
            vendor = server_get("USERNAME").unwrap_or_default();
        } else if !empty(
            &server_get("USER")
                .clone()
                .map(PhpMixed::String)
                .unwrap_or(PhpMixed::Null),
        ) {
            vendor = server_get("USER").unwrap_or_default();
        } else if !get_current_user().is_empty() {
            vendor = get_current_user();
        }

        let vendor = self.sanitize_package_name_component(&vendor);

        format!("{}/{}", vendor, name)
    }

    fn get_default_author(&mut self) -> Option<String> {
        let git = self.get_git_config();

        let mut author_name: Option<String> = None;
        let composer_default_author = server_get("COMPOSER_DEFAULT_AUTHOR");
        if !empty(
            &composer_default_author
                .clone()
                .map(PhpMixed::String)
                .unwrap_or(PhpMixed::Null),
        ) {
            author_name = composer_default_author;
        } else if git.contains_key("user.name") {
            author_name = git.get("user.name").cloned();
        }

        let mut author_email: Option<String> = None;
        let composer_default_email = server_get("COMPOSER_DEFAULT_EMAIL");
        if !empty(
            &composer_default_email
                .clone()
                .map(PhpMixed::String)
                .unwrap_or(PhpMixed::Null),
        ) {
            author_email = composer_default_email;
        } else if git.contains_key("user.email") {
            author_email = git.get("user.email").cloned();
        }

        if let (Some(name), Some(email)) = (author_name, author_email) {
            return Some(sprintf(
                "%s <%s>",
                &[PhpMixed::String(name), PhpMixed::String(email)],
            ));
        }

        None
    }
}
