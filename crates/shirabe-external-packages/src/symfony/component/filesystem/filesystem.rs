use shirabe_php_shim::PhpMixed;

#[derive(Debug, Clone)]
pub struct Filesystem;

impl Filesystem {
    pub fn new() -> Self {
        todo!()
    }

    pub fn copy(
        &self,
        origin_file: &str,
        target_file: &str,
        override_file: bool,
    ) -> anyhow::Result<()> {
        todo!()
    }

    pub fn mkdir(&self, dirs: PhpMixed, mode: u32) -> anyhow::Result<()> {
        todo!()
    }

    pub fn exists(&self, files: PhpMixed) -> bool {
        todo!()
    }

    pub fn touch(
        &self,
        files: PhpMixed,
        time: Option<i64>,
        atime: Option<i64>,
    ) -> anyhow::Result<()> {
        todo!()
    }

    pub fn remove(&self, files: PhpMixed) -> anyhow::Result<()> {
        todo!()
    }

    pub fn chmod(
        &self,
        files: PhpMixed,
        mode: u32,
        umask: u32,
        recursive: bool,
    ) -> anyhow::Result<()> {
        todo!()
    }

    pub fn chown(&self, files: PhpMixed, user: PhpMixed, recursive: bool) -> anyhow::Result<()> {
        todo!()
    }

    pub fn chgrp(&self, files: PhpMixed, group: PhpMixed, recursive: bool) -> anyhow::Result<()> {
        todo!()
    }

    pub fn rename(&self, origin: &str, target: &str, override_file: bool) -> anyhow::Result<()> {
        todo!()
    }

    pub fn symlink(
        &self,
        origin_dir: &str,
        target_dir: &str,
        copy_on_windows: bool,
    ) -> anyhow::Result<()> {
        todo!()
    }

    pub fn hard_link(&self, origin_file: &str, target_files: PhpMixed) -> anyhow::Result<()> {
        todo!()
    }

    pub fn read_link(&self, path: &str) -> String {
        todo!()
    }

    pub fn make_path_relative(&self, end_path: &str, start_path: &str) -> String {
        todo!()
    }

    pub fn mirror(
        &self,
        origin_dir: &str,
        target_dir: &str,
        iterator: Option<PhpMixed>,
        options: &indexmap::IndexMap<String, PhpMixed>,
    ) -> anyhow::Result<()> {
        todo!()
    }

    pub fn is_absolute_path(&self, file: &str) -> bool {
        todo!()
    }

    pub fn dump_file(&self, filename: &str, content: &str) -> anyhow::Result<()> {
        todo!()
    }

    pub fn append_to_file(&self, filename: &str, content: &str) -> anyhow::Result<()> {
        todo!()
    }

    pub fn temp_nam(&self, dir: &str, prefix: &str) -> anyhow::Result<String> {
        todo!()
    }

    // Static-style helper methods used in the ported codebase
    pub fn is_readable(path: &str) -> bool {
        todo!()
    }

    pub fn is_local_path(path: &str) -> bool {
        todo!()
    }

    pub fn trim_trailing_slash(path: &str) -> String {
        todo!()
    }

    pub fn get_platform_path(path: &str) -> String {
        todo!()
    }
}
