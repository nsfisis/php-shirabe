//! ref: composer/src/Composer/Command/HomeCommand.php

use anyhow::Result;
use shirabe_external_packages::symfony::component::console::command::command::Command;
use shirabe_external_packages::symfony::console::input::input_interface::InputInterface;
use shirabe_external_packages::symfony::console::output::output_interface::OutputInterface;
use shirabe_php_shim::{FILTER_VALIDATE_URL, filter_var};

use crate::command::base_command::BaseCommand;
use crate::command::completion_trait::CompletionTrait;
use crate::composer::Composer;
use crate::console::input::input_argument::InputArgument;
use crate::console::input::input_option::InputOption;
use crate::io::io_interface::IOInterface;
use crate::package::complete_package_interface::CompletePackageInterface;
use crate::repository::repository_factory::RepositoryFactory;
use crate::repository::repository_interface::RepositoryInterface;
use crate::repository::root_package_repository::RootPackageRepository;
use crate::util::platform::Platform;
use crate::util::process_executor::ProcessExecutor;

#[derive(Debug)]
pub struct HomeCommand {
    inner: Command,
    composer: Option<Composer>,
    io: Option<Box<dyn IOInterface>>,
}

impl CompletionTrait for HomeCommand {
    fn require_composer(
        &self,
        disable_plugins: Option<bool>,
        disable_scripts: Option<bool>,
    ) -> Composer {
        todo!()
    }
}

impl HomeCommand {
    pub fn configure(&mut self) {
        self.inner
            .set_name("browse")
            .set_aliases(vec!["home".to_string()])
            .set_description("Opens the package's repository URL or homepage in your browser")
            .set_definition(vec![
                InputArgument::new(
                    "packages",
                    Some(InputArgument::IS_ARRAY),
                    "Package(s) to browse to.",
                    None,
                    self.suggest_installed_package(),
                ),
                InputOption::new(
                    "homepage",
                    Some(shirabe_php_shim::PhpMixed::String("H".to_string())),
                    Some(InputOption::VALUE_NONE),
                    "Open the homepage instead of the repository URL.",
                    None,
                    vec![],
                ),
                InputOption::new(
                    "show",
                    Some(shirabe_php_shim::PhpMixed::String("s".to_string())),
                    Some(InputOption::VALUE_NONE),
                    "Only show the homepage or repository URL.",
                    None,
                    vec![],
                ),
            ])
            .set_help(
                "The home command opens or shows a package's repository URL or\n\
                homepage in your default browser.\n\n\
                To open the homepage by default, use -H or --homepage.\n\
                To show instead of open the repository or homepage URL, use -s or --show.\n\n\
                Read more at https://getcomposer.org/doc/03-cli.md#browse-home",
            );
    }

    pub fn execute(
        &self,
        input: &dyn InputInterface,
        _output: &dyn OutputInterface,
    ) -> Result<i64> {
        let repos = self.initialize_repos()?;
        let io = self.inner.get_io();
        let mut return_code: i64 = 0;

        let packages: Vec<String> = input
            .get_argument("packages")
            .as_list()
            .map(|l| {
                l.iter()
                    .filter_map(|v| v.as_string().map(|s| s.to_string()))
                    .collect()
            })
            .unwrap_or_default();

        let packages = if packages.is_empty() {
            io.write_error("No package specified, opening homepage for the root package");
            vec![
                self.inner
                    .require_composer()?
                    .get_package()
                    .get_name()
                    .to_string(),
            ]
        } else {
            packages
        };

        let show_homepage = input.get_option("homepage").as_bool().unwrap_or(false);
        let show_only = input.get_option("show").as_bool().unwrap_or(false);

        for package_name in &packages {
            let mut handled = false;
            let mut package_exists = false;

            'repos: for repo in &repos {
                for package in repo.find_packages(package_name) {
                    package_exists = true;
                    if let Some(complete_pkg) = package.as_complete_package_interface() {
                        if self.handle_package(complete_pkg, show_homepage, show_only) {
                            handled = true;
                            break 'repos;
                        }
                    }
                }
            }

            if !package_exists {
                return_code = 1;
                io.write_error(&format!(
                    "<warning>Package {} not found</warning>",
                    package_name
                ));
            }

            if !handled {
                return_code = 1;
                let msg = if show_homepage {
                    "Invalid or missing homepage"
                } else {
                    "Invalid or missing repository URL"
                };
                io.write_error(&format!("<warning>{} for {}</warning>", msg, package_name));
            }
        }

        Ok(return_code)
    }

    fn handle_package(
        &self,
        package: &dyn CompletePackageInterface,
        show_homepage: bool,
        show_only: bool,
    ) -> bool {
        let support = package.get_support();
        let mut url = support
            .get("source")
            .and_then(|v| v.as_string())
            .map(|s| s.to_string())
            .or_else(|| package.get_source_url().map(|s| s.to_string()));
        if url.as_deref().map_or(true, |s| s.is_empty()) || show_homepage {
            url = package.get_homepage().map(|s| s.to_string());
        }

        let url = match url {
            None => return false,
            Some(u) if u.is_empty() => return false,
            Some(u) => u,
        };

        if !filter_var(&url, FILTER_VALIDATE_URL) {
            return false;
        }

        if show_only {
            self.inner.get_io().write(&format!("<info>{}</info>", url));
        } else {
            self.open_browser(&url);
        }

        true
    }

    fn open_browser(&self, url: &str) {
        let io = self.inner.get_io();
        let mut process = ProcessExecutor::new(io);
        if Platform::is_windows() {
            process.execute(&["start", "\"web\"", "explorer", url], None);
            return;
        }

        let linux = process.execute(&["which", "xdg-open"], None);
        let osx = process.execute(&["which", "open"], None);

        if linux == 0 {
            process.execute(&["xdg-open", url], None);
        } else if osx == 0 {
            process.execute(&["open", url], None);
        } else {
            io.write_error(&format!(
                "No suitable browser opening command found, open yourself: {}",
                url
            ));
        }
    }

    fn initialize_repos(&self) -> Result<Vec<Box<dyn RepositoryInterface>>> {
        let composer = self.inner.try_composer();

        if let Some(composer) = composer {
            let mut repos: Vec<Box<dyn RepositoryInterface>> = vec![];
            repos.push(Box::new(RootPackageRepository::new(
                composer.get_package().clone_package(),
            )));
            repos.push(Box::new(
                composer.get_repository_manager().get_local_repository(),
            ));
            for repo in composer.get_repository_manager().get_repositories() {
                repos.push(repo);
            }
            return Ok(repos);
        }

        Ok(RepositoryFactory::default_repos_with_default_manager(
            self.inner.get_io(),
        ))
    }
}

impl BaseCommand for HomeCommand {
    fn inner(&self) -> &Command {
        &self.inner
    }

    fn inner_mut(&mut self) -> &mut Command {
        &mut self.inner
    }

    fn composer(&self) -> Option<&Composer> {
        self.composer.as_ref()
    }

    fn composer_mut(&mut self) -> &mut Option<Composer> {
        &mut self.composer
    }

    fn io(&self) -> Option<&dyn IOInterface> {
        self.io.as_deref()
    }

    fn io_mut(&mut self) -> &mut Option<Box<dyn IOInterface>> {
        &mut self.io
    }
}
