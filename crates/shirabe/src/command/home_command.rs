//! ref: composer/src/Composer/Command/HomeCommand.php

use anyhow::Result;
use shirabe_external_packages::symfony::component::console::input::InputInterface;
use shirabe_external_packages::symfony::component::console::output::OutputInterface;
use shirabe_php_shim::{FILTER_VALIDATE_URL, PhpMixed, filter_var};

use crate::command::{BaseCommand, BaseCommandData, HasBaseCommandData};
use crate::console::input::InputArgument;
use crate::console::input::InputOption;
use crate::io::IOInterface;
use crate::io::IOInterfaceImmutable;
use crate::package::CompletePackageInterfaceHandle;
use crate::package::PackageInterface;
use crate::package::RootPackageInterface;
use crate::repository::RepositoryFactory;
use crate::repository::RepositoryInterface;
use crate::repository::RootPackageRepository;
use crate::util::Platform;
use crate::util::ProcessExecutor;

#[derive(Debug)]
pub struct HomeCommand {
    base_command_data: BaseCommandData,
}

impl HomeCommand {
    pub fn configure(&mut self) {
        // TODO(cli-completion): suggest_installed_package() for `packages` argument
        self.set_name("browse")
            .set_aliases(&["home".to_string()])
            .set_description("Opens the package's repository URL or homepage in your browser")
            .set_definition(&[
                InputArgument::new(
                    "packages",
                    Some(InputArgument::IS_ARRAY),
                    "Package(s) to browse to.",
                    None,
                )
                .unwrap()
                .into(),
                InputOption::new(
                    "homepage",
                    Some(shirabe_php_shim::PhpMixed::String("H".to_string())),
                    Some(InputOption::VALUE_NONE),
                    "Open the homepage instead of the repository URL.",
                    None,
                )
                .unwrap()
                .into(),
                InputOption::new(
                    "show",
                    Some(shirabe_php_shim::PhpMixed::String("s".to_string())),
                    Some(InputOption::VALUE_NONE),
                    "Only show the homepage or repository URL.",
                    None,
                )
                .unwrap()
                .into(),
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
        &mut self,
        input: &dyn InputInterface,
        _output: &dyn OutputInterface,
    ) -> Result<i64> {
        let repos = self.initialize_repos()?;
        // TODO(phase-b): clone_box to release self borrow held by get_io.
        let io_box = self.get_io().clone();
        let io_ref = io_box.borrow();
        let io: &dyn IOInterface = &*io_ref;
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
            let composer_rc = self.require_composer(None, None)?;
            let composer_ref = crate::command::composer_full(&composer_rc);
            vec![composer_ref.get_package().get_name().to_string()]
        } else {
            packages
        };

        let show_homepage = input.get_option("homepage").as_bool().unwrap_or(false);
        let show_only = input.get_option("show").as_bool().unwrap_or(false);

        for package_name in &packages {
            let mut handled = false;
            let mut package_exists = false;

            'repos: for repo in &repos {
                for package in repo.find_packages(&package_name, None) {
                    package_exists = true;
                    if let Some(complete_pkg) = package.as_complete() {
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
        &mut self,
        package: CompletePackageInterfaceHandle,
        show_homepage: bool,
        show_only: bool,
    ) -> bool {
        let support = package.get_support();
        let mut url: Option<String> = support
            .get("source")
            .cloned()
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
            self.get_io().write(&format!("<info>{}</info>", url));
        } else {
            self.open_browser(&url);
        }

        true
    }

    fn open_browser(&mut self, url: &str) {
        let mut process = ProcessExecutor::new(Some(self.get_io().clone()));
        if Platform::is_windows() {
            let _ = process.execute(
                PhpMixed::from(vec!["start", "\"web\"", "explorer", url]),
                (),
                (),
            );
            return;
        }

        let linux = process
            .execute(PhpMixed::from(vec!["which", "xdg-open"]), (), ())
            .unwrap_or(1);
        let osx = process
            .execute(PhpMixed::from(vec!["which", "open"]), (), ())
            .unwrap_or(1);

        if linux == 0 {
            let _ = process.execute(PhpMixed::from(vec!["xdg-open", url]), (), ());
        } else if osx == 0 {
            let _ = process.execute(PhpMixed::from(vec!["open", url]), (), ());
        } else {
            self.get_io().write_error(&format!(
                "No suitable browser opening command found, open yourself: {}",
                url
            ));
        }
    }

    fn initialize_repos(&mut self) -> Result<Vec<crate::repository::RepositoryInterfaceHandle>> {
        let composer = self.try_composer(None, None);

        if let Some(composer) = composer {
            let composer = crate::command::composer_full(&composer);
            let mut repos: Vec<crate::repository::RepositoryInterfaceHandle> = vec![];
            repos.push(crate::repository::RepositoryInterfaceHandle::new(
                RootPackageRepository::new(composer.get_package().clone()),
            ));
            // TODO(phase-b): get_local_repository / get_repositories return shared refs; needs Rc<dyn ...> migration
            return Ok(repos);
        }

        RepositoryFactory::default_repos_with_default_manager(self.get_io())
            .map(|m| m.into_iter().map(|(_, v)| v).collect())
    }
}

impl HasBaseCommandData for HomeCommand {
    fn base_command_data(&self) -> &BaseCommandData {
        &self.base_command_data
    }

    fn base_command_data_mut(&mut self) -> &mut BaseCommandData {
        &mut self.base_command_data
    }
}
