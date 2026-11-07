//! ref: composer/vendor/symfony/finder/Finder.php sha1:d1c2c5fe811b22be6bd6858b624e783cf96e6fb3
//!
//! The iterator pipeline of `searchInDirectory()` is reproduced inline here
//! instead of as separate `Iterator\*` classes. Entries are materialized as
//! `PathBuf` (the SplFileInfo replacement); the relative-path information that
//! `RecursiveDirectoryIterator` attaches to each `SplFileInfo` is carried on the
//! private `Entry` struct so the path/exclude filters keep their exact behavior.

use crate::Glob;
use bstr::{BStr, BString, ByteSlice};
use chrono::{DateTime, TimeZone};
use indexmap::IndexSet;
use shirabe_php_shim::{
    GLOB_BRACE, GLOB_ONLYDIR, bformat, file_exists, fileatime, filemtime, glob_with_flags, implode,
    is_dir, loosely_compare, path_from_bytes, path_to_bytes, php_regex, php_str, preg_is_match,
    preg_match, preg_quote, realpath, rtrim_chars, str_replace,
};
use std::path::{Path, PathBuf};

const IGNORE_VCS_FILES: i64 = 1;
const IGNORE_DOT_FILES: i64 = 2;

const ONLY_FILES: i64 = 1;
const ONLY_DIRECTORIES: i64 = 2;

const VCS_PATTERNS: [&BStr; 9] = [
    php_str(b".svn"),
    php_str(b"_svn"),
    php_str(b"CVS"),
    php_str(b"_darcs"),
    php_str(b".arch-params"),
    php_str(b".monotone"),
    php_str(b".bzr"),
    php_str(b".git"),
    php_str(b".hg"),
];

/// Helper trait so `Finder::exclude` accepts both single strings and slices
/// (PHP's variadic / array argument compatibility).
pub trait IntoFinderExclude {
    fn into_exclude(self) -> Vec<BString>;
}
impl IntoFinderExclude for &BStr {
    fn into_exclude(self) -> Vec<BString> {
        vec![self.to_owned()]
    }
}
impl IntoFinderExclude for BString {
    fn into_exclude(self) -> Vec<BString> {
        vec![self]
    }
}
impl IntoFinderExclude for &BString {
    fn into_exclude(self) -> Vec<BString> {
        vec![self.clone()]
    }
}
impl IntoFinderExclude for &[BString] {
    fn into_exclude(self) -> Vec<BString> {
        self.to_vec()
    }
}
impl IntoFinderExclude for &Vec<BString> {
    fn into_exclude(self) -> Vec<BString> {
        self.clone()
    }
}
impl IntoFinderExclude for Vec<BString> {
    fn into_exclude(self) -> Vec<BString> {
        self
    }
}

/// The sort strategy. Mirrors the `$sort` property which is either `false`, an
/// `Iterator\SortableIterator::SORT_BY_*` constant, or a PHP callback.
enum Sort {
    None,
    ByName,
    ByAccessedTime,
    Closure(std::cell::RefCell<Box<dyn FnMut(&PathBuf, &PathBuf) -> i64>>),
}

/// One traversal result, replacing `Symfony\Component\Finder\SplFileInfo`.
struct Entry {
    pathname: PathBuf,
    /// `getRelativePath()`: the directory of the entry relative to the search root.
    relative_path: BString,
    /// `getRelativePathname()`: the full path of the entry relative to the search root.
    relative_pathname: BString,
    /// `getFilename()`: the basename.
    filename: BString,
    depth: i64,
    is_dir: bool,
    is_file: bool,
}

pub struct Finder {
    mode: i64,
    names: Vec<BString>,
    not_names: Vec<BString>,
    exclude: Vec<BString>,
    filters: Vec<std::cell::RefCell<Box<dyn FnMut(&Path) -> bool>>>,
    depths: Vec<(BString, i64)>,
    follow_links: bool,
    reverse_sorting: bool,
    sort: Sort,
    ignore: i64,
    dirs: Vec<BString>,
    dates: Vec<(BString, i64)>,
    paths: Vec<BString>,
    not_paths: Vec<BString>,
}

impl std::fmt::Debug for Finder {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Finder")
            .field("mode", &self.mode)
            .field("names", &self.names)
            .field("not_names", &self.not_names)
            .field("exclude", &self.exclude)
            .field("depths", &self.depths)
            .field("follow_links", &self.follow_links)
            .field("ignore", &self.ignore)
            .field("dirs", &self.dirs)
            .field("dates", &self.dates)
            .field("paths", &self.paths)
            .field("not_paths", &self.not_paths)
            .finish_non_exhaustive()
    }
}

impl Default for Finder {
    fn default() -> Self {
        Self::new()
    }
}

impl Finder {
    pub fn new() -> Self {
        Self {
            mode: 0,
            names: Vec::new(),
            not_names: Vec::new(),
            exclude: Vec::new(),
            filters: Vec::new(),
            depths: Vec::new(),
            follow_links: false,
            reverse_sorting: false,
            sort: Sort::None,
            ignore: IGNORE_VCS_FILES | IGNORE_DOT_FILES,
            dirs: Vec::new(),
            dates: Vec::new(),
            paths: Vec::new(),
            not_paths: Vec::new(),
        }
    }

    pub fn create() -> Self {
        Self::new()
    }

    pub fn files(&mut self) -> &mut Self {
        self.mode = ONLY_FILES;

        self
    }

    pub fn directories(&mut self) -> &mut Self {
        self.mode = ONLY_DIRECTORIES;

        self
    }

    pub fn depth(&mut self, level: i64) -> &mut Self {
        // `NumberComparator` over an integer always yields the `==` operator.
        self.depths.push((BString::from("=="), level));

        self
    }

    pub fn r#in(&mut self, dirs: impl AsRef<Path>) -> &mut Self {
        let dir = path_to_bytes(&dirs);
        let mut resolved_dirs: Vec<BString> = Vec::new();

        if is_dir(dirs.as_ref()) {
            resolved_dirs.push(self.normalize_dir(dir));
        } else {
            // TODO(php-semantics): PHP also passes GLOB_NOSORT, which `glob_with_flags` does not
            // model; the result is sorted below either way.
            // PHP checks defined('GLOB_BRACE'), which depends on the glob() PHP was built with;
            // Shirabe's glob() always supports GLOB_BRACE.
            let flags = GLOB_BRACE | GLOB_ONLYDIR;
            let mut glob = glob_with_flags(dir, flags);
            if !glob.is_empty() {
                glob.sort_by(|a, b| loosely_compare(a, b));
                for g in &glob {
                    resolved_dirs.push(self.normalize_dir(g));
                }
            } else {
                panic!(
                    "The \"{}\" directory does not exist.",
                    dirs.as_ref().display()
                );
            }
        }

        self.dirs.extend(resolved_dirs);

        self
    }

    pub fn filter(&mut self, closure: Box<dyn FnMut(&std::path::Path) -> bool>) -> &mut Self {
        self.filters.push(std::cell::RefCell::new(closure));

        self
    }

    pub fn follow_links(&mut self) -> &mut Self {
        self.follow_links = true;

        self
    }

    pub fn exclude<E: IntoFinderExclude>(&mut self, exclude: E) -> &mut Self {
        self.exclude.extend(exclude.into_exclude());

        self
    }

    pub fn ignore_vcs(&mut self, ignore_vcs: bool) -> &mut Self {
        if ignore_vcs {
            self.ignore |= IGNORE_VCS_FILES;
        } else {
            self.ignore &= !IGNORE_VCS_FILES;
        }

        self
    }

    pub fn ignore_dot_files(&mut self, ignore_dot_files: bool) -> &mut Self {
        if ignore_dot_files {
            self.ignore |= IGNORE_DOT_FILES;
        } else {
            self.ignore &= !IGNORE_DOT_FILES;
        }

        self
    }

    pub fn not_name(&mut self, pattern: &BStr) -> &mut Self {
        self.not_names.push(pattern.to_owned());

        self
    }

    pub fn not_path(&mut self, pattern: &BStr) -> &mut Self {
        self.not_paths.push(pattern.to_owned());

        self
    }

    pub fn name(&mut self, pattern: &BStr) -> &mut Self {
        self.names.push(pattern.to_owned());

        self
    }

    pub fn sort<F>(&mut self, comparator: F) -> &mut Self
    where
        F: FnMut(&PathBuf, &PathBuf) -> i64 + 'static,
    {
        self.sort = Sort::Closure(std::cell::RefCell::new(Box::new(comparator)));

        self
    }

    pub fn sort_by_name(&mut self) -> &mut Self {
        self.sort = Sort::ByName;

        self
    }

    pub fn sort_by_accessed_time(&mut self) -> &mut Self {
        self.sort = Sort::ByAccessedTime;

        self
    }

    /// `date('until '.$date->format('Y-m-d H:i:s'))`, taking the date as a value instead of a
    /// string to parse. Symfony's Finder has no such method.
    pub fn date_until<Tz: TimeZone>(&mut self, date: &DateTime<Tz>) -> &mut Self {
        self.dates.push((BString::from("<"), date.timestamp()));

        self
    }

    pub fn get_iterator(&self) -> FinderIterator {
        FinderIterator {
            items: self.collect_paths(),
            pos: 0,
        }
    }

    pub fn iter(&self) -> impl Iterator<Item = PathBuf> {
        self.get_iterator()
    }

    pub fn len(&self) -> usize {
        self.collect_paths().len()
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    fn normalize_dir(&self, dir: &[u8]) -> BString {
        if dir == b"/" {
            return dir.into();
        }

        let mut dir = rtrim_chars(dir, "/");

        if preg_is_match(php_regex!("#^(ssh2\\.)?s?ftp://#"), &dir) {
            dir.push(b'/');
        }
        dir
    }

    fn collect_paths(&self) -> Vec<PathBuf> {
        if self.dirs.is_empty() {
            panic!("You must call one of in() or append() methods before iterating over a Finder.");
        }

        let mut entries: Vec<Entry> = Vec::new();
        for dir in &self.dirs {
            self.search_in_directory(dir.as_ref(), &mut entries);
        }

        if !matches!(self.sort, Sort::None) || self.reverse_sorting {
            self.apply_sort(&mut entries);
        }

        entries.into_iter().map(|entry| entry.pathname).collect()
    }

    fn search_in_directory(&self, dir: &BStr, out: &mut Vec<Entry>) {
        let mut exclude = self.exclude.clone();
        let mut not_paths = self.not_paths.clone();

        if IGNORE_VCS_FILES == (IGNORE_VCS_FILES & self.ignore) {
            exclude.extend(VCS_PATTERNS.iter().map(|p| BString::from(*p)));
        }

        if IGNORE_DOT_FILES == (IGNORE_DOT_FILES & self.ignore) {
            not_paths.push(BString::from("#(^|/)\\..+(/|$)#"));
        }

        let mut min_depth = 0i64;
        let mut max_depth = i64::MAX;
        for (operator, target) in &self.depths {
            match operator.as_bstr().as_bytes() {
                b">" => min_depth = target + 1,
                b">=" => min_depth = *target,
                b"<" => max_depth = target - 1,
                b"<=" => max_depth = *target,
                _ => {
                    min_depth = *target;
                    max_depth = *target;
                }
            }
        }

        let (excluded_dirs, excluded_pattern) = build_exclude(&exclude);

        let mut raw: Vec<Entry> = Vec::new();
        let root = path_from_bytes(dir);
        self.walk(
            root,
            b"",
            0,
            max_depth,
            &excluded_dirs,
            &excluded_pattern,
            &mut raw,
        );

        let match_names: Vec<BString> = self
            .names
            .iter()
            .map(|p| to_regex_filename(p.as_bstr()))
            .collect();
        let nomatch_names: Vec<BString> = self
            .not_names
            .iter()
            .map(|p| to_regex_filename(p.as_bstr()))
            .collect();
        let match_paths: Vec<BString> = self
            .paths
            .iter()
            .map(|p| to_regex_path(p.as_bstr()))
            .collect();
        let nomatch_paths: Vec<BString> = not_paths
            .iter()
            .map(|p| to_regex_path(p.as_bstr()))
            .collect();

        let has_name_filter = !self.names.is_empty() || !self.not_names.is_empty();
        let has_path_filter = !self.paths.is_empty() || !not_paths.is_empty();

        for entry in raw {
            if entry.depth < min_depth {
                continue;
            }

            if self.mode != 0 {
                if ONLY_DIRECTORIES == (ONLY_DIRECTORIES & self.mode) && entry.is_file {
                    continue;
                }
                if ONLY_FILES == (ONLY_FILES & self.mode) && entry.is_dir {
                    continue;
                }
            }

            if has_name_filter
                && !is_accepted(entry.filename.as_ref(), &match_names, &nomatch_names)
            {
                continue;
            }

            if !self.dates.is_empty() {
                if !file_exists(&entry.pathname) {
                    continue;
                }
                // TODO(error-model): SplFileInfo::getMTime() throws a RuntimeException when stat
                // fails (e.g. the file vanished after file_exists()); it is not propagated here.
                let filedate = filemtime(&entry.pathname).expect("filemtime() failed");
                if !self.dates.iter().all(|(operator, target)| {
                    comparator_test(operator.as_bstr(), filedate, *target)
                }) {
                    continue;
                }
            }

            if !self.filters.is_empty()
                && !self
                    .filters
                    .iter()
                    .all(|filter| (*filter.borrow_mut())(&entry.pathname))
            {
                continue;
            }

            if has_path_filter
                && !is_accepted(
                    entry.relative_pathname.as_ref(),
                    &match_paths,
                    &nomatch_paths,
                )
            {
                continue;
            }

            out.push(entry);
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn walk(
        &self,
        dir: &Path,
        relative_dir: &[u8],
        depth: i64,
        max_depth: i64,
        excluded_dirs: &IndexSet<BString>,
        excluded_pattern: &Option<BString>,
        out: &mut Vec<Entry>,
    ) {
        // `RecursiveDirectoryIterator::SKIP_DOTS` is implicit: read_dir omits "." and "..".
        // TODO(symfony): unreadable directories are skipped here; the SplFileInfo-less,
        // non-fallible iterator signatures cannot surface the AccessDeniedException that PHP
        // throws when ignoreUnreadableDirs is false.
        let read = match std::fs::read_dir(dir) {
            Ok(read) => read,
            Err(_) => return,
        };

        for entry in read {
            let Ok(entry) = entry else { continue };

            let filename = BString::from(path_to_bytes(&entry.file_name()));
            let pathname = entry.path();
            let relative_pathname = if relative_dir.is_empty() {
                filename.clone()
            } else {
                bformat!("{relative_dir}/{filename}")
            };

            let file_type = entry.file_type().ok();
            let is_symlink = file_type.is_some_and(|t| t.is_symlink());
            let (is_dir, is_file) = match file_type {
                Some(file_type) if !is_symlink => (file_type.is_dir(), file_type.is_file()),
                _ => {
                    // PHP's is_dir() and is_file() follow symlinks.
                    let metadata = std::fs::metadata(&pathname).ok();
                    (
                        metadata.as_ref().is_some_and(|m| m.is_dir()),
                        metadata.as_ref().is_some_and(|m| m.is_file()),
                    )
                }
            };

            let entry = Entry {
                pathname: pathname.clone(),
                relative_path: relative_dir.into(),
                relative_pathname: relative_pathname.clone(),
                filename,
                depth,
                is_dir,
                is_file,
            };

            if !exclude_accept(&entry, excluded_dirs, excluded_pattern) {
                continue;
            }

            let can_recurse = is_dir && (self.follow_links || !is_symlink);

            out.push(entry);

            if can_recurse && (max_depth == i64::MAX || depth < max_depth) {
                self.walk(
                    &pathname,
                    &relative_pathname,
                    depth + 1,
                    max_depth,
                    excluded_dirs,
                    excluded_pattern,
                    out,
                );
            }
        }
    }

    fn apply_sort(&self, entries: &mut [Entry]) {
        let order: i64 = if self.reverse_sorting { -1 } else { 1 };

        match &self.sort {
            Sort::None => {
                if self.reverse_sorting {
                    entries.reverse();
                }
            }
            Sort::ByName => {
                entries.sort_by(|a, b| {
                    let pa = realpath(&a.pathname).unwrap_or_else(|| a.pathname.clone());
                    let pb = realpath(&b.pathname).unwrap_or_else(|| b.pathname.clone());
                    apply_order(
                        pa.as_os_str()
                            .as_encoded_bytes()
                            .cmp(pb.as_os_str().as_encoded_bytes()),
                        order,
                    )
                });
            }
            Sort::ByAccessedTime => {
                // TODO(error-model): SplFileInfo::getATime() throws a RuntimeException when stat
                // fails; the sort comparator cannot propagate it.
                entries.sort_by(|a, b| {
                    let atime = |path: &Path| fileatime(path).expect("fileatime() failed");
                    apply_order((atime(&a.pathname) - atime(&b.pathname)).cmp(&0), order)
                });
            }
            Sort::Closure(comparator) => {
                let mut comparator = comparator.borrow_mut();
                entries.sort_by(|a, b| {
                    let result = (*comparator)(&a.pathname, &b.pathname);
                    let result = if self.reverse_sorting {
                        -result
                    } else {
                        result
                    };
                    result.cmp(&0)
                });
            }
        }
    }
}

/// Reproduces `ExcludeDirectoryFilterIterator`'s constructor split between simple
/// directory names and `/`-containing path patterns.
fn build_exclude(directories: &[BString]) -> (IndexSet<BString>, Option<BString>) {
    let mut excluded_dirs = IndexSet::new();
    let mut patterns: Vec<BString> = Vec::new();

    for directory in directories {
        let directory = rtrim_chars(directory, "/");
        // The inner iterator is always recursive, so only `/`-containing names become patterns.
        if directory.contains_str("/") {
            patterns.push(preg_quote(&directory, Some(b'#')));
        } else {
            excluded_dirs.insert(directory);
        }
    }

    let excluded_pattern = if patterns.is_empty() {
        None
    } else {
        Some(bformat!("#(?:^|/)(?:{})(?:/|$)#", implode("|", &patterns)))
    };

    (excluded_dirs, excluded_pattern)
}

/// `ExcludeDirectoryFilterIterator::accept`.
fn exclude_accept(
    entry: &Entry,
    excluded_dirs: &IndexSet<BString>,
    excluded_pattern: &Option<BString>,
) -> bool {
    if entry.is_dir
        && excluded_dirs
            .iter()
            .any(|dir| dir.as_bytes() == entry.filename)
    {
        return false;
    }

    if let Some(pattern) = excluded_pattern {
        let path = if entry.is_dir {
            &entry.relative_pathname
        } else {
            &entry.relative_path
        };
        let path = str_replace("\\", "/", path);

        return !preg_is_match(pattern, &path);
    }

    true
}

/// `FilenameFilterIterator::toRegex`.
fn to_regex_filename(pattern: &BStr) -> BString {
    if is_regex(pattern) {
        pattern.to_owned()
    } else {
        Glob::to_regex(pattern, true, true)
    }
}

/// `PathFilterIterator::toRegex`.
fn to_regex_path(pattern: &BStr) -> BString {
    if is_regex(pattern) {
        pattern.to_owned()
    } else {
        bformat!("/{}/", preg_quote(pattern, Some(b'/')))
    }
}

/// `MultiplePcreFilterIterator::isAccepted`.
fn is_accepted(string: &BStr, match_regexps: &[BString], nomatch_regexps: &[BString]) -> bool {
    for regex in nomatch_regexps {
        if preg_is_match(regex, string) {
            return false;
        }
    }

    if !match_regexps.is_empty() {
        for regex in match_regexps {
            if preg_is_match(regex, string) {
                return true;
            }
        }

        return false;
    }

    true
}

/// `MultiplePcreFilterIterator::isRegex`.
fn is_regex(str: &BStr) -> bool {
    // PHP 8.2+ available modifiers.
    let available_modifiers = "imsxuADUn";

    let pattern = bformat!("/^(.{{3,}}?)[{available_modifiers}]*$/");
    if let Some(matches) = preg_match(&pattern, str) {
        let bytes = matches.get(1).unwrap_or_default();
        let start = &bytes[..bytes.len().min(1)];
        let end = &bytes[bytes.len().saturating_sub(1)..];

        if start == end {
            return !preg_is_match(php_regex!("/[*?[:alnum:] \\\\]/"), start);
        }

        for (open, close) in [("{", "}"), ("(", ")"), ("[", "]"), ("<", ">")] {
            if start == open && end == close {
                return true;
            }
        }
    }

    false
}

/// `Comparator::test`.
fn comparator_test(operator: &BStr, test: i64, target: i64) -> bool {
    match operator.as_bytes() {
        b">" => test > target,
        b">=" => test >= target,
        b"<" => test < target,
        b"<=" => test <= target,
        b"!=" => test != target,
        _ => test == target,
    }
}

fn apply_order(ordering: std::cmp::Ordering, order: i64) -> std::cmp::Ordering {
    if order < 0 {
        ordering.reverse()
    } else {
        ordering
    }
}

#[derive(Debug)]
pub struct FinderIterator {
    items: Vec<PathBuf>,
    pos: usize,
}

impl FinderIterator {
    pub fn valid(&self) -> bool {
        self.pos < self.items.len()
    }

    pub fn current(&self) -> PathBuf {
        self.items[self.pos].clone()
    }
}

impl Iterator for FinderIterator {
    type Item = PathBuf;

    fn next(&mut self) -> Option<PathBuf> {
        if self.pos < self.items.len() {
            let item = self.items[self.pos].clone();
            self.pos += 1;
            Some(item)
        } else {
            None
        }
    }
}

impl IntoIterator for &Finder {
    type Item = PathBuf;
    type IntoIter = std::vec::IntoIter<PathBuf>;

    fn into_iter(self) -> Self::IntoIter {
        self.collect_paths().into_iter()
    }
}

impl IntoIterator for Finder {
    type Item = PathBuf;
    type IntoIter = std::vec::IntoIter<PathBuf>;

    fn into_iter(self) -> Self::IntoIter {
        self.collect_paths().into_iter()
    }
}

impl IntoIterator for &mut Finder {
    type Item = PathBuf;
    type IntoIter = std::vec::IntoIter<PathBuf>;

    fn into_iter(self) -> Self::IntoIter {
        self.collect_paths().into_iter()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use shirabe_php_shim::{time, touch_with_mtime};

    fn relative_names(finder: &Finder, root: &Path) -> Vec<BString> {
        let mut names: Vec<BString> = finder
            .iter()
            .map(|path| BString::from(path_to_bytes(path.strip_prefix(root).unwrap())))
            .collect();
        names.sort();
        names
    }

    #[test]
    fn in_expands_wildcards_and_braces() {
        let tmp = tempfile::tempdir().unwrap();
        for dir in ["x/a", "x/b", "x/c"] {
            std::fs::create_dir_all(tmp.path().join(dir)).unwrap();
        }
        for file in ["x/a/1", "x/b/2", "x/c/3"] {
            std::fs::write(tmp.path().join(file), b"").unwrap();
        }

        let mut finder = Finder::create();
        finder.files().r#in(tmp.path().join("x/{a,c}"));
        assert_eq!(
            relative_names(&finder, tmp.path()),
            vec![BString::from("x/a/1"), BString::from("x/c/3")]
        );

        let mut finder = Finder::create();
        finder.files().r#in(tmp.path().join("x/[ab]"));
        assert_eq!(
            relative_names(&finder, tmp.path()),
            vec![BString::from("x/a/1"), BString::from("x/b/2")]
        );
    }

    #[test]
    #[should_panic(expected = "directory does not exist")]
    fn in_rejects_a_wildcard_matching_no_directory() {
        let tmp = tempfile::tempdir().unwrap();
        std::fs::write(tmp.path().join("file"), b"").unwrap();
        Finder::create().r#in(tmp.path().join("fi*"));
    }

    #[test]
    fn date_until_keeps_entries_older_than_the_date() {
        let tmp = tempfile::tempdir().unwrap();
        std::fs::write(tmp.path().join("old"), b"").unwrap();
        std::fs::write(tmp.path().join("new"), b"").unwrap();
        assert!(touch_with_mtime(
            tmp.path().join("old"),
            time() - 3 * 86_400
        ));

        let mut finder = Finder::create();
        finder
            .files()
            .r#in(tmp.path())
            .date_until(&(chrono::Utc::now() - chrono::Duration::days(1)));
        assert_eq!(
            relative_names(&finder, tmp.path()),
            vec![BString::from("old")]
        );
    }
}
