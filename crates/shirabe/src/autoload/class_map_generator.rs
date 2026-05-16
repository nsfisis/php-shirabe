//! ref: composer/src/Composer/Autoload/ClassMapGenerator.php

use indexmap::IndexMap;

use shirabe_class_map_generator::class_map_generator::ClassMapGenerator as ExternalClassMapGenerator;
use shirabe_class_map_generator::file_list::FileList;
use shirabe_php_shim::PhpMixed;

use crate::io::io_interface::IOInterface;

#[derive(Debug)]
pub struct ClassMapGenerator;

impl ClassMapGenerator {
    pub fn dump(dirs: Vec<String>, file: &str) -> anyhow::Result<()> {
        let mut maps: IndexMap<String, String> = IndexMap::new();
        for dir in dirs {
            maps.extend(ClassMapGenerator::create_map(
                PhpMixed::String(dir),
                None,
                None,
                None,
                None,
                &mut IndexMap::new(),
            )?);
        }
        let maps_php = PhpMixed::Array(
            maps.into_iter()
                .map(|(k, v)| (k, Box::new(PhpMixed::String(v))))
                .collect(),
        );
        std::fs::write(
            file,
            format!(
                "<?php return {};",
                shirabe_php_shim::var_export(&maps_php, true)
            ),
        )?;
        Ok(())
    }

    pub fn create_map(
        path: PhpMixed,
        excluded: Option<String>,
        io: Option<Box<dyn IOInterface>>,
        namespace: Option<String>,
        autoload_type: Option<String>,
        scanned_files: &mut IndexMap<String, bool>,
    ) -> anyhow::Result<IndexMap<String, String>> {
        let generator = ExternalClassMapGenerator::new(vec![
            "php".to_string(),
            "inc".to_string(),
            "hh".to_string(),
        ]);
        let mut file_list = FileList::new();
        file_list.files = scanned_files.clone();
        generator.avoid_duplicate_scans(&file_list);

        generator.scan_paths(
            path,
            excluded.as_deref(),
            autoload_type.as_deref().unwrap_or("classmap"),
            namespace.as_deref(),
        )?;

        let class_map = generator.get_class_map();

        *scanned_files = file_list.files;

        if let Some(io) = &io {
            for msg in class_map.get_psr_violations() {
                io.write_error(&format!("<warning>{}</warning>", msg));
            }

            for (class, paths) in class_map.get_ambiguous_classes() {
                if paths.len() > 1 {
                    io.write_error(&format!(
                        "<warning>Warning: Ambiguous class resolution, \"{}\" was found {}x: in \"{}\" and \"{}\", the first will be used.</warning>",
                        class,
                        paths.len() + 1,
                        class_map.get_class_path(&class),
                        paths.join("\", \""),
                    ));
                } else {
                    io.write_error(&format!(
                        "<warning>Warning: Ambiguous class resolution, \"{}\" was found in both \"{}\" and \"{}\", the first will be used.</warning>",
                        class,
                        class_map.get_class_path(&class),
                        paths.join("\", \""),
                    ));
                }
            }
        }

        Ok(class_map.get_map())
    }
}
