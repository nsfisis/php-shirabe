//! ref: composer/src/Composer/Config/ConfigSourceInterface.php

use shirabe_php_shim::PhpMixed;

pub trait ConfigSourceInterface: std::fmt::Debug {
    fn add_repository(&mut self, name: &str, config: PhpMixed, append: bool) -> anyhow::Result<()>;

    fn insert_repository(
        &mut self,
        name: &str,
        config: PhpMixed,
        reference_name: &str,
        offset: i64,
    ) -> anyhow::Result<()>;

    fn set_repository_url(&mut self, name: &str, url: &str) -> anyhow::Result<()>;

    fn remove_repository(&mut self, name: &str) -> anyhow::Result<()>;

    fn add_config_setting(&mut self, name: &str, value: PhpMixed) -> anyhow::Result<()>;

    fn remove_config_setting(&mut self, name: &str) -> anyhow::Result<()>;

    fn add_property(&mut self, name: &str, value: PhpMixed) -> anyhow::Result<()>;

    fn remove_property(&mut self, name: &str) -> anyhow::Result<()>;

    fn add_link(&mut self, r#type: &str, name: &str, value: &str) -> anyhow::Result<()>;

    fn remove_link(&mut self, r#type: &str, name: &str) -> anyhow::Result<()>;

    fn get_name(&self) -> String;
}
