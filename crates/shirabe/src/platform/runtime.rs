//! ref: composer/src/Composer/Platform/Runtime.php

use anyhow::Result;
use shirabe_php_shim::PhpMixed;

#[derive(Debug)]
pub struct Runtime;

impl Runtime {
    pub fn has_constant(&self, constant_name: &str, class: Option<&str>) -> bool {
        todo!()
    }

    pub fn get_constant(&self, constant_name: &str, class: Option<&str>) -> PhpMixed {
        todo!()
    }

    pub fn has_function(&self, f: &str) -> bool {
        todo!()
    }

    pub fn invoke(
        &self,
        callable: Box<dyn Fn(Vec<PhpMixed>) -> PhpMixed>,
        arguments: Vec<PhpMixed>,
    ) -> PhpMixed {
        todo!()
    }

    pub fn has_class(&self, class: &str) -> bool {
        todo!()
    }

    pub fn construct(&self, class: &str, arguments: Vec<PhpMixed>) -> Result<PhpMixed> {
        todo!()
    }

    pub fn get_extensions(&self) -> Vec<String> {
        todo!()
    }

    pub fn get_extension_version(&self, extension: &str) -> String {
        todo!()
    }

    pub fn get_extension_info(&self, extension: &str) -> Result<String> {
        todo!()
    }

    pub fn parse_html_extension_info(html: &str) -> String {
        todo!()
    }
}
