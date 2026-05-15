//! ref: composer/src/Composer/DependencyResolver/SolverProblemsException.php

use shirabe_php_shim::RuntimeException;

use crate::dependency_resolver::pool::Pool;
use crate::dependency_resolver::problem::Problem;
use crate::dependency_resolver::request::Request;
use crate::dependency_resolver::rule::Rule;
use crate::repository::repository_set::RepositorySet;
use crate::util::ini_helper::IniHelper;

#[derive(Debug)]
pub struct SolverProblemsException {
    inner: RuntimeException,
    pub(crate) problems: Vec<Problem>,
    pub(crate) learned_pool: Vec<Vec<Rule>>,
}

impl SolverProblemsException {
    pub const ERROR_DEPENDENCY_RESOLUTION_FAILED: i64 = 2;

    pub fn new(problems: Vec<Problem>, learned_pool: Vec<Vec<Rule>>) -> Self {
        let message = format!(
            "Failed resolving dependencies with {} problems, call getPrettyString to get formatted details",
            problems.len()
        );
        Self {
            inner: RuntimeException {
                message,
                code: Self::ERROR_DEPENDENCY_RESOLUTION_FAILED,
            },
            problems,
            learned_pool,
        }
    }

    pub fn get_pretty_string(
        &self,
        repository_set: &RepositorySet,
        request: &Request,
        pool: &Pool,
        is_verbose: bool,
        is_dev_extraction: bool,
    ) -> String {
        let installed_map = request.get_present_map(true);
        let mut missing_extensions: Vec<String> = Vec::new();
        let mut is_caused_by_lock = false;

        let mut problems: Vec<String> = Vec::new();
        for problem in &self.problems {
            problems.push(format!(
                "{}\n",
                problem.get_pretty_string(repository_set, request, pool, is_verbose, &installed_map, &self.learned_pool)
            ));
            missing_extensions.extend(self.get_extension_problems(problem.get_reasons()));
            is_caused_by_lock = is_caused_by_lock || problem.is_caused_by_lock(repository_set, request, pool);
        }

        let mut i = 1;
        let mut text = "\n".to_string();
        let mut unique_problems = problems.clone();
        unique_problems.dedup();
        for problem in &unique_problems {
            text.push_str(&format!("  Problem {}{}", i, problem));
            i += 1;
        }

        let mut hints: Vec<String> = Vec::new();
        if !is_dev_extraction && (text.contains("could not be found") || text.contains("no matching package found")) {
            hints.push("Potential causes:\n - A typo in the package name\n - The package is not available in a stable-enough version according to your minimum-stability setting\n   see <https://getcomposer.org/doc/04-schema.md#minimum-stability> for more details.\n - It's a private package and you forgot to add a custom repository to find it\n\nRead <https://getcomposer.org/doc/articles/troubleshooting.md> for further common problems.".to_string());
        }

        if !missing_extensions.is_empty() {
            hints.push(self.create_extension_hint(&missing_extensions));
        }

        if is_caused_by_lock && !is_dev_extraction && !request.get_update_allow_transitive_root_dependencies() {
            hints.push("Use the option --with-all-dependencies (-W) to allow upgrades, downgrades and removals for packages currently locked to specific versions.".to_string());
        }

        if text.contains("found composer-plugin-api[2.0.0] but it does not match") && text.contains("- ocramius/package-versions") {
            hints.push("<warning>ocramius/package-versions only provides support for Composer 2 in 1.8+, which requires PHP 7.4.</warning>\nIf you can not upgrade PHP you can require <info>composer/package-versions-deprecated</info> to resolve this with PHP 7.0+.".to_string());
        }

        // class_exists('PHPUnit\Framework\TestCase', false) is always false at runtime
        if text.contains("found composer-plugin-api[2.0.0] but it does not match") {
            hints.push("You are using Composer 2, which some of your plugins seem to be incompatible with. Make sure you update your plugins or report a plugin-issue to ask them to support Composer 2.".to_string());
        }

        if !hints.is_empty() {
            text.push('\n');
            text.push_str(&hints.join("\n\n"));
        }

        text
    }

    pub fn get_problems(&self) -> &Vec<Problem> {
        &self.problems
    }

    fn create_extension_hint(&self, missing_extensions: &[String]) -> String {
        let mut paths = IniHelper::get_all();

        if paths.first().map_or(false, |s| s.is_empty()) {
            if paths.len() == 1 {
                return String::new();
            }
            paths.remove(0);
        }

        let mut unique_extensions: Vec<String> = missing_extensions.to_vec();
        unique_extensions.sort();
        unique_extensions.dedup();
        let ignore_extensions_arguments: String = unique_extensions
            .iter()
            .map(|ext| format!("--ignore-platform-req={}", ext))
            .collect::<Vec<_>>()
            .join(" ");

        let mut text = "To enable extensions, verify that they are enabled in your .ini files:\n    - ".to_string();
        text.push_str(&paths.join("\n    - "));
        text.push_str("\nYou can also run `php --ini` in a terminal to see which files are used by PHP in CLI mode.");
        text.push_str(&format!("\nAlternatively, you can run Composer with `{}` to temporarily ignore these required extensions.", ignore_extensions_arguments));

        text
    }

    fn get_extension_problems(&self, reason_sets: Vec<Vec<Rule>>) -> Vec<String> {
        let mut missing_extensions: indexmap::IndexMap<String, i64> = indexmap::IndexMap::new();
        for reason_set in reason_sets {
            for rule in reason_set {
                let required = rule.get_required_package();
                if let Some(req) = required {
                    if req.starts_with("ext-") {
                        missing_extensions.insert(req.to_string(), 1);
                    }
                }
            }
        }
        missing_extensions.into_keys().collect()
    }
}

impl std::fmt::Display for SolverProblemsException {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.inner.message)
    }
}

impl std::error::Error for SolverProblemsException {}
