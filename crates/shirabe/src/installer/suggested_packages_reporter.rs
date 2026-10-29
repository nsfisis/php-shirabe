//! ref: composer/src/Composer/Installer/SuggestedPackagesReporter.php

use crate::io::{IOInterface, IOInterfaceImmutable};
use crate::package::PackageInterfaceHandle;
use crate::repository::{InstalledRepository, RepositoryInterface};
use bstr::BString;
use indexmap::IndexMap;
use shirabe_php_shim::{legacy_lossy, php_regex, preg_replace};
use shirabe_symfony_console::formatter::OutputFormatter;

#[derive(Debug)]
pub struct SuggestedPackagesReporter {
    suggested_packages: Vec<IndexMap<BString, BString>>,
    io: std::rc::Rc<std::cell::RefCell<dyn IOInterface>>,
}

impl SuggestedPackagesReporter {
    pub const MODE_LIST: i64 = 1;
    pub const MODE_BY_PACKAGE: i64 = 2;
    pub const MODE_BY_SUGGESTION: i64 = 4;

    pub fn new(io: std::rc::Rc<std::cell::RefCell<dyn IOInterface>>) -> Self {
        Self {
            suggested_packages: Vec::new(),
            io,
        }
    }

    pub fn get_packages(&self) -> &Vec<IndexMap<BString, BString>> {
        &self.suggested_packages
    }

    pub fn add_package(&mut self, source: BString, target: BString, reason: BString) -> &mut Self {
        let mut entry = IndexMap::new();
        entry.insert(BString::from("source"), source);
        entry.insert(BString::from("target"), target);
        entry.insert(BString::from("reason"), reason);
        self.suggested_packages.push(entry);

        self
    }

    pub fn add_suggestions_from_package(&mut self, package: PackageInterfaceHandle) -> &mut Self {
        let source = package.get_pretty_name();
        for (target, reason) in package.get_suggests() {
            self.add_package(source.clone(), target, reason);
        }

        self
    }

    pub fn output(
        &self,
        mode: i64,
        mut installed_repo: Option<&mut InstalledRepository>,
        only_dependents_of: Option<PackageInterfaceHandle>,
    ) -> anyhow::Result<()> {
        let suggested_packages = self
            .get_filtered_suggestions(installed_repo.as_deref_mut(), only_dependents_of.clone())?;

        let mut suggesters: IndexMap<BString, IndexMap<BString, BString>> = IndexMap::new();
        let mut suggested: IndexMap<BString, IndexMap<BString, BString>> = IndexMap::new();
        for suggestion in &suggested_packages {
            suggesters
                .entry(suggestion[b"source".as_slice()].clone())
                .or_default()
                .insert(
                    suggestion[b"target".as_slice()].clone(),
                    suggestion[b"reason".as_slice()].clone(),
                );
            suggested
                .entry(suggestion[b"target".as_slice()].clone())
                .or_default()
                .insert(
                    suggestion[b"source".as_slice()].clone(),
                    suggestion[b"reason".as_slice()].clone(),
                );
        }
        suggesters.sort_keys();
        suggested.sort_keys();

        // Simple mode
        if mode & Self::MODE_LIST != 0 {
            for name in suggested.keys() {
                self.io
                    .write(&format!("<info>{}</info>", legacy_lossy(&name)));
            }

            return Ok(());
        }

        // Grouped by package
        if mode & Self::MODE_BY_PACKAGE != 0 {
            for (suggester, suggestions) in &suggesters {
                self.io.write(&format!(
                    "<comment>{}</comment> suggests:",
                    legacy_lossy(&suggester)
                ));

                for (suggestion, reason) in suggestions {
                    self.io.write(&format!(
                        " - <info>{}</info>{}",
                        legacy_lossy(&suggestion),
                        if !reason.is_empty() {
                            format!(": {}", self.escape_output(&legacy_lossy(reason)))
                        } else {
                            String::new()
                        }
                    ));
                }
                self.io.write("");
            }
        }

        // Grouped by suggestion
        if mode & Self::MODE_BY_SUGGESTION != 0 {
            // Improve readability in full mode
            if mode & Self::MODE_BY_PACKAGE != 0 {
                self.io.write(&"-".repeat(78));
            }
            for (suggestion, suggesters) in &suggested {
                self.io.write(&format!(
                    "<comment>{}</comment> is suggested by:",
                    legacy_lossy(&suggestion)
                ));

                for (suggester, reason) in suggesters {
                    self.io.write(&format!(
                        " - <info>{}</info>{}",
                        legacy_lossy(&suggester),
                        if !reason.is_empty() {
                            format!(": {}", self.escape_output(&legacy_lossy(reason)))
                        } else {
                            String::new()
                        }
                    ));
                }
                self.io.write("");
            }
        }

        if let Some(only_dependents_of) = only_dependents_of {
            let all_suggested_packages = self.get_filtered_suggestions(installed_repo, None)?;
            let diff = all_suggested_packages.len() as i64 - suggested_packages.len() as i64;
            if diff != 0 {
                self.io.write(&format!("<info>{} additional suggestions</info> by transitive dependencies can be shown with <info>--all</info>", diff));
            }
        }

        Ok(())
    }

    pub fn output_minimalistic(
        &self,
        installed_repo: Option<&mut InstalledRepository>,
        only_dependents_of: Option<PackageInterfaceHandle>,
    ) -> anyhow::Result<()> {
        let suggested_packages =
            self.get_filtered_suggestions(installed_repo, only_dependents_of)?;
        if !suggested_packages.is_empty() {
            self.io.write_error(&format!(
                "<info>{} package suggestions were added by new dependencies, use `shirabe suggest` to see details.</info>",
                suggested_packages.len()
            ));
        }

        Ok(())
    }

    fn get_filtered_suggestions(
        &self,
        installed_repo: Option<&mut InstalledRepository>,
        only_dependents_of: Option<PackageInterfaceHandle>,
    ) -> anyhow::Result<Vec<IndexMap<BString, BString>>> {
        let suggested_packages = self.get_packages();
        let mut installed_names: Vec<BString> = Vec::new();
        if let Some(installed_repo) = installed_repo
            && !suggested_packages.is_empty()
        {
            for package in installed_repo.get_packages()? {
                installed_names.extend(package.get_names(true));
            }
        }

        let mut source_filter: Vec<BString> = Vec::new();
        if let Some(only_dependents_of) = only_dependents_of {
            let requires = only_dependents_of.get_requires();
            let dev_requires = only_dependents_of.get_dev_requires();
            source_filter = requires
                .values()
                .chain(dev_requires.values())
                .map(|link| BString::from(link.get_target()))
                .collect();
            source_filter.push(only_dependents_of.get_name());
        }

        let mut suggestions: Vec<IndexMap<BString, BString>> = Vec::new();
        for suggestion in suggested_packages {
            if installed_names.contains(&suggestion[b"target".as_slice()])
                || (!source_filter.is_empty()
                    && !source_filter.contains(&suggestion[b"source".as_slice()]))
            {
                continue;
            }

            suggestions.push(suggestion.clone());
        }

        Ok(suggestions)
    }

    fn escape_output(&self, string: &str) -> String {
        legacy_lossy(
            &OutputFormatter::escape(self.remove_control_characters(string))
                .expect("OutputFormatter::escape does not fail"),
        )
        .into_owned()
    }

    fn remove_control_characters(&self, string: &str) -> String {
        legacy_lossy(&preg_replace(
            php_regex!("/[[:cntrl:]]/"),
            "",
            string.replace('\n', " "),
        ))
        .into_owned()
    }
}
