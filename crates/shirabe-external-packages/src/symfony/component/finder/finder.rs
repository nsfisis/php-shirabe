use crate::symfony::component::finder::spl_file_info::SplFileInfo;

#[derive(Debug)]
pub struct Finder;

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

    pub fn depth(&mut self, level: i64) -> &mut Self {
        todo!()
    }

    pub fn r#in(&mut self, dirs: &str) -> &mut Self {
        todo!()
    }

    pub fn follow_links(&mut self) -> &mut Self {
        todo!()
    }

    pub fn exclude(&mut self, exclude: &[String]) -> &mut Self {
        todo!()
    }

    pub fn ignore_vcs(&mut self, ignore_vcs: bool) -> &mut Self {
        todo!()
    }

    pub fn ignore_dot_files(&mut self, ignore_dot_files: bool) -> &mut Self {
        todo!()
    }

    pub fn not_name(&mut self, pattern: &str) -> &mut Self {
        todo!()
    }

    pub fn name(&mut self, pattern: &str) -> &mut Self {
        todo!()
    }

    pub fn sort_by_name(&mut self) -> &mut Self {
        todo!()
    }

    pub fn iter(&self) -> impl Iterator<Item = SplFileInfo> {
        todo!();
        std::iter::empty()
    }
}
