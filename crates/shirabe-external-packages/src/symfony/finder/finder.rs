use crate::symfony::finder::SplFileInfo;

/// Helper trait so `Finder::exclude` accepts both single strings and slices
/// (PHP's variadic / array argument compatibility).
pub trait IntoFinderExclude {}
impl IntoFinderExclude for &str {}
impl IntoFinderExclude for String {}
impl IntoFinderExclude for &String {}
impl IntoFinderExclude for &[String] {}
impl IntoFinderExclude for &Vec<String> {}
impl IntoFinderExclude for Vec<String> {}

#[derive(Debug)]
pub struct Finder;

impl Default for Finder {
    fn default() -> Self {
        Self::new()
    }
}

impl Finder {
    pub fn new() -> Self {
        todo!()
    }

    pub fn create() -> Self {
        todo!()
    }

    pub fn files(&mut self) -> &mut Self {
        todo!()
    }

    pub fn directories(&mut self) -> &mut Self {
        todo!()
    }

    pub fn depth(&mut self, _level: i64) -> &mut Self {
        todo!()
    }

    pub fn r#in(&mut self, _dirs: &str) -> &mut Self {
        todo!()
    }

    pub fn filter(&mut self, _closure: Box<dyn FnMut(&std::path::Path) -> bool>) -> &mut Self {
        todo!()
    }

    pub fn follow_links(&mut self) -> &mut Self {
        todo!()
    }

    pub fn exclude<E: IntoFinderExclude>(&mut self, _exclude: E) -> &mut Self {
        todo!()
    }

    pub fn ignore_vcs(&mut self, _ignore_vcs: bool) -> &mut Self {
        todo!()
    }

    pub fn ignore_dot_files(&mut self, _ignore_dot_files: bool) -> &mut Self {
        todo!()
    }

    pub fn not_name(&mut self, _pattern: &str) -> &mut Self {
        todo!()
    }

    pub fn not_path(&mut self, _pattern: &str) -> &mut Self {
        todo!()
    }

    pub fn name(&mut self, _pattern: &str) -> &mut Self {
        todo!()
    }

    pub fn sort<F>(&mut self, _comparator: F) -> &mut Self
    where
        F: FnMut(&SplFileInfo, &SplFileInfo) -> i64,
    {
        todo!()
    }

    pub fn sort_by_name(&mut self) -> &mut Self {
        todo!()
    }

    pub fn sort_by_accessed_time(&mut self) -> &mut Self {
        todo!()
    }

    pub fn date(&mut self, _date: &str) -> &mut Self {
        todo!()
    }

    pub fn get_iterator(&self) -> FinderIterator {
        todo!()
    }

    pub fn iter(&self) -> impl Iterator<Item = SplFileInfo> {
        todo!();
        std::iter::empty()
    }

    pub fn len(&self) -> usize {
        todo!()
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

impl IntoIterator for &Finder {
    type Item = SplFileInfo;
    type IntoIter = std::vec::IntoIter<SplFileInfo>;

    fn into_iter(self) -> Self::IntoIter {
        todo!()
    }
}

#[derive(Debug)]
pub struct FinderIterator;

impl FinderIterator {
    pub fn valid(&self) -> bool {
        todo!()
    }

    pub fn current(&self) -> SplFileInfo {
        todo!()
    }
}

impl Iterator for FinderIterator {
    type Item = SplFileInfo;

    fn next(&mut self) -> Option<SplFileInfo> {
        todo!()
    }
}

impl IntoIterator for Finder {
    type Item = SplFileInfo;
    type IntoIter = std::vec::IntoIter<SplFileInfo>;

    fn into_iter(self) -> Self::IntoIter {
        todo!()
    }
}

impl IntoIterator for &mut Finder {
    type Item = SplFileInfo;
    type IntoIter = std::vec::IntoIter<SplFileInfo>;

    fn into_iter(self) -> Self::IntoIter {
        todo!()
    }
}
