//! ref: composer/src/Composer/Package/Archiver/ArchivableFilesFinder.php

use crate::package::archiver::{BaseExcludeFilter, ComposerExcludeFilter, GitExcludeFilter};
use crate::util::Filesystem;
use bstr::{BStr, BString};
use shirabe_php_shim::{RuntimeException, path_to_bytes, preg_quote, preg_replace, realpath};
use shirabe_symfony_finder::Finder;
use std::path::{Path, PathBuf};

pub struct ArchivableFilesFinder {
    finder: Finder,
    inner_iter: Box<dyn Iterator<Item = PathBuf>>,
}

impl std::fmt::Debug for ArchivableFilesFinder {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ArchivableFilesFinder")
            .field("finder", &self.finder)
            .finish()
    }
}

impl ArchivableFilesFinder {
    pub fn new(
        sources: impl AsRef<Path>,
        excludes: Vec<BString>,
        ignore_filters: bool,
    ) -> anyhow::Result<Self> {
        let sources = sources.as_ref();
        let fs = Filesystem::new(None);

        let sources_real_path = realpath(sources);
        if sources_real_path.is_none() {
            return Err(RuntimeException::new(format!(
                "Could not realpath() the source directory \"{}\"",
                sources.display()
            ))
            .into());
        }
        let sources = fs.normalize_path(sources_real_path.unwrap());

        let filters: Vec<Box<dyn BaseExcludeFilter>> = if ignore_filters {
            vec![]
        } else {
            vec![
                Box::new(GitExcludeFilter::new(sources.clone())),
                Box::new(ComposerExcludeFilter::new(sources.clone(), excludes)),
            ]
        };

        let mut finder = Finder::new();

        let sources_clone = sources.clone();
        let filter = move |file: &Path| -> bool {
            let Ok(realpath) = file.canonicalize() else {
                return false;
            };
            if file.is_symlink()
                && !path_to_bytes(&realpath).starts_with(path_to_bytes(&sources_clone))
            {
                return false;
            }

            let relative_path = preg_replace(
                format!(
                    "#^{}#",
                    preg_quote(path_to_bytes(&sources_clone), Some('#'))
                ),
                "",
                path_to_bytes(&fs.normalize_path(&realpath)),
            );

            let mut exclude = false;
            for f in &filters {
                exclude = f.filter(BStr::new(&relative_path), exclude);
            }

            !exclude
        };

        finder
            .r#in(&sources)
            .filter(Box::new(filter))
            .ignore_vcs(true)
            .ignore_dot_files(false)
            .sort_by_name();

        let inner_iter: Box<dyn Iterator<Item = PathBuf>> = Box::new(finder.get_iterator());

        Ok(Self { finder, inner_iter })
    }

    pub fn accept(&self, current: &Path) -> bool {
        if !current.is_dir() {
            return true;
        }

        match std::fs::read_dir(current) {
            Ok(mut iter) => iter.next().is_none(),
            Err(_) => false,
        }
    }
}

impl Iterator for ArchivableFilesFinder {
    type Item = PathBuf;

    fn next(&mut self) -> Option<Self::Item> {
        loop {
            let item = self.inner_iter.next()?;
            if self.accept(&item) {
                return Some(item);
            }
        }
    }
}
