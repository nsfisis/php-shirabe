//! ref: composer/src/Composer/Util/Zip.php

use anyhow::Result;
use indexmap::IndexMap;
use shirabe_php_shim::{
    RuntimeException, ZipArchive, dirname, extension_loaded, implode, stream_get_contents,
};

pub struct Zip;

impl Zip {
    pub fn get_composer_json(path_to_zip: &str) -> Result<Option<String>> {
        if !extension_loaded("zip") {
            return Err(RuntimeException {
                message: "The Zip Util requires PHP's zip extension".to_string(),
                code: 0,
            }
            .into());
        }

        let mut zip = ZipArchive::new();
        if zip.open(path_to_zip, 0).is_err() {
            return Ok(None);
        }

        if zip.num_files == 0 {
            zip.close();
            return Ok(None);
        }

        let found_file_index = Self::locate_file(&zip, "composer.json")?;

        let mut content: Option<String> = None;
        let configuration_file_name = zip.get_name_index(found_file_index);
        let stream = zip.get_stream(&configuration_file_name);

        if stream.is_some() {
            content = stream_get_contents(stream.unwrap());
        }

        zip.close();

        Ok(content)
    }

    fn locate_file(zip: &ZipArchive, filename: &str) -> Result<i64> {
        // return root composer.json if it is there and is a file
        if let Some(index) = zip.locate_name(filename) {
            if zip.get_from_index(index).is_some() {
                return Ok(index);
            }
        }

        let mut top_level_paths: IndexMap<String, bool> = IndexMap::new();
        for i in 0..zip.num_files {
            let name = zip.get_name_index(i);
            let dir_name = dirname(&name);

            // ignore OSX specific resource fork folder
            if name.contains("__MACOSX") {
                continue;
            }

            // handle archives with proper TOC
            if dir_name == "." {
                top_level_paths.insert(name, true);
                if top_level_paths.len() > 1 {
                    return Err(RuntimeException {
                        message: format!(
                            "Archive has more than one top level directories, and no composer.json was found on the top level, so it's an invalid archive. Top level paths found were: {}",
                            implode(",", &top_level_paths.keys().cloned().collect::<Vec<_>>())
                        ),
                        code: 0,
                    }
                    .into());
                }
                continue;
            }

            // handle archives which do not have a TOC record for the directory itself
            if !dir_name.contains('\\') && !dir_name.contains('/') {
                top_level_paths.insert(format!("{}/", dir_name), true);
                if top_level_paths.len() > 1 {
                    return Err(RuntimeException {
                        message: format!(
                            "Archive has more than one top level directories, and no composer.json was found on the top level, so it's an invalid archive. Top level paths found were: {}",
                            implode(",", &top_level_paths.keys().cloned().collect::<Vec<_>>())
                        ),
                        code: 0,
                    }
                    .into());
                }
            }
        }

        if !top_level_paths.is_empty() {
            let first_key = top_level_paths.keys().next().unwrap().clone();
            if let Some(index) = zip.locate_name(&format!("{}{}", first_key, filename)) {
                if zip.get_from_index(index).is_some() {
                    return Ok(index);
                }
            }
        }

        Err(RuntimeException {
            message:
                "No composer.json found either at the top level or within the topmost directory"
                    .to_string(),
            code: 0,
        }
        .into())
    }
}
