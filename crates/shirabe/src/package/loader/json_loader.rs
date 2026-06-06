//! ref: composer/src/Composer/Package/Loader/JsonLoader.php

use crate::json::JsonFile;
use crate::package::PackageInterfaceHandle;
use crate::package::loader::LoaderInterface;
use anyhow::Result;
use indexmap::IndexMap;
use shirabe_php_shim::{PhpMixed, TypeError};
use std::path::Path;

pub enum JsonLoaderInput {
    File(JsonFile),
    String(String),
}

pub struct JsonLoader {
    loader: Box<dyn LoaderInterface>,
}

impl JsonLoader {
    pub fn new(loader: Box<dyn LoaderInterface>) -> Self {
        Self { loader }
    }

    pub fn load(&self, json: JsonLoaderInput) -> Result<PackageInterfaceHandle> {
        let config = match json {
            JsonLoaderInput::File(mut json_file) => json_file.read()?,
            JsonLoaderInput::String(ref s) if Path::new(s).exists() => {
                let contents = std::fs::read_to_string(s)?;
                JsonFile::parse_json(Some(&contents), Some(s))?
            }
            JsonLoaderInput::String(ref s) => JsonFile::parse_json(Some(s), None)?,
        };

        let config: IndexMap<String, PhpMixed> = match config {
            PhpMixed::Array(m) => m.into_iter().map(|(k, v)| (k, *v)).collect(),
            _ => {
                return Err(TypeError {
                    message: "Composer\\Package\\Loader\\LoaderInterface::load(): Argument #1 ($config) must be of type array".to_string(),
                    code: 0,
                }
                .into());
            }
        };

        self.loader.load(config, None)
    }
}
