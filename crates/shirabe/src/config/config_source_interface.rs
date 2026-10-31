//! ref: composer/src/Composer/Config/ConfigSourceInterface.php

use bstr::BStr;
use shirabe_php_shim::PhpMixed;

pub trait ConfigSourceInterface: std::fmt::Debug {
    fn add_repository(&mut self, name: &BStr, config: PhpMixed, append: bool)
    -> anyhow::Result<()>;

    fn insert_repository(
        &mut self,
        name: &BStr,
        config: PhpMixed,
        reference_name: &BStr,
        offset: i64,
    ) -> anyhow::Result<()>;

    fn set_repository_url(&mut self, name: &BStr, url: &BStr) -> anyhow::Result<()>;

    fn remove_repository(&mut self, name: &BStr) -> anyhow::Result<()>;

    fn add_config_setting(&mut self, name: &BStr, value: PhpMixed) -> anyhow::Result<()>;

    fn remove_config_setting(&mut self, name: &BStr) -> anyhow::Result<()>;

    fn add_property(&mut self, name: &BStr, value: PhpMixed) -> anyhow::Result<()>;

    fn remove_property(&mut self, name: &BStr) -> anyhow::Result<()>;

    fn add_link(&mut self, r#type: &BStr, name: &BStr, value: &BStr) -> anyhow::Result<()>;

    fn remove_link(&mut self, r#type: &BStr, name: &BStr) -> anyhow::Result<()>;

    fn get_name(&self) -> String;
}
