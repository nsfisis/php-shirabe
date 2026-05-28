//! ref: composer/src/Composer/Repository/Vcs/VcsDriverInterface.php

use crate::config::Config;
use crate::io::IOInterface;
use chrono::{DateTime, Utc};
use indexmap::IndexMap;
use shirabe_php_shim::PhpMixed;
use std::cell::RefCell;
use std::rc::Rc;

pub trait VcsDriverInterface: std::fmt::Debug {
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

    fn supports(io: Rc<RefCell<dyn IOInterface>>, config: &Config, url: &str, deep: bool) -> bool
    where
        Self: Sized;
}
