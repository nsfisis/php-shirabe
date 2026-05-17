use shirabe_php_shim::PhpMixed;

#[derive(Debug, Clone)]
pub struct Filesystem;

impl Default for Filesystem {
    fn default() -> Self {
        Self::new()
    }
}

impl Filesystem {
    pub fn new() -> Self {
        todo!()
    }

    pub fn copy(
        &self,
        _origin_file: &str,
        _target_file: &str,
        _override_file: bool,
    ) -> anyhow::Result<()> {
        todo!()
    }

    pub fn mkdir(&self, _dirs: PhpMixed, _mode: u32) -> anyhow::Result<()> {
        todo!()
    }

    pub fn exists(&self, _files: PhpMixed) -> bool {
        todo!()
    }

    pub fn touch(
        &self,
        _files: PhpMixed,
        _time: Option<i64>,
        _atime: Option<i64>,
    ) -> anyhow::Result<()> {
        todo!()
    }

    pub fn remove(&self, _files: PhpMixed) -> anyhow::Result<()> {
        todo!()
    }

    pub fn chmod(
        &self,
        _files: PhpMixed,
        _mode: u32,
        _umask: u32,
        _recursive: bool,
    ) -> anyhow::Result<()> {
        todo!()
    }

    pub fn chown(&self, _files: PhpMixed, _user: PhpMixed, _recursive: bool) -> anyhow::Result<()> {
        todo!()
    }

    pub fn chgrp(
        &self,
        _files: PhpMixed,
        _group: PhpMixed,
        _recursive: bool,
    ) -> anyhow::Result<()> {
        todo!()
    }

    pub fn rename(&self, _origin: &str, _target: &str, _override_file: bool) -> anyhow::Result<()> {
        todo!()
    }

    pub fn symlink(
        &self,
        _origin_dir: &str,
        _target_dir: &str,
        _copy_on_windows: bool,
    ) -> anyhow::Result<()> {
        todo!()
    }

    pub fn hard_link(&self, _origin_file: &str, _target_files: PhpMixed) -> anyhow::Result<()> {
        todo!()
    }

    pub fn read_link(&self, _path: &str) -> String {
        todo!()
    }

    pub fn make_path_relative(&self, _end_path: &str, _start_path: &str) -> String {
        todo!()
    }

    pub fn mirror(
        &self,
        _origin_dir: &str,
        _target_dir: &str,
        _iterator: Option<PhpMixed>,
        _options: &indexmap::IndexMap<String, PhpMixed>,
    ) -> anyhow::Result<()> {
        todo!()
    }

    pub fn is_absolute_path(&self, _file: &str) -> bool {
        todo!()
    }

    pub fn dump_file(&self, _filename: &str, _content: &str) -> anyhow::Result<()> {
        todo!()
    }

    pub fn append_to_file(&self, _filename: &str, _content: &str) -> anyhow::Result<()> {
        todo!()
    }

    pub fn temp_nam(&self, _dir: &str, _prefix: &str) -> anyhow::Result<String> {
        todo!()
    }

    // Static-style helper methods used in the ported codebase
    pub fn is_readable(_path: &str) -> bool {
        todo!()
    }

    pub fn is_local_path(_path: &str) -> bool {
        todo!()
    }

    pub fn trim_trailing_slash(_path: &str) -> String {
        todo!()
    }

    pub fn get_platform_path(_path: &str) -> String {
        todo!()
    }
}
