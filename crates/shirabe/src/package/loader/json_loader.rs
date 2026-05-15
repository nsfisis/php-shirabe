//! ref: composer/src/Composer/Package/Loader/JsonLoader.php

use std::path::Path;
use anyhow::Result;
use crate::json::json_file::JsonFile;
use crate::package::base_package::BasePackage;
use crate::package::loader::loader_interface::LoaderInterface;

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

    pub fn load(&self, json: JsonLoaderInput) -> Result<Box<BasePackage>> {
        let config = match json {
            JsonLoaderInput::File(json_file) => json_file.read()?,
            JsonLoaderInput::String(ref s) if Path::new(s).exists() => {
                JsonFile::parse_json(&std::fs::read_to_string(s)?, Some(s))?
            }
            JsonLoaderInput::String(ref s) => {
                JsonFile::parse_json(s, None)?
            }
        };

        self.loader.load(config, None)
    }
}
