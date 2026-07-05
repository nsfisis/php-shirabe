//! ref: composer/src/Composer/Repository/Vcs/VcsDriverInterface.php

use crate::config::Config;
use crate::io::IOInterface;
use chrono::{DateTime, FixedOffset};
use indexmap::IndexMap;
use shirabe_php_shim::PhpMixed;
use std::cell::RefCell;
use std::rc::Rc;

pub trait VcsDriverInterface: std::fmt::Debug {
    fn initialize(&mut self) -> anyhow::Result<()>;

    fn get_composer_information(
        &mut self,
        identifier: &str,
    ) -> anyhow::Result<Option<IndexMap<String, PhpMixed>>>;

    fn get_file_content(&mut self, file: &str, identifier: &str) -> anyhow::Result<Option<String>>;

    fn get_change_date(
        &mut self,
        identifier: &str,
    ) -> anyhow::Result<Option<DateTime<FixedOffset>>>;

    fn get_root_identifier(&mut self) -> anyhow::Result<String>;

    fn get_branches(&mut self) -> anyhow::Result<IndexMap<String, String>>;

    fn get_tags(&mut self) -> anyhow::Result<IndexMap<String, String>>;

    fn get_dist(&self, identifier: &str) -> Option<IndexMap<String, String>>;

    fn get_source(&self, identifier: &str) -> IndexMap<String, String>;

    fn get_url(&self) -> String;

    fn has_composer_file(&mut self, identifier: &str) -> anyhow::Result<bool>;

    fn cleanup(&mut self) -> anyhow::Result<()>;

    fn supports(
        io: Rc<RefCell<dyn IOInterface>>,
        config: Rc<RefCell<Config>>,
        url: &str,
        deep: bool,
    ) -> anyhow::Result<bool>
    where
        Self: Sized;
}
