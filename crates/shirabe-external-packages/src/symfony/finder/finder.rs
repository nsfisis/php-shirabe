//! ref: composer/vendor/symfony/finder/Finder.php
//!
//! The iterator pipeline of `searchInDirectory()` is reproduced inline here
//! instead of as separate `Iterator\*` classes. Entries are materialized as
//! `PathBuf` (the SplFileInfo replacement); the relative-path information that
//! `RecursiveDirectoryIterator` attaches to each `SplFileInfo` is carried on the
//! private `Entry` struct so the path/exclude filters keep their exact behavior.

use crate::composer::pcre::{CaptureKey, Preg};
use crate::symfony::finder::glob::Glob;
use chrono::{NaiveDate, NaiveDateTime};
use indexmap::{IndexMap, IndexSet};
use shirabe_php_shim::{file_exists, glob, is_dir, preg_quote, rtrim};
use std::path::{Path, PathBuf};
use std::time::UNIX_EPOCH;

const IGNORE_VCS_FILES: i64 = 1;
const IGNORE_DOT_FILES: i64 = 2;

const ONLY_FILES: i64 = 1;
const ONLY_DIRECTORIES: i64 = 2;

const VCS_PATTERNS: [&str; 9] = [
    ".svn",
    "_svn",
    "CVS",
    "_darcs",
    ".arch-params",
    ".monotone",
    ".bzr",
    ".git",
    ".hg",
];

/// Helper trait so `Finder::exclude` accepts both single strings and slices
/// (PHP's variadic / array argument compatibility).
pub trait IntoFinderExclude {
    fn into_exclude(self) -> Vec<String>;
}
impl IntoFinderExclude for &str {
    fn into_exclude(self) -> Vec<String> {
        vec![self.to_string()]
    }
}
impl IntoFinderExclude for String {
    fn into_exclude(self) -> Vec<String> {
        vec![self]
    }
}
impl IntoFinderExclude for &String {
    fn into_exclude(self) -> Vec<String> {
        vec![self.clone()]
    }
}
impl IntoFinderExclude for &[String] {
    fn into_exclude(self) -> Vec<String> {
        self.to_vec()
    }
}
impl IntoFinderExclude for &Vec<String> {
    fn into_exclude(self) -> Vec<String> {
        self.clone()
    }
}
impl IntoFinderExclude for Vec<String> {
    fn into_exclude(self) -> Vec<String> {
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
    relative_path: String,
    /// `getRelativePathname()`: the full path of the entry relative to the search root.
    relative_pathname: String,
    /// `getFilename()`: the basename.
    filename: String,
    depth: i64,
    is_dir: bool,
    is_file: bool,
}

pub struct Finder {
    mode: i64,
    names: Vec<String>,
    not_names: Vec<String>,
    exclude: Vec<String>,
    filters: Vec<std::cell::RefCell<Box<dyn FnMut(&Path) -> bool>>>,
    depths: Vec<(String, i64)>,
    follow_links: bool,
    reverse_sorting: bool,
    sort: Sort,
    ignore: i64,
    dirs: Vec<String>,
    dates: Vec<(String, i64)>,
    paths: Vec<String>,
    not_paths: Vec<String>,
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
        self.depths.push(("==".to_string(), level));

        self
    }

    pub fn r#in(&mut self, dirs: impl AsRef<Path>) -> &mut Self {
        let dir = dirs.as_ref().to_string_lossy().into_owned();
        let mut resolved_dirs: Vec<String> = Vec::new();

        if is_dir(&dir) {
            resolved_dirs.push(self.normalize_dir(&dir));
        } else {
            // GLOB_ONLYDIR is emulated by retaining directory matches only.
            // TODO(phase-c): wildcard `in()` paths depend on `shirabe_php_shim::glob`, which is
            // still `todo!()`; only the real-directory branch above currently resolves.
            let mut globbed: Vec<String> =
                glob(&dir).into_iter().filter(|path| is_dir(path)).collect();
            if !globbed.is_empty() {
                globbed.sort();
                for g in &globbed {
                    resolved_dirs.push(self.normalize_dir(g));
                }
            } else {
                panic!("The \"{dir}\" directory does not exist.");
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

    pub fn not_name(&mut self, pattern: &str) -> &mut Self {
        self.not_names.push(pattern.to_string());

        self
    }

    pub fn not_path(&mut self, pattern: &str) -> &mut Self {
        self.not_paths.push(pattern.to_string());

        self
    }

    pub fn name(&mut self, pattern: &str) -> &mut Self {
        self.names.push(pattern.to_string());

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

    pub fn date(&mut self, date: &str) -> &mut Self {
        self.dates.push(parse_date_comparator(date));

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

    fn normalize_dir(&self, dir: &str) -> String {
        if dir == "/" {
            return dir.to_string();
        }

        let dir = rtrim(dir, Some("/"));

        if Preg::is_match("#^(ssh2\\.)?s?ftp://#", &dir) {
            format!("{dir}/")
        } else {
            dir
        }
    }

    fn collect_paths(&self) -> Vec<PathBuf> {
        if self.dirs.is_empty() {
            panic!("You must call one of in() or append() methods before iterating over a Finder.");
        }

        let mut entries: Vec<Entry> = Vec::new();
        for dir in &self.dirs {
            self.search_in_directory(dir, &mut entries);
        }

        if !matches!(self.sort, Sort::None) || self.reverse_sorting {
            self.apply_sort(&mut entries);
        }

        entries.into_iter().map(|entry| entry.pathname).collect()
    }

    fn search_in_directory(&self, dir: &str, out: &mut Vec<Entry>) {
        let mut exclude = self.exclude.clone();
        let mut not_paths = self.not_paths.clone();

        if IGNORE_VCS_FILES == (IGNORE_VCS_FILES & self.ignore) {
            exclude.extend(VCS_PATTERNS.iter().map(|p| p.to_string()));
        }

        if IGNORE_DOT_FILES == (IGNORE_DOT_FILES & self.ignore) {
            not_paths.push("#(^|/)\\..+(/|$)#".to_string());
        }

        let mut min_depth = 0i64;
        let mut max_depth = i64::MAX;
        for (operator, target) in &self.depths {
            match operator.as_str() {
                ">" => min_depth = target + 1,
                ">=" => min_depth = *target,
                "<" => max_depth = target - 1,
                "<=" => max_depth = *target,
                _ => {
                    min_depth = *target;
                    max_depth = *target;
                }
            }
        }

        let (excluded_dirs, excluded_pattern) = build_exclude(&exclude);

        let mut raw: Vec<Entry> = Vec::new();
        let root = Path::new(dir);
        self.walk(
            root,
            "",
            0,
            max_depth,
            &excluded_dirs,
            &excluded_pattern,
            &mut raw,
        );

        let match_names: Vec<String> = self.names.iter().map(|p| to_regex_filename(p)).collect();
        let nomatch_names: Vec<String> = self
            .not_names
            .iter()
            .map(|p| to_regex_filename(p))
            .collect();
        let match_paths: Vec<String> = self.paths.iter().map(|p| to_regex_path(p)).collect();
        let nomatch_paths: Vec<String> = not_paths.iter().map(|p| to_regex_path(p)).collect();

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

            if has_name_filter && !is_accepted(&entry.filename, &match_names, &nomatch_names) {
                continue;
            }

            if !self.dates.is_empty() {
                if !file_exists(&entry.pathname) {
                    continue;
                }
                let filedate = mtime(&entry.pathname);
                if !self
                    .dates
                    .iter()
                    .all(|(operator, target)| comparator_test(operator, filedate, *target))
                {
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
                && !is_accepted(&entry.relative_pathname, &match_paths, &nomatch_paths)
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
        relative_dir: &str,
        depth: i64,
        max_depth: i64,
        excluded_dirs: &IndexSet<String>,
        excluded_pattern: &Option<String>,
        out: &mut Vec<Entry>,
    ) {
        // `RecursiveDirectoryIterator::SKIP_DOTS` is implicit: read_dir omits "." and "..".
        // TODO(phase-c): unreadable directories are skipped here; the SplFileInfo-less,
        // non-fallible iterator signatures cannot surface the AccessDeniedException that PHP
        // throws when ignoreUnreadableDirs is false.
        let read = match std::fs::read_dir(dir) {
            Ok(read) => read,
            Err(_) => return,
        };

        for entry in read {
            let entry = match entry {
                Ok(entry) => entry,
                Err(_) => continue,
            };

            let filename = entry.file_name().to_string_lossy().into_owned();
            let pathname = entry.path();
            let relative_pathname = if relative_dir.is_empty() {
                filename.clone()
            } else {
                format!("{relative_dir}/{filename}")
            };

            let metadata = std::fs::metadata(&pathname);
            let is_dir = metadata.as_ref().map(|m| m.is_dir()).unwrap_or(false);
            let is_file = metadata.as_ref().map(|m| m.is_file()).unwrap_or(false);

            let entry = Entry {
                pathname: pathname.clone(),
                relative_path: relative_dir.to_string(),
                relative_pathname: relative_pathname.clone(),
                filename,
                depth,
                is_dir,
                is_file,
            };

            if !exclude_accept(&entry, excluded_dirs, excluded_pattern) {
                continue;
            }

            let is_symlink = std::fs::symlink_metadata(&pathname)
                .map(|m| m.file_type().is_symlink())
                .unwrap_or(false);
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
                    let pa = realpath_or_pathname(&a.pathname);
                    let pb = realpath_or_pathname(&b.pathname);
                    apply_order(pa.as_bytes().cmp(pb.as_bytes()), order)
                });
            }
            Sort::ByAccessedTime => {
                entries.sort_by(|a, b| {
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
fn build_exclude(directories: &[String]) -> (IndexSet<String>, Option<String>) {
    let mut excluded_dirs = IndexSet::new();
    let mut patterns: Vec<String> = Vec::new();

    for directory in directories {
        let directory = rtrim(directory, Some("/"));
        // The inner iterator is always recursive, so only `/`-containing names become patterns.
        if directory.contains('/') {
            patterns.push(preg_quote(&directory, Some('#')));
        } else {
            excluded_dirs.insert(directory);
        }
    }

    let excluded_pattern = if patterns.is_empty() {
        None
    } else {
        Some(format!("#(?:^|/)(?:{})(?:/|$)#", patterns.join("|")))
    };

    (excluded_dirs, excluded_pattern)
}

/// `ExcludeDirectoryFilterIterator::accept`.
fn exclude_accept(
    entry: &Entry,
    excluded_dirs: &IndexSet<String>,
    excluded_pattern: &Option<String>,
) -> bool {
    if excluded_dirs.contains(&entry.filename) && entry.is_dir {
        return false;
    }

    if let Some(pattern) = excluded_pattern {
        let path = if entry.is_dir {
            &entry.relative_pathname
        } else {
            &entry.relative_path
        };
        let path = path.replace('\\', "/");

        return !Preg::is_match(pattern, &path);
    }

    true
}

/// `FilenameFilterIterator::toRegex`.
fn to_regex_filename(pattern: &str) -> String {
    if is_regex(pattern) {
        pattern.to_string()
    } else {
        Glob::to_regex(pattern, true, true)
    }
}

/// `PathFilterIterator::toRegex`.
fn to_regex_path(pattern: &str) -> String {
    if is_regex(pattern) {
        pattern.to_string()
    } else {
        format!("/{}/", preg_quote(pattern, Some('/')))
    }
}

/// `MultiplePcreFilterIterator::isAccepted`.
fn is_accepted(string: &str, match_regexps: &[String], nomatch_regexps: &[String]) -> bool {
    for regex in nomatch_regexps {
        if Preg::is_match(regex, string) {
            return false;
        }
    }

    if !match_regexps.is_empty() {
        for regex in match_regexps {
            if Preg::is_match(regex, string) {
                return true;
            }
        }

        return false;
    }

    true
}

/// `MultiplePcreFilterIterator::isRegex`.
fn is_regex(str: &str) -> bool {
    // PHP 8.2+ available modifiers.
    let available_modifiers = "imsxuADUn";

    let mut matches: IndexMap<CaptureKey, String> = IndexMap::new();
    let pattern = format!("/^(.{{3,}}?)[{available_modifiers}]*$/");
    if Preg::is_match3(&pattern, str, Some(&mut matches)) {
        let group = matches
            .get(&CaptureKey::ByIndex(1))
            .cloned()
            .unwrap_or_default();
        let bytes = group.as_bytes();
        let start = bytes
            .first()
            .map(|b| (*b as char).to_string())
            .unwrap_or_default();
        let end = bytes
            .last()
            .map(|b| (*b as char).to_string())
            .unwrap_or_default();

        if start == end {
            return !Preg::is_match("/[*?[:alnum:] \\\\]/", &start);
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
fn comparator_test(operator: &str, test: i64, target: i64) -> bool {
    match operator {
        ">" => test > target,
        ">=" => test >= target,
        "<" => test < target,
        "<=" => test <= target,
        "!=" => test != target,
        _ => test == target,
    }
}

/// `DateComparator::__construct`, returning `(operator, target unix timestamp)`.
fn parse_date_comparator(test: &str) -> (String, i64) {
    let pattern = "#^\\s*(==|!=|[<>]=?|after|since|before|until)?\\s*(.+?)\\s*$#i";
    let mut matches: IndexMap<CaptureKey, String> = IndexMap::new();
    if !Preg::is_match3(pattern, test, Some(&mut matches)) {
        panic!("Don't understand \"{test}\" as a date test.");
    }

    let date = matches
        .get(&CaptureKey::ByIndex(2))
        .cloned()
        .unwrap_or_default();
    let target = parse_datetime_to_unix(&date);

    let mut operator = matches
        .get(&CaptureKey::ByIndex(1))
        .cloned()
        .unwrap_or_else(|| "==".to_string());
    if operator == "since" || operator == "after" {
        operator = ">".to_string();
    }
    if operator == "until" || operator == "before" {
        operator = "<".to_string();
    }

    (operator, target)
}

/// `(new \DateTime($s))->format('U')`.
///
/// TODO(phase-c): PHP's `\DateTime` accepts any strtotime() expression, but only the
/// `Y-m-d H:i:s` / `Y-m-d` shapes produced by the callers are parsed here. The components are
/// interpreted as UTC (not PHP's local timezone) so the timestamp round-trips with the
/// `chrono::Utc`-derived thresholds the callers format from.
fn parse_datetime_to_unix(s: &str) -> i64 {
    if let Ok(datetime) = NaiveDateTime::parse_from_str(s, "%Y-%m-%d %H:%M:%S") {
        return datetime.and_utc().timestamp();
    }
    if let Ok(date) = NaiveDate::parse_from_str(s, "%Y-%m-%d") {
        return date
            .and_hms_opt(0, 0, 0)
            .expect("midnight is always valid")
            .and_utc()
            .timestamp();
    }

    panic!("\"{s}\" is not a valid date.");
}

fn apply_order(ordering: std::cmp::Ordering, order: i64) -> std::cmp::Ordering {
    if order < 0 {
        ordering.reverse()
    } else {
        ordering
    }
}

/// `$file->getRealPath() ?: $file->getPathname()`.
fn realpath_or_pathname(path: &Path) -> String {
    std::fs::canonicalize(path)
        .map(|resolved| resolved.to_string_lossy().into_owned())
        .unwrap_or_else(|_| path.to_string_lossy().into_owned())
}

fn mtime(path: &Path) -> i64 {
    file_unix_time(path, |metadata| metadata.modified())
}

fn atime(path: &Path) -> i64 {
    file_unix_time(path, |metadata| metadata.accessed())
}

fn file_unix_time(
    path: &Path,
    accessor: fn(&std::fs::Metadata) -> std::io::Result<std::time::SystemTime>,
) -> i64 {
    std::fs::metadata(path)
        .ok()
        .and_then(|metadata| accessor(&metadata).ok())
        .and_then(|time| time.duration_since(UNIX_EPOCH).ok())
        .map(|duration| duration.as_secs() as i64)
        .unwrap_or(0)
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
