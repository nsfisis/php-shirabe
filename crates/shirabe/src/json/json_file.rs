//! ref: composer/src/Composer/Json/JsonFile.php

use anyhow::Result;
use shirabe_external_packages::composer::pcre::preg::Preg;
use shirabe_external_packages::json_schema::validator::Validator;
use shirabe_external_packages::seld::json_lint::json_parser::JsonParser;
use shirabe_external_packages::seld::json_lint::parsing_exception::ParsingException;
use shirabe_php_shim::{
    defined, dirname, file_exists, file_get_contents, file_put_contents, is_dir, is_file,
    json_decode, json_encode_ex, json_last_error, mkdir, php_dir, realpath, str_contains,
    str_ends_with, str_repeat, strlen, strpos, usleep, InvalidArgumentException, PhpMixed,
    RuntimeException, Silencer, UnexpectedValueException, JSON_ERROR_CTRL_CHAR, JSON_ERROR_DEPTH,
    JSON_ERROR_NONE, JSON_ERROR_STATE_MISMATCH, JSON_ERROR_UTF8, JSON_PRETTY_PRINT,
    JSON_UNESCAPED_SLASHES, JSON_UNESCAPED_UNICODE,
};

use crate::downloader::transport_exception::TransportException;
use crate::io::io_interface::IOInterface;
use crate::json::json_validation_exception::JsonValidationException;
use crate::util::filesystem::Filesystem;
use crate::util::http_downloader::HttpDownloader;

/// Reads/writes json files.
#[derive(Debug)]
pub struct JsonFile {
    /// @var string
    path: String,
    /// @var ?HttpDownloader
    http_downloader: Option<HttpDownloader>,
    /// @var ?IOInterface
    io: Option<Box<dyn IOInterface>>,
    /// @var string
    indent: String,
}

impl JsonFile {
    pub const LAX_SCHEMA: i64 = 1;
    pub const STRICT_SCHEMA: i64 = 2;
    pub const AUTH_SCHEMA: i64 = 3;
    pub const LOCK_SCHEMA: i64 = 4;

    /// @deprecated Use \JSON_UNESCAPED_SLASHES
    pub const JSON_UNESCAPED_SLASHES: i64 = 64;
    /// @deprecated Use \JSON_PRETTY_PRINT
    pub const JSON_PRETTY_PRINT: i64 = 128;
    /// @deprecated Use \JSON_UNESCAPED_UNICODE
    pub const JSON_UNESCAPED_UNICODE: i64 = 256;

    pub const INDENT_DEFAULT: &'static str = "    ";

    /// PHP: __DIR__ . '/../../../res/composer-schema.json'
    pub fn composer_schema_path() -> String {
        format!("{}/../../../res/composer-schema.json", php_dir())
    }

    /// PHP: __DIR__ . '/../../../res/composer-lock-schema.json'
    pub fn lock_schema_path() -> String {
        format!("{}/../../../res/composer-lock-schema.json", php_dir())
    }

    /// Initializes json file reader/parser.
    ///
    /// @param  string                    $path           path to a lockfile
    /// @param  ?HttpDownloader           $httpDownloader required for loading http/https json files
    /// @throws \InvalidArgumentException
    pub fn new(
        path: String,
        http_downloader: Option<HttpDownloader>,
        io: Option<Box<dyn IOInterface>>,
    ) -> Result<Self> {
        if http_downloader.is_none() && Preg::is_match(r"{^https?://}i", &path) {
            return Err(InvalidArgumentException {
                message: "http urls require a HttpDownloader instance to be passed".to_string(),
                code: 0,
            }
            .into());
        }
        Ok(Self {
            path,
            http_downloader,
            io,
            indent: Self::INDENT_DEFAULT.to_string(),
        })
    }

    pub fn get_path(&self) -> &str {
        &self.path
    }

    /// Checks whether json file exists.
    pub fn exists(&self) -> bool {
        is_file(&self.path)
    }

    /// Reads json file.
    ///
    /// @throws ParsingException
    /// @throws \RuntimeException
    /// @return mixed
    pub fn read(&mut self) -> Result<PhpMixed> {
        // TODO(phase-b): use anyhow::Result<Result<T, E>> to model PHP try/catch
        let json: Option<String> = match (|| -> Result<Option<String>> {
            if let Some(http_downloader) = &self.http_downloader {
                Ok(Some(http_downloader.get(&self.path)?.get_body()))
            } else {
                if !Filesystem::is_readable(&self.path) {
                    return Err(RuntimeException {
                        message: format!("The file \"{}\" is not readable.", self.path),
                        code: 0,
                    }
                    .into());
                }
                if let Some(io) = &self.io {
                    if io.is_debug() {
                        let mut realpath_info = String::new();
                        if let Some(realpath) = realpath(&self.path) {
                            if realpath != self.path {
                                realpath_info = format!(" ({})", realpath);
                            }
                        }
                        io.write_error(
                            PhpMixed::String(format!("Reading {}{}", self.path, realpath_info)),
                            true,
                            IOInterface::NORMAL,
                        );
                    }
                }
                Ok(file_get_contents(&self.path))
            }
        })() {
            Ok(j) => j,
            Err(e) => {
                // TODO(phase-b): downcast e to TransportException to match the specific catch
                let _te: &TransportException = todo!("downcast e to TransportException");
                // PHP: throw new \RuntimeException($e->getMessage(), 0, $e); (rethrow wrapped)
                // PHP fallback: throw new \RuntimeException('Could not read '.$this->path."\n\n".$e->getMessage());
                return Err(RuntimeException {
                    message: format!("Could not read {}\n\n{}", self.path, e),
                    code: 0,
                }
                .into());
            }
        };

        let json = match json {
            Some(j) => j,
            None => {
                return Err(RuntimeException {
                    message: format!("Could not read {}", self.path),
                    code: 0,
                }
                .into());
            }
        };

        self.indent = Self::detect_indenting(Some(&json));

        Self::parse_json(Some(&json), Some(&self.path))
    }

    /// Writes json file.
    ///
    /// @param  mixed[]                              $hash    writes hash into json file
    /// @param  int                                  $options json_encode options
    /// @throws \UnexpectedValueException|\Exception
    /// @return void
    pub fn write(&self, hash: PhpMixed, options: i64) -> Result<()> {
        if self.path == "php://memory" {
            file_put_contents(
                &self.path,
                Self::encode(&hash, options, &self.indent).as_bytes(),
            );

            return Ok(());
        }

        let dir = dirname(&self.path);
        if !is_dir(&dir) {
            if file_exists(&dir) {
                return Err(UnexpectedValueException {
                    message: format!(
                        "{} exists and is not a directory.",
                        realpath(&dir).unwrap_or_default(),
                    ),
                    code: 0,
                }
                .into());
            }
            // PHP: @mkdir($dir, 0777, true)
            if !Silencer::call(|| Ok(mkdir(&dir, 0o777, true))).unwrap_or(false) {
                return Err(UnexpectedValueException {
                    message: format!("{} does not exist and could not be created.", dir),
                    code: 0,
                }
                .into());
            }
        }

        let mut retries = 3;
        while retries > 0 {
            retries -= 1;
            // TODO(phase-b): use anyhow::Result<Result<T, E>> to model PHP try/catch
            let attempt: Result<()> = (|| -> Result<()> {
                self.file_put_contents_if_modified(
                    &self.path,
                    &format!(
                        "{}{}",
                        Self::encode(&hash, options, &self.indent),
                        if options & JSON_PRETTY_PRINT != 0 {
                            "\n"
                        } else {
                            ""
                        },
                    ),
                )?;
                Ok(())
            })();
            match attempt {
                Ok(_) => break,
                Err(e) => {
                    if retries > 0 {
                        usleep(500_000);
                        continue;
                    }

                    return Err(e);
                }
            }
        }

        Ok(())
    }

    /// Modify file properties only if content modified
    ///
    /// @return int|false
    fn file_put_contents_if_modified(&self, path: &str, content: &str) -> Result<Option<i64>> {
        // PHP: @file_get_contents($path)
        let current_content = Silencer::call(|| Ok(file_get_contents(path))).ok().flatten();
        if current_content.is_none() || current_content.as_deref() != Some(content) {
            return Ok(file_put_contents(path, content.as_bytes()));
        }

        Ok(Some(0))
    }

    /// Validates the schema of the current json file according to composer-schema.json rules
    ///
    /// @param  int                     $schema     a JsonFile::*_SCHEMA constant
    /// @param  string|null             $schemaFile a path to the schema file
    /// @throws JsonValidationException
    /// @throws ParsingException
    /// @return true                    true on success
    ///
    /// @phpstan-param self::*_SCHEMA $schema
    pub fn validate_schema(&self, schema: i64, schema_file: Option<&str>) -> Result<bool> {
        if !Filesystem::is_readable(&self.path) {
            return Err(RuntimeException {
                message: format!("The file \"{}\" is not readable.", self.path),
                code: 0,
            }
            .into());
        }
        let content = file_get_contents(&self.path).unwrap_or_default();
        let data = json_decode(&content, false)?;

        if matches!(data, PhpMixed::Null) && content != "null" {
            Self::validate_syntax(&content, Some(&self.path))?;
        }

        Self::validate_json_schema(&self.path, &data, schema, schema_file)
    }

    /// Validates the schema of the current json file according to composer-schema.json rules
    ///
    /// @param  mixed                   $data       Decoded JSON data to validate
    /// @param  int                     $schema     a JsonFile::*_SCHEMA constant
    /// @param  string|null             $schemaFile a path to the schema file
    /// @throws JsonValidationException
    /// @return true                    true on success
    ///
    /// @phpstan-param self::*_SCHEMA $schema
    pub fn validate_json_schema(
        source: &str,
        data: &PhpMixed,
        schema: i64,
        schema_file: Option<&str>,
    ) -> Result<bool> {
        let mut is_composer_schema_file = false;
        let mut schema_file: String = match schema_file {
            Some(f) => f.to_string(),
            None => {
                if schema == Self::LOCK_SCHEMA {
                    Self::lock_schema_path()
                } else {
                    is_composer_schema_file = true;
                    Self::composer_schema_path()
                }
            }
        };

        // Prepend with file:// only when not using a special schema already (e.g. in the phar)
        if strpos(&schema_file, "://").is_none() {
            schema_file = format!("file://{}", schema_file);
        }

        // PHP: $schemaData = (object) ['$ref' => $schemaFile, '$schema' => "https://json-schema.org/draft-04/schema#"];
        // TODO(phase-b): represent (object) cast as PhpMixed::Array or a dedicated stdClass shim
        let mut schema_data: PhpMixed = {
            let mut m = indexmap::IndexMap::new();
            m.insert(
                "$ref".to_string(),
                Box::new(PhpMixed::String(schema_file.clone())),
            );
            m.insert(
                "$schema".to_string(),
                Box::new(PhpMixed::String(
                    "https://json-schema.org/draft-04/schema#".to_string(),
                )),
            );
            PhpMixed::Array(m)
        };

        if schema == Self::STRICT_SCHEMA && is_composer_schema_file {
            schema_data = json_decode(&file_get_contents(&schema_file).unwrap_or_default(), false)?;
            // TODO(phase-b): mutate object properties; using PhpMixed::Array we set keys
            if let PhpMixed::Array(map) = &mut schema_data {
                map.insert(
                    "additionalProperties".to_string(),
                    Box::new(PhpMixed::Bool(false)),
                );
                map.insert(
                    "required".to_string(),
                    Box::new(PhpMixed::List(vec![
                        Box::new(PhpMixed::String("name".to_string())),
                        Box::new(PhpMixed::String("description".to_string())),
                    ])),
                );
            }
        } else if schema == Self::AUTH_SCHEMA && is_composer_schema_file {
            let mut m = indexmap::IndexMap::new();
            m.insert(
                "$ref".to_string(),
                Box::new(PhpMixed::String(format!(
                    "{}#/properties/config",
                    schema_file,
                ))),
            );
            m.insert(
                "$schema".to_string(),
                Box::new(PhpMixed::String(
                    "https://json-schema.org/draft-04/schema#".to_string(),
                )),
            );
            schema_data = PhpMixed::Array(m);
        }

        let mut validator = Validator::new();
        // convert assoc arrays to objects
        let data_converted = json_decode(&json_encode_ex(data, 0).unwrap_or_default(), false)?;
        validator.validate(&data_converted, &schema_data);

        if !validator.is_valid() {
            let mut errors: Vec<String> = vec![];
            for error in validator.get_errors() {
                let property = error
                    .get("property")
                    .and_then(|v| v.as_string())
                    .unwrap_or("");
                let message = error
                    .get("message")
                    .and_then(|v| v.as_string())
                    .unwrap_or("");
                errors.push(format!(
                    "{}{}",
                    if !property.is_empty() {
                        format!("{} : ", property)
                    } else {
                        String::new()
                    },
                    message,
                ));
            }
            return Err(JsonValidationException::new(
                format!("\"{}\" does not match the expected JSON schema", source),
                errors,
            )
            .into());
        }

        Ok(true)
    }

    /// Encodes an array into (optionally pretty-printed) JSON
    ///
    /// @param  mixed  $data    Data to encode into a formatted JSON string
    /// @param  int    $options json_encode options (defaults to JSON_UNESCAPED_SLASHES | JSON_PRETTY_PRINT | JSON_UNESCAPED_UNICODE)
    /// @param  string $indent  Indentation string
    /// @return string Encoded json
    pub fn encode(data: &PhpMixed, options: i64, indent: &str) -> String {
        let json = json_encode_ex(data, options);

        let json = match json {
            Some(j) => j,
            None => {
                // PHP: self::throwEncodeError(json_last_error());
                // TODO(phase-b): throw an error; downstream callers expect a String
                Self::throw_encode_error(json_last_error()).unwrap_or_default();
                String::new()
            }
        };

        if (options & JSON_PRETTY_PRINT) > 0 && indent != Self::INDENT_DEFAULT {
            // Pretty printing and not using default indentation
            let indent = indent.to_string();
            return Preg::replace_callback(
                r"#^ {4,}#m",
                move |m| -> String {
                    str_repeat(
                        &indent,
                        (strlen(m.get(0).map(|s| s.as_str()).unwrap_or("")) / 4) as usize,
                    )
                },
                &json,
            );
        }

        json
    }

    /// Throws an exception according to a given code with a customized message
    ///
    /// @param  int               $code return code of json_last_error function
    /// @throws \RuntimeException
    /// @return never
    fn throw_encode_error(code: i64) -> Result<()> {
        let msg = if code == JSON_ERROR_DEPTH {
            "Maximum stack depth exceeded"
        } else if code == JSON_ERROR_STATE_MISMATCH {
            "Underflow or the modes mismatch"
        } else if code == JSON_ERROR_CTRL_CHAR {
            "Unexpected control character found"
        } else if code == JSON_ERROR_UTF8 {
            "Malformed UTF-8 characters, possibly incorrectly encoded"
        } else {
            "Unknown error"
        };

        Err(RuntimeException {
            message: format!("JSON encoding failed: {}", msg),
            code: 0,
        }
        .into())
    }

    /// Parses json string and returns hash.
    ///
    /// @param null|string $json json string
    /// @param string $file the json file
    ///
    /// @throws ParsingException
    /// @return mixed
    pub fn parse_json(json: Option<&str>, file: Option<&str>) -> Result<PhpMixed> {
        let json = match json {
            None => return Ok(PhpMixed::Null),
            Some(j) => j,
        };
        let mut data = json_decode(json, true)?;
        if matches!(data, PhpMixed::Null) && JSON_ERROR_NONE != json_last_error() {
            // attempt resolving simple conflicts in lock files so that one can run `composer update --lock` and get a valid lock file
            if let Some(file) = file {
                if str_ends_with(file, ".lock") && str_contains(json, "\"content-hash\"") {
                    // TODO(phase-b): Preg::replace_with_count signature unavailable; ignoring $count
                    let replaced = Preg::replace(
                        r#"{\r?\n<<<<<<< [^\r\n]+\r?\n\s+"content-hash": *"[0-9a-f]+", *\r?\n(?:\|{7} [^\r\n]+\r?\n\s+"content-hash": *"[0-9a-f]+", *\r?\n)?=======\r?\n\s+"content-hash": *"[0-9a-f]+", *\r?\n>>>>>>> [^\r\n]+(\r?\n)}"#,
                        "    \"content-hash\": \"VCS merge conflict detected. Please run `composer update --lock`.\",$1",
                        json,
                    );
                    let count = todo!("Preg::replace returning $count");
                    if count == 1 {
                        data = json_decode(&replaced, true)?;
                        if !matches!(data, PhpMixed::Null) {
                            return Ok(data);
                        }
                    }
                }
            }

            Self::validate_syntax(json, file)?;
        }

        Ok(data)
    }

    /// Validates the syntax of a JSON string
    ///
    /// @throws \UnexpectedValueException
    /// @throws ParsingException
    /// @return bool                      true on success
    pub(crate) fn validate_syntax(json: &str, file: Option<&str>) -> Result<bool> {
        let mut parser = JsonParser::new();
        let result = parser.lint(json);
        if result.is_none() {
            if defined("JSON_ERROR_UTF8") && JSON_ERROR_UTF8 == json_last_error() {
                return Err(UnexpectedValueException {
                    message: match file {
                        None => "The input is not UTF-8, could not parse as JSON".to_string(),
                        Some(f) => format!("\"{}\" is not UTF-8, could not parse as JSON", f),
                    },
                    code: 0,
                }
                .into());
            }

            return Ok(true);
        }

        let result = result.unwrap();
        Err(match file {
            None => ParsingException::new(
                format!("The input does not contain valid JSON\n{}", result.get_message()),
                result.get_details(),
            ),
            Some(f) => ParsingException::new(
                format!("\"{}\" does not contain valid JSON\n{}", f, result.get_message()),
                result.get_details(),
            ),
        }
        .into())
    }

    pub fn detect_indenting(json: Option<&str>) -> String {
        if let Some(m) = Preg::is_match_strict_groups(
            r##"#^([ \t]+)"#m"##,
            json.unwrap_or(""),
        ) {
            return m.get(1).cloned().unwrap_or_default();
        }

        Self::INDENT_DEFAULT.to_string()
    }
}
