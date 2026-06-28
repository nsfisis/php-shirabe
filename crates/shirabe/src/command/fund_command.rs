//! ref: composer/src/Composer/Command/FundCommand.php

use crate::command::base_command::base_command_initialize;
use crate::command::{BaseCommand, BaseCommandData};
use crate::console::input::InputOption;
use crate::io::IOInterfaceImmutable;
use crate::json::JsonFile;
use crate::package::base_package::{self};
use crate::repository::CompositeRepository;
use crate::repository::RepositoryInterface;
use indexmap::IndexMap;
use shirabe_external_packages::composer::pcre::Preg;
use shirabe_external_packages::symfony::console::command::command::Command;
use shirabe_external_packages::symfony::console::formatter::OutputFormatter;
use shirabe_external_packages::symfony::console::input::InputInterface;
use shirabe_external_packages::symfony::console::output::OutputInterface;
use shirabe_php_shim::PhpMixed;
use shirabe_semver::constraint::AnyConstraint;
use shirabe_semver::constraint::MatchAllConstraint;
use std::cell::RefCell;
use std::rc::Rc;

#[derive(Debug)]
pub struct FundCommand {
    base_command_data: BaseCommandData,
}

impl Default for FundCommand {
    fn default() -> Self {
        Self::new()
    }
}

impl FundCommand {
    pub fn new() -> Self {
        let command = FundCommand {
            base_command_data: BaseCommandData::new(None),
        };
        command
            .configure()
            .expect("FundCommand::configure uses static, valid metadata");
        command
    }

    fn insert_funding_data(
        fundings: &mut IndexMap<String, IndexMap<String, Vec<String>>>,
        package: &crate::package::CompletePackageInterfaceHandle,
    ) -> anyhow::Result<()> {
        let pretty_name = package.get_pretty_name();
        let (vendor, package_name) = pretty_name
            .split_once('/')
            .unwrap_or(("", pretty_name.as_str()));

        for funding_option in package.get_funding() {
            let url_val = funding_option.get("url").and_then(|v| v.as_string());
            if url_val.is_none_or(|s| s.is_empty()) {
                continue;
            }
            let mut url = url_val.unwrap().to_string();
            let r#type = funding_option
                .get("type")
                .and_then(|v| v.as_string())
                .unwrap_or("");
            if r#type == "github"
                && let Some(matches) =
                    Preg::is_match_with_indexed_captures(r"^https://github.com/([^/]+)$", &url)
                && let Some(sponsor) = matches.into_iter().nth(1)
            {
                url = format!("https://github.com/sponsors/{}", sponsor);
            }
            fundings
                .entry(vendor.to_string())
                .or_default()
                .entry(url)
                .or_default()
                .push(package_name.to_string());
        }
        Ok(())
    }
}

impl Command for FundCommand {
    fn configure(&self) -> anyhow::Result<()> {
        self.set_name("fund")?;
        self.set_description("Discover how to help fund the maintenance of your dependencies");
        self.set_definition(&[InputOption::new(
            "format",
            Some(PhpMixed::String("f".to_string())),
            Some(InputOption::VALUE_REQUIRED),
            "Format of the output: text or json",
            Some(PhpMixed::String("text".to_string())),
        )
        .unwrap()
        .into()]);
        Ok(())
    }

    fn execute(
        &self,
        input: Rc<RefCell<dyn InputInterface>>,
        _output: Rc<RefCell<dyn OutputInterface>>,
    ) -> anyhow::Result<i64> {
        let composer = self.require_composer(None, None)?;
        let composer = crate::composer::composer_full(&composer);

        let repository_manager = composer.get_repository_manager().clone();
        let repository_manager = repository_manager.borrow();
        let repo = repository_manager.get_local_repository();
        let mut remote_repos =
            CompositeRepository::new(repository_manager.get_repositories().to_vec());
        let mut fundings: IndexMap<String, IndexMap<String, Vec<String>>> = IndexMap::new();

        let mut packages_to_load: IndexMap<String, Option<AnyConstraint>> = IndexMap::new();
        let mut packages_to_load_names: indexmap::IndexSet<String> = indexmap::IndexSet::new();
        for package in repo.get_packages()? {
            if package.as_alias().is_some() {
                continue;
            }
            packages_to_load.insert(
                package.get_name(),
                Some(MatchAllConstraint::new(None).into()),
            );
            packages_to_load_names.insert(package.get_name());
        }

        // load all packages dev versions in parallel
        let result = remote_repos.load_packages(
            packages_to_load,
            IndexMap::from([("dev".to_string(), base_package::STABILITY_DEV)]),
            IndexMap::new(),
            IndexMap::new(),
        )?;

        // collect funding data from default branches
        for (_, package) in &result.packages {
            if package.as_alias().is_none() {
                // TODO: check for CompleteAliasPackage as well
                if let Some(complete_pkg) = package.as_complete()
                    && complete_pkg.is_default_branch()
                    && !complete_pkg.get_funding().is_empty()
                    && packages_to_load_names.contains(&complete_pkg.get_name())
                {
                    Self::insert_funding_data(&mut fundings, &complete_pkg)?;
                    packages_to_load_names.shift_remove(&complete_pkg.get_name());
                }
            }
        }

        // collect funding from installed packages if none was found in the default branch above
        for package in repo.get_packages()? {
            if package.as_alias().is_some() || !packages_to_load_names.contains(&package.get_name())
            {
                continue;
            }
            // TODO: check for CompleteAliasPackage as well
            if let Some(complete_pkg) = package.as_complete()
                && !complete_pkg.get_funding().is_empty()
            {
                Self::insert_funding_data(&mut fundings, &complete_pkg)?;
            }
        }

        fundings.sort_keys();

        let io = self.get_io();

        let format = input
            .borrow()
            .get_option("format")?
            .as_string()
            .unwrap_or("text")
            .to_string();
        if !matches!(format.as_str(), "text" | "json") {
            io.write_error(&format!(
                "Unsupported format \"{}\". See help for supported formats.",
                format
            ));
            return Ok(1);
        }

        if !fundings.is_empty() && format == "text" {
            let mut prev: Option<String> = None;

            io.write("The following packages were found in your dependencies which publish funding information:");

            for (vendor, links) in &fundings {
                io.write("");
                io.write(&format!("<comment>{}</comment>", vendor));
                for (url, packages) in links {
                    let line = format!("  <info>{}</info>", packages.join(", "));
                    if prev.as_deref() != Some(&line) {
                        io.write(&line);
                        prev = Some(line);
                    }
                    io.write(&format!(
                        "    <href={}>{}</>",
                        OutputFormatter::escape(url)?,
                        url
                    ));
                }
            }

            io.write("");
            io.write(
                "Please consider following these links and sponsoring the work of package authors!",
            );
            io.write("Thank you!");
        } else if format == "json" {
            let fundings_mixed: PhpMixed = fundings.clone().into();
            io.write(&JsonFile::encode(&fundings_mixed));
        } else {
            io.write("No funding links were found in your package dependencies. This doesn't mean they don't need your support!");
        }

        Ok(0)
    }

    fn initialize(
        &self,
        input: Rc<RefCell<dyn InputInterface>>,
        output: Rc<RefCell<dyn OutputInterface>>,
    ) -> anyhow::Result<()> {
        base_command_initialize(self, input, output)
    }

    shirabe_external_packages::delegate_command_trait_impls_to_inner!(base_command_data);
}

impl BaseCommand for FundCommand {
    fn command_data(
        &self,
    ) -> &shirabe_external_packages::symfony::console::command::command::CommandData {
        self.base_command_data.command_data()
    }

    crate::delegate_base_command_trait_impls_to_inner!(base_command_data);
}
