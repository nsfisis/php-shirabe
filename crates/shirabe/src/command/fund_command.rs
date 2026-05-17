//! ref: composer/src/Composer/Command/FundCommand.php

use std::any::Any;

use anyhow::Result;
use indexmap::IndexMap;
use shirabe_external_packages::composer::pcre::preg::Preg;
use shirabe_external_packages::symfony::component::console::command::command::Command;
use shirabe_external_packages::symfony::console::formatter::output_formatter::OutputFormatter;
use shirabe_external_packages::symfony::console::input::input_interface::InputInterface;
use shirabe_external_packages::symfony::console::output::output_interface::OutputInterface;
use shirabe_php_shim::PhpMixed;
use shirabe_semver::constraint::match_all_constraint::MatchAllConstraint;

use crate::command::base_command::BaseCommand;
use crate::composer::Composer;
use crate::console::input::input_option::InputOption;
use crate::io::io_interface::IOInterface;
use crate::json::json_file::JsonFile;
use crate::package::alias_package::AliasPackage;
use crate::package::base_package::BasePackage;
use crate::package::complete_package::CompletePackage;
use crate::repository::composite_repository::CompositeRepository;

#[derive(Debug)]
pub struct FundCommand {
    inner: Command,
    composer: Option<Composer>,
    io: Option<Box<dyn IOInterface>>,
}

impl FundCommand {
    pub fn configure(&mut self) {
        self.inner
            .set_name("fund")
            .set_description("Discover how to help fund the maintenance of your dependencies")
            .set_definition(vec![InputOption::new(
                "format",
                Some(PhpMixed::String("f".to_string())),
                Some(InputOption::VALUE_REQUIRED),
                "Format of the output: text or json",
                Some(PhpMixed::String("text".to_string())),
                vec!["text".to_string(), "json".to_string()],
            )]);
    }

    pub fn execute(
        &self,
        input: &dyn InputInterface,
        _output: &dyn OutputInterface,
    ) -> Result<i64> {
        let composer = self.inner.require_composer()?;

        let repo = composer.get_repository_manager().get_local_repository();
        let remote_repos =
            CompositeRepository::new(composer.get_repository_manager().get_repositories());
        let mut fundings: IndexMap<String, IndexMap<String, Vec<String>>> = IndexMap::new();

        let mut packages_to_load: IndexMap<String, Box<MatchAllConstraint>> = IndexMap::new();
        for package in repo.get_packages() {
            if (package.as_any() as &dyn Any)
                .downcast_ref::<AliasPackage>()
                .is_some()
            {
                continue;
            }
            packages_to_load.insert(
                package.get_name().to_string(),
                Box::new(MatchAllConstraint::new()),
            );
        }

        // load all packages dev versions in parallel
        let result = remote_repos.load_packages(
            &packages_to_load,
            &IndexMap::from([("dev".to_string(), BasePackage::STABILITY_DEV)]),
            &IndexMap::new(),
        )?;

        // collect funding data from default branches
        for package in &result.packages {
            if (package.as_any() as &dyn Any)
                .downcast_ref::<AliasPackage>()
                .is_none()
            {
                // TODO: check for CompleteAliasPackage as well
                if let Some(complete_pkg) =
                    (package.as_any() as &dyn Any).downcast_ref::<CompletePackage>()
                {
                    if complete_pkg.is_default_branch()
                        && !complete_pkg.get_funding().is_empty()
                        && packages_to_load.contains_key(complete_pkg.get_name())
                    {
                        Self::insert_funding_data(&mut fundings, complete_pkg)?;
                        packages_to_load.remove(complete_pkg.get_name());
                    }
                }
            }
        }

        // collect funding from installed packages if none was found in the default branch above
        for package in repo.get_packages() {
            if (package.as_any() as &dyn Any)
                .downcast_ref::<AliasPackage>()
                .is_some()
                || !packages_to_load.contains_key(package.get_name())
            {
                continue;
            }
            // TODO: check for CompleteAliasPackage as well
            if let Some(complete_pkg) =
                (package.as_any() as &dyn Any).downcast_ref::<CompletePackage>()
            {
                if !complete_pkg.get_funding().is_empty() {
                    Self::insert_funding_data(&mut fundings, complete_pkg)?;
                }
            }
        }

        fundings.sort_keys();

        let io = self.inner.get_io();

        let format = input
            .get_option("format")
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
                        OutputFormatter::escape(url),
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
            io.write(&JsonFile::encode(&fundings));
        } else {
            io.write("No funding links were found in your package dependencies. This doesn't mean they don't need your support!");
        }

        Ok(0)
    }

    fn insert_funding_data(
        fundings: &mut IndexMap<String, IndexMap<String, Vec<String>>>,
        package: &CompletePackage,
    ) -> Result<()> {
        let pretty_name = package.get_pretty_name();
        let (vendor, package_name) = pretty_name.split_once('/').unwrap_or(("", pretty_name));

        for funding_option in package.get_funding() {
            let url_val = funding_option.get("url").and_then(|v| v.as_string());
            if url_val.map_or(true, |s| s.is_empty()) {
                continue;
            }
            let mut url = url_val.unwrap().to_string();
            let r#type = funding_option
                .get("type")
                .and_then(|v| v.as_string())
                .unwrap_or("");
            if r#type == "github" {
                if let Ok(Some(matches)) =
                    Preg::is_match_with_indexed_captures(r"^https://github.com/([^/]+)$", &url)
                {
                    if let Some(sponsor) = matches.into_iter().nth(1) {
                        url = format!("https://github.com/sponsors/{}", sponsor);
                    }
                }
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

impl BaseCommand for FundCommand {
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
