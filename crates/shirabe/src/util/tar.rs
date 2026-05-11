//! ref: composer/src/Composer/Util/Tar.php

use anyhow::Result;
use indexmap::IndexMap;
use shirabe_php_shim::{PharData, RuntimeException};

pub struct Tar;

impl Tar {
    pub fn get_composer_json(path_to_archive: &str) -> Result<Option<String>> {
        let phar = PharData::new(path_to_archive.to_string());

        if !phar.valid() {
            return Ok(None);
        }

        Ok(Some(Self::extract_composer_json_from_folder(&phar)?))
    }

    fn extract_composer_json_from_folder(phar: &PharData) -> Result<String> {
        if let Some(file) = phar.get("composer.json") {
            return Ok(file.get_content());
        }

        let mut top_level_paths: IndexMap<String, bool> = IndexMap::new();
        for folder_file in phar.iter() {
            let name = folder_file.get_basename();
            if folder_file.is_dir() {
                top_level_paths.insert(name, true);
                if top_level_paths.len() > 1 {
                    return Err(anyhow::anyhow!(RuntimeException {
                        message: format!(
                            "Archive has more than one top level directories, and no composer.json was found on the top level, so it's an invalid archive. Top level paths found were: {}",
                            top_level_paths.keys().cloned().collect::<Vec<_>>().join(",")
                        ),
                        code: 0,
                    }));
                }
            }
        }

        let composer_json_path = format!(
            "{}/composer.json",
            top_level_paths.keys().next().cloned().unwrap_or_default()
        );
        if !top_level_paths.is_empty() {
            if let Some(file) = phar.get(&composer_json_path) {
                return Ok(file.get_content());
            }
        }

        Err(anyhow::anyhow!(RuntimeException {
            message: "No composer.json found either at the top level or within the topmost directory".to_string(),
            code: 0,
        }))
    }
}
