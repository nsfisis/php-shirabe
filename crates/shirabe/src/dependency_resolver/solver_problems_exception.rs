//! ref: composer/src/Composer/DependencyResolver/SolverProblemsException.php sha1:19fcbddb76767fa34141021c02fb1646e80e7d8d

use crate::dependency_resolver::{Pool, Problem, Request, Rule};
use crate::repository::RepositorySet;
use crate::util::IniHelper;
use bstr::{BString, ByteSlice, ByteVec};
use shirabe_php_shim::{
    RuntimeException, array_unique, bconcat, impl_php_exception, implode, path_to_bytes,
};

#[derive(Debug)]
pub struct SolverProblemsException {
    inner: RuntimeException,
    problems: Vec<Problem>,
    learned_pool: Vec<Vec<std::rc::Rc<std::cell::RefCell<Rule>>>>,
}

impl SolverProblemsException {
    pub const ERROR_DEPENDENCY_RESOLUTION_FAILED: i64 = 2;

    #[track_caller]
    pub fn new(
        problems: Vec<Problem>,
        learned_pool: Vec<Vec<std::rc::Rc<std::cell::RefCell<Rule>>>>,
    ) -> Self {
        let message = bconcat!(
            "Failed resolving dependencies with ",
            problems.len(),
            " problems, call getPrettyString to get formatted details"
        );
        Self {
            inner: RuntimeException::with_code(message, Self::ERROR_DEPENDENCY_RESOLUTION_FAILED),
            problems,
            learned_pool,
        }
    }

    pub fn get_pretty_string(
        &self,
        repository_set: &RepositorySet,
        request: &Request,
        pool: &mut Pool,
        is_verbose: bool,
        is_dev_extraction: bool,
    ) -> anyhow::Result<BString> {
        let installed_map = request.get_present_map(true)?;
        let mut missing_extensions: Vec<BString> = Vec::new();
        let mut is_caused_by_lock = false;

        let mut problems: Vec<BString> = Vec::new();
        for problem in &self.problems {
            problems.push(bconcat!(
                problem.get_pretty_string(
                    repository_set,
                    request,
                    pool,
                    is_verbose,
                    &installed_map,
                    &self.learned_pool
                )?,
                "\n"
            ));
            missing_extensions.extend(self.get_extension_problems(problem.get_reasons()));
            is_caused_by_lock =
                is_caused_by_lock || problem.is_caused_by_lock(repository_set, request, pool);
        }

        let mut i = 1;
        let mut text = BString::from("\n");
        let mut unique_problems = problems.clone();
        unique_problems.dedup();
        for problem in &unique_problems {
            text.push_str(bconcat!("  Problem ", i as i64, problem));
            i += 1;
        }

        let mut hints: Vec<BString> = Vec::new();
        if !is_dev_extraction
            && (text.contains_str("could not be found")
                || text.contains_str("no matching package found"))
        {
            hints.push(BString::from("Potential causes:\n - A typo in the package name\n - The package is not available in a stable-enough version according to your minimum-stability setting\n   see <https://getcomposer.org/doc/04-schema.md#minimum-stability> for more details.\n - It's a private package and you forgot to add a custom repository to find it\n\nRead <https://getcomposer.org/doc/articles/troubleshooting.md> for further common problems."));
        }

        if !missing_extensions.is_empty() {
            hints.push(self.create_extension_hint(&missing_extensions)?);
        }

        if is_caused_by_lock
            && !is_dev_extraction
            && !request.get_update_allow_transitive_root_dependencies()
        {
            hints.push(BString::from("Use the option --with-all-dependencies (-W) to allow upgrades, downgrades and removals for packages currently locked to specific versions."));
        }

        if text.contains_str("found composer-plugin-api[2.0.0] but it does not match")
            && text.contains_str("- ocramius/package-versions")
        {
            hints.push(BString::from("<warning>ocramius/package-versions only provides support for Composer 2 in 1.8+, which requires PHP 7.4.</warning>\nIf you can not upgrade PHP you can require <info>composer/package-versions-deprecated</info> to resolve this with PHP 7.0+."));
        }

        // class_exists('PHPUnit\Framework\TestCase', false) is always false at runtime
        if text.contains_str("found composer-plugin-api[2.0.0] but it does not match") {
            hints.push(BString::from("You are using Composer 2, which some of your plugins seem to be incompatible with. Make sure you update your plugins or report a plugin-issue to ask them to support Composer 2."));
        }

        if !hints.is_empty() {
            text.push(b'\n');
            text.push_str(implode("\n\n", &hints));
        }

        Ok(text)
    }

    pub fn get_problems(&self) -> &Vec<Problem> {
        &self.problems
    }

    fn create_extension_hint(&self, missing_extensions: &[BString]) -> anyhow::Result<BString> {
        let mut paths = IniHelper::get_all()?;

        if paths.first().is_some_and(|s| s.as_os_str().is_empty()) {
            if paths.len() == 1 {
                return Ok(BString::default());
            }
            paths.remove(0);
        }

        let unique_extensions = array_unique(missing_extensions);
        let ignore_extensions_arguments = implode(
            " ",
            &unique_extensions
                .iter()
                .map(|ext| bconcat!("--ignore-platform-req=", ext))
                .collect::<Vec<_>>(),
        );

        let mut text = BString::from(
            "To enable extensions, verify that they are enabled in your .ini files:\n    - ",
        );
        text.push_str(implode(
            "\n    - ",
            &paths.iter().map(path_to_bytes).collect::<Vec<_>>(),
        ));
        text.push_str("\nYou can also run `php --ini` in a terminal to see which files are used by PHP in CLI mode.");
        text.push_str(bconcat!(
            "\nAlternatively, you can run Composer with `",
            ignore_extensions_arguments,
            "` to temporarily ignore these required extensions."
        ));

        Ok(text)
    }

    fn get_extension_problems(
        &self,
        reason_sets: &indexmap::IndexMap<i64, Vec<std::rc::Rc<std::cell::RefCell<Rule>>>>,
    ) -> Vec<BString> {
        let mut missing_extensions: indexmap::IndexMap<BString, i64> = indexmap::IndexMap::new();
        for reason_set in reason_sets.values() {
            for rule in reason_set {
                let required = rule.borrow().get_required_package();
                if let Some(req) = required
                    && req.starts_with(b"ext-")
                {
                    missing_extensions.insert(req, 1);
                }
            }
        }
        missing_extensions.into_keys().collect()
    }
}

impl_php_exception!(
    SolverProblemsException,
    inner,
    r"Composer\DependencyResolver\SolverProblemsException",
    !Send
);
