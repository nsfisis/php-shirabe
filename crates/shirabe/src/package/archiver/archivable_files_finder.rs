//! ref: composer/src/Composer/Package/Archiver/ArchivableFilesFinder.php

use crate::package::archiver::composer_exclude_filter::ComposerExcludeFilter;
use crate::package::archiver::git_exclude_filter::GitExcludeFilter;
use crate::util::filesystem::Filesystem;
use shirabe_external_packages::composer::pcre::preg::Preg;
use shirabe_external_packages::symfony::component::finder::finder::Finder;
use shirabe_external_packages::symfony::component::finder::spl_file_info::SplFileInfo;
use shirabe_php_shim::{RuntimeException, preg_quote, realpath};

pub struct ArchivableFilesFinder {
    pub(crate) finder: Finder,
    inner_iter: Box<dyn Iterator<Item = SplFileInfo>>,
}

impl std::fmt::Debug for ArchivableFilesFinder {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ArchivableFilesFinder")
            .field("finder", &self.finder)
            .finish()
    }
}

impl ArchivableFilesFinder {
    pub fn new(sources: &str, excludes: Vec<String>, ignore_filters: bool) -> anyhow::Result<Self> {
        let fs = Filesystem::new();

        let sources_real_path = realpath(sources);
        if sources_real_path.is_none() {
            return Err(RuntimeException {
                message: format!("Could not realpath() the source directory \"{}\"", sources),
                code: 0,
            }
            .into());
        }
        let sources = fs.normalize_path(&sources_real_path.unwrap());

        let filters: Vec<Box<dyn ArchivableFilesFilter>> = if ignore_filters {
            vec![]
        } else {
            vec![
                Box::new(GitExcludeFilter::new(&sources)),
                Box::new(ComposerExcludeFilter::new(&sources, excludes)),
            ]
        };

        let mut finder = Finder::new();

        let sources_clone = sources.clone();
        let filter = move |file: &SplFileInfo| -> bool {
            let realpath = file.get_real_path();
            if realpath.is_none() {
                return false;
            }
            let realpath = realpath.unwrap();
            if file.is_link() && !realpath.starts_with(sources_clone.as_str()) {
                return false;
            }

            let relative_path = Preg::replace(
                &format!("^{}", preg_quote(&sources_clone, Some('#'))),
                "",
                &fs.normalize_path(&realpath),
            );

            let mut exclude = false;
            for f in &filters {
                exclude = f.filter(&relative_path, exclude);
            }

            !exclude
        };

        finder
            .in_dir(&sources)
            .filter(Box::new(filter))
            .ignore_vcs(true)
            .ignore_dot_files(false)
            .sort_by_name();

        let inner_iter = finder.get_iterator();

        Ok(Self { finder, inner_iter })
    }

    pub fn accept(&self, current: &SplFileInfo) -> bool {
        if !current.is_dir() {
            return true;
        }

        let path = current.to_string();
        match std::fs::read_dir(&path) {
            Ok(mut iter) => iter.next().is_none(),
            Err(_) => false,
        }
    }
}

trait ArchivableFilesFilter {
    fn filter(&self, relative_path: &str, exclude: bool) -> bool;
}

impl ArchivableFilesFilter for GitExcludeFilter {
    fn filter(&self, relative_path: &str, exclude: bool) -> bool {
        self.filter(relative_path, exclude)
    }
}

impl ArchivableFilesFilter for ComposerExcludeFilter {
    fn filter(&self, relative_path: &str, exclude: bool) -> bool {
        self.filter(relative_path, exclude)
    }
}

impl Iterator for ArchivableFilesFinder {
    type Item = SplFileInfo;

    fn next(&mut self) -> Option<Self::Item> {
        loop {
            let item = self.inner_iter.next()?;
            if self.accept(&item) {
                return Some(item);
            }
        }
    }
}
