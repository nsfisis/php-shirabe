//! ref: composer/src/Composer/Repository/Vcs/VcsDriverInterface.php

use crate::config::Config;
use crate::io::io_interface::IOInterface;
use chrono::{DateTime, Utc};
use indexmap::IndexMap;
use shirabe_php_shim::PhpMixed;

pub trait VcsDriverInterface {
    fn initialize(&mut self) -> anyhow::Result<()>;

    fn get_composer_information(
        &self,
        identifier: &str,
    ) -> anyhow::Result<Option<IndexMap<String, PhpMixed>>>;

    fn get_file_content(&self, file: &str, identifier: &str) -> anyhow::Result<Option<String>>;

    fn get_change_date(&self, identifier: &str) -> anyhow::Result<Option<DateTime<Utc>>>;

    fn get_root_identifier(&self) -> anyhow::Result<String>;

    fn get_branches(&self) -> anyhow::Result<IndexMap<String, String>>;

    fn get_tags(&self) -> anyhow::Result<IndexMap<String, String>>;

    fn get_dist(&self, identifier: &str) -> anyhow::Result<Option<IndexMap<String, String>>>;

    fn get_source(&self, identifier: &str) -> anyhow::Result<IndexMap<String, String>>;

    fn get_url(&self) -> String;

    fn has_composer_file(&self, identifier: &str) -> anyhow::Result<bool>;

    fn cleanup(&mut self) -> anyhow::Result<()>;

    fn supports(io: &dyn IOInterface, config: &Config, url: &str, deep: bool) -> bool
    where
        Self: Sized;
}
