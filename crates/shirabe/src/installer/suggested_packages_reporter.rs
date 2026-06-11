//! ref: composer/src/Composer/Installer/SuggestedPackagesReporter.php

use crate::io::IOInterface;
use crate::io::IOInterfaceImmutable;
use crate::package::PackageInterfaceHandle;
use crate::repository::InstalledRepository;
use crate::repository::RepositoryInterface;
use indexmap::IndexMap;
use shirabe_external_packages::composer::pcre::Preg;
use shirabe_external_packages::symfony::console::formatter::OutputFormatter;

#[derive(Debug)]
pub struct SuggestedPackagesReporter {
    suggested_packages: Vec<IndexMap<String, String>>,
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

    pub fn get_packages(&self) -> &Vec<IndexMap<String, String>> {
        &self.suggested_packages
    }

    pub fn add_package(&mut self, source: String, target: String, reason: String) -> &mut Self {
        let mut entry = IndexMap::new();
        entry.insert("source".to_string(), source);
        entry.insert("target".to_string(), target);
        entry.insert("reason".to_string(), reason);
        self.suggested_packages.push(entry);

        self
    }

    pub fn add_suggestions_from_package(&mut self, package: PackageInterfaceHandle) -> &mut Self {
        let source = package.get_pretty_name();
        for (target, reason) in package.get_suggests() {
            self.add_package(source.clone(), target.clone(), reason.clone());
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

        let mut suggesters: IndexMap<String, IndexMap<String, String>> = IndexMap::new();
        let mut suggested: IndexMap<String, IndexMap<String, String>> = IndexMap::new();
        for suggestion in &suggested_packages {
            suggesters
                .entry(suggestion["source"].clone())
                .or_insert_with(IndexMap::new)
                .insert(suggestion["target"].clone(), suggestion["reason"].clone());
            suggested
                .entry(suggestion["target"].clone())
                .or_insert_with(IndexMap::new)
                .insert(suggestion["source"].clone(), suggestion["reason"].clone());
        }
        suggesters.sort_keys();
        suggested.sort_keys();

        // Simple mode
        if mode & Self::MODE_LIST != 0 {
            for name in suggested.keys() {
                self.io.write(&format!("<info>{}</info>", name));
            }

            return Ok(());
        }

        // Grouped by package
        if mode & Self::MODE_BY_PACKAGE != 0 {
            for (suggester, suggestions) in &suggesters {
                self.io
                    .write(&format!("<comment>{}</comment> suggests:", suggester));

                for (suggestion, reason) in suggestions {
                    self.io.write(&format!(
                        " - <info>{}</info>{}",
                        suggestion,
                        if !reason.is_empty() {
                            format!(": {}", self.escape_output(reason))
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
                    suggestion
                ));

                for (suggester, reason) in suggesters {
                    self.io.write(&format!(
                        " - <info>{}</info>{}",
                        suggester,
                        if !reason.is_empty() {
                            format!(": {}", self.escape_output(reason))
                        } else {
                            String::new()
                        }
                    ));
                }
                self.io.write("");
            }
        }

        if let Some(only_dependents_of) = only_dependents_of {
            let all_suggested_packages =
                self.get_filtered_suggestions(installed_repo.as_deref_mut(), None)?;
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
                "<info>{} package suggestions were added by new dependencies, use `composer suggest` to see details.</info>",
                suggested_packages.len()
            ));
        }

        Ok(())
    }

    fn get_filtered_suggestions(
        &self,
        installed_repo: Option<&mut InstalledRepository>,
        only_dependents_of: Option<PackageInterfaceHandle>,
    ) -> anyhow::Result<Vec<IndexMap<String, String>>> {
        let suggested_packages = self.get_packages();
        let mut installed_names: Vec<String> = Vec::new();
        if installed_repo.is_some() && !suggested_packages.is_empty() {
            for package in installed_repo.unwrap().get_packages()? {
                installed_names.extend(package.get_names(true));
            }
        }

        let mut source_filter: Vec<String> = Vec::new();
        if let Some(only_dependents_of) = only_dependents_of {
            let requires = only_dependents_of.get_requires();
            let dev_requires = only_dependents_of.get_dev_requires();
            source_filter = requires
                .values()
                .chain(dev_requires.values())
                .map(|link| link.get_target().to_string())
                .collect();
            source_filter.push(only_dependents_of.get_name());
        }

        let mut suggestions: Vec<IndexMap<String, String>> = Vec::new();
        for suggestion in suggested_packages {
            if installed_names.contains(&suggestion["target"])
                || (!source_filter.is_empty() && !source_filter.contains(&suggestion["source"]))
            {
                continue;
            }

            suggestions.push(suggestion.clone());
        }

        Ok(suggestions)
    }

    fn escape_output(&self, string: &str) -> String {
        OutputFormatter::escape(&self.remove_control_characters(string))
            .expect("OutputFormatter::escape does not fail")
    }

    fn remove_control_characters(&self, string: &str) -> String {
        Preg::replace("/[[:cntrl:]]/", "", &string.replace('\n', " "))
            .unwrap_or_else(|_| string.replace('\n', " "))
    }
}
