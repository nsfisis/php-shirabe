//! ref: composer/src/Composer/Json/JsonFile.php

use crate::downloader::TransportException;
use crate::io::IOInterface;
use crate::io::IOInterfaceImmutable;
use crate::io::io_interface;
use crate::json::JsonValidationException;
use crate::util::Filesystem;
use crate::util::HttpDownloader;
use crate::util::Silencer;
use shirabe_php_shim::Catch as _;
use shirabe_php_shim::{
    InvalidArgumentException, JSON_PRETTY_PRINT, JSON_UNESCAPED_SLASHES, JSON_UNESCAPED_UNICODE,
    PhpMixed, PregMatches, RuntimeException, UnexpectedValueException, dirname, file_exists,
    file_get_contents, file_put_contents, is_dir, is_file, json_decode_assoc, json_decode_obj,
    json_encode_ex, mkdir, php_regex, preg_match, preg_replace_callback, preg_replace2, realpath,
    str_repeat, strlen, strpos, usleep,
};
use shirabe_seld_json_lint::{ParsingException, ParsingExceptionDetails};

#[derive(Debug, Clone)]
pub struct JsonEncodeOptions {
    pub unescaped_slashes: bool,
    pub pretty_print: bool,
    pub unescaped_unicode: bool,
    pub indent: String,
}

impl Default for JsonEncodeOptions {
    fn default() -> Self {
        Self {
            unescaped_slashes: true,
            pretty_print: true,
            unescaped_unicode: true,
            indent: JsonFile::INDENT_DEFAULT.to_string(),
        }
    }
}

impl JsonEncodeOptions {
    pub fn none() -> Self {
        Self {
            unescaped_slashes: false,
            pretty_print: false,
            unescaped_unicode: false,
            indent: JsonFile::INDENT_DEFAULT.to_string(),
        }
    }

    fn to_flags(&self) -> i64 {
        let mut flags = 0;
        if self.unescaped_slashes {
            flags |= JSON_UNESCAPED_SLASHES;
        }
        if self.pretty_print {
            flags |= JSON_PRETTY_PRINT;
        }
        if self.unescaped_unicode {
            flags |= JSON_UNESCAPED_UNICODE;
        }
        flags
    }
}

/// Reads/writes json files.
#[derive(Debug)]
pub struct JsonFile {
    /// @var string
    path: String,
    /// @var ?HttpDownloader
    http_downloader: Option<std::rc::Rc<std::cell::RefCell<HttpDownloader>>>,
    /// @var ?IOInterface
    io: Option<std::rc::Rc<std::cell::RefCell<dyn IOInterface>>>,
    /// @var string
    // RefCell so that read() can stay `&self`: PHP's late-bound repository initialize()
    // chains (getRepoName -> count -> initialize -> file read) run from shared contexts.
    indent: std::cell::RefCell<String>,
}

impl JsonFile {
    pub const LAX_SCHEMA: i64 = 1;
    pub const STRICT_SCHEMA: i64 = 2;
    pub const AUTH_SCHEMA: i64 = 3;
    pub const LOCK_SCHEMA: i64 = 4;

    pub const INDENT_DEFAULT: &'static str = "    ";

    /// PHP points these at Composer's res/ directory; here the schemas are embedded in the
    /// binary and SchemaRetriever resolves these URIs back to the constants below. The res/
    /// path segment matters: composer-lock-schema.json reaches the composer schema through the
    /// relative reference "./composer-schema.json", which must resolve to COMPOSER_SCHEMA_PATH.
    pub const COMPOSER_SCHEMA_PATH: &'static str = "shirabe:///res/composer-schema.json";
    pub const LOCK_SCHEMA_PATH: &'static str = "shirabe:///res/composer-lock-schema.json";

    pub const COMPOSER_SCHEMA_JSON: &'static str =
        include_str!("../../../../composer/res/composer-schema.json");
    const LOCK_SCHEMA_JSON: &'static str =
        include_str!("../../../../composer/res/composer-lock-schema.json");

    /// Initializes json file reader/parser.
    ///
    /// @param  string                    $path           path to a lockfile
    /// @param  ?HttpDownloader           $httpDownloader required for loading http/https json files
    /// @throws \InvalidArgumentException
    pub fn new(
        path: String,
        http_downloader: Option<std::rc::Rc<std::cell::RefCell<HttpDownloader>>>,
        io: Option<std::rc::Rc<std::cell::RefCell<dyn IOInterface>>>,
    ) -> anyhow::Result<Self> {
        if http_downloader.is_none() && preg_match(php_regex!(r"{^https?://}i"), &path).is_some() {
            return Err(InvalidArgumentException::new(
                "http urls require a HttpDownloader instance to be passed".to_string(),
            )
            .into());
        }
        Ok(Self {
            path,
            http_downloader,
            io,
            indent: std::cell::RefCell::new(Self::INDENT_DEFAULT.to_string()),
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
    pub fn read(&self) -> anyhow::Result<PhpMixed> {
        let json: Option<String> = match (|| -> anyhow::Result<Option<String>> {
            if let Some(http_downloader) = &self.http_downloader {
                Ok(http_downloader
                    .borrow_mut()
                    .get(&self.path, indexmap::IndexMap::new())?
                    .get_body()
                    .map(|s| s.to_string()))
            } else {
                if !Filesystem::is_readable(&self.path) {
                    return Err(RuntimeException::new(format!(
                        "The file \"{}\" is not readable.",
                        self.path
                    ))
                    .into());
                }
                if let Some(io) = &self.io
                    && io.is_debug()
                {
                    let mut realpath_info = String::new();
                    if let Some(realpath) = realpath(&self.path)
                        && realpath != self.path
                    {
                        realpath_info = format!(" ({})", realpath);
                    }
                    io.write_error3(
                        &format!("Reading {}{}", self.path, realpath_info),
                        true,
                        io_interface::NORMAL,
                    );
                }
                Ok(file_get_contents(&self.path))
            }
        })() {
            Ok(j) => j,
            Err(e) => {
                if let Some(te) = e.catch::<TransportException>() {
                    let message = te.get_message().to_string();
                    return Err(RuntimeException::with_code_and_previous(
                        message,
                        0,
                        Some(std::sync::Arc::new(e)),
                    )
                    .into());
                }
                // `catch (\Exception)` leaves an \Error to propagate.
                if e.is_instanceof::<shirabe_php_shim::Error>() {
                    return Err(e);
                }
                return Err(RuntimeException::new(format!(
                    "Could not read {}\n\n{}",
                    self.path, e
                ))
                .into());
            }
        };

        let json = match json {
            Some(j) => j,
            None => {
                return Err(RuntimeException::new(format!("Could not read {}", self.path)).into());
            }
        };

        *self.indent.borrow_mut() = Self::detect_indenting(Some(&json));

        Self::parse_json(Some(&json), Some(&self.path))
    }

    pub fn write(&self, hash: PhpMixed) -> anyhow::Result<()> {
        self.write_with_options(hash, JsonEncodeOptions::default())
    }

    pub fn write_with_options(
        &self,
        hash: PhpMixed,
        options: JsonEncodeOptions,
    ) -> anyhow::Result<()> {
        let options = JsonEncodeOptions {
            indent: self.indent.borrow().clone(),
            ..options
        };

        if self.path == "php://memory" {
            file_put_contents(
                &self.path,
                Self::encode_with_options(&hash, options)?.as_bytes(),
            );

            return Ok(());
        }

        let dir = dirname(&self.path);
        if !is_dir(&dir) {
            if file_exists(&dir) {
                return Err(UnexpectedValueException::new(format!(
                    "{} exists and is not a directory.",
                    realpath(&dir).unwrap_or_default(),
                ))
                .into());
            }
            // PHP: @mkdir($dir, 0777, true)
            if !Silencer::call(|| Ok(mkdir(&dir, 0o777, true).is_ok())).unwrap_or(false) {
                return Err(UnexpectedValueException::new(format!(
                    "{} does not exist and could not be created.",
                    dir
                ))
                .into());
            }
        }

        let mut retries = 3;
        while retries > 0 {
            retries -= 1;
            let attempt: anyhow::Result<()> = (|| -> anyhow::Result<()> {
                self.file_put_contents_if_modified(
                    &self.path,
                    &format!(
                        "{}{}",
                        Self::encode_with_options(&hash, options.clone())?,
                        if options.pretty_print { "\n" } else { "" },
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
    fn file_put_contents_if_modified(
        &self,
        path: &str,
        content: &str,
    ) -> anyhow::Result<Option<i64>> {
        // PHP: @file_get_contents($path)
        let current_content = Silencer::call(|| Ok(file_get_contents(path)))
            .ok()
            .flatten();
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
    pub fn validate_schema(&self, schema: i64, schema_file: Option<&str>) -> anyhow::Result<bool> {
        if !Filesystem::is_readable(&self.path) {
            return Err(RuntimeException::new(format!(
                "The file \"{}\" is not readable.",
                self.path
            ))
            .into());
        }
        let content = file_get_contents(&self.path).unwrap_or_default();
        let data = json_decode_obj(&content)?;

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
    pub fn validate_json_schema(
        source: &str,
        data: &PhpMixed,
        schema: i64,
        schema_file: Option<&str>,
    ) -> anyhow::Result<bool> {
        let mut is_composer_schema_file = false;
        let mut schema_file = match schema_file {
            Some(f) => f.to_string(),
            None => {
                if schema == Self::LOCK_SCHEMA {
                    Self::LOCK_SCHEMA_PATH.to_string()
                } else {
                    is_composer_schema_file = true;
                    Self::COMPOSER_SCHEMA_PATH.to_string()
                }
            }
        };

        // Prepend with file:// only when not using a special schema already (e.g. in the phar)
        if strpos(&schema_file, "://").is_none() {
            schema_file = format!("file://{}", schema_file);
        }

        // PHP: $schemaData = (object) ['$ref' => $schemaFile, '$schema' => "https://json-schema.org/draft-04/schema#"];
        // A string-keyed `PhpMixed::Array` serializes as a JSON object, matching the (object) cast.
        let mut schema_data: PhpMixed = {
            let mut m = indexmap::IndexMap::new();
            m.insert("$ref".to_string(), PhpMixed::String(schema_file.clone()));
            m.insert(
                "$schema".to_string(),
                PhpMixed::String("https://json-schema.org/draft-04/schema#".to_string()),
            );
            PhpMixed::Array(m)
        };

        if schema == Self::STRICT_SCHEMA && is_composer_schema_file {
            schema_data = json_decode_obj(Self::COMPOSER_SCHEMA_JSON)?;
            if let PhpMixed::Object(map) = &mut schema_data {
                map.insert("additionalProperties".to_string(), PhpMixed::Bool(false));
                map.insert(
                    "required".to_string(),
                    PhpMixed::List(vec![
                        PhpMixed::String("name".to_string()),
                        PhpMixed::String("description".to_string()),
                    ]),
                );
            }
        } else if schema == Self::AUTH_SCHEMA && is_composer_schema_file {
            let mut m = indexmap::IndexMap::new();
            m.insert(
                "$ref".to_string(),
                PhpMixed::String(format!("{}#/properties/config", schema_file)),
            );
            m.insert(
                "$schema".to_string(),
                PhpMixed::String("https://json-schema.org/draft-04/schema#".to_string()),
            );
            schema_data = PhpMixed::Array(m);
        }

        // convert assoc arrays to objects
        let schema_value = serde_json::to_value(&schema_data)?;
        let data_value = serde_json::to_value(data)?;
        let validator = jsonschema::options()
            .with_retriever(SchemaRetriever)
            .build(&schema_value)
            .map_err(|e| anyhow::anyhow!("{e}"))?;

        let errors: Vec<String> = validator
            .iter_errors(&data_value)
            .map(|error| {
                let mut property = error
                    .instance_path()
                    .as_str()
                    .trim_start_matches('/')
                    .replace('/', ".");
                // A missing required property is reported against its parent object, so the
                // instance path is the parent (empty at the root). Composer points the error at
                // the missing property itself, so append its name to match the `PROPERTY : MESSAGE`
                // shape.
                if let jsonschema::error::ValidationErrorKind::Required { property: missing } =
                    error.kind()
                    && let Some(name) = missing.as_str()
                {
                    if property.is_empty() {
                        property = name.to_string();
                    } else {
                        property = format!("{}.{}", property, name);
                    }
                }
                if property.is_empty() {
                    error.to_string()
                } else {
                    format!("{} : {}", property, error)
                }
            })
            .collect();

        if !errors.is_empty() {
            return Err(JsonValidationException::new(
                format!("\"{}\" does not match the expected JSON schema", source),
                errors,
            )
            .into());
        }

        Ok(true)
    }

    pub fn encode<T: serde::Serialize + ?Sized>(data: &T) -> anyhow::Result<String> {
        Self::encode_with_options(data, JsonEncodeOptions::default())
    }

    pub fn encode_with_options<T: serde::Serialize + ?Sized>(
        data: &T,
        options: JsonEncodeOptions,
    ) -> anyhow::Result<String> {
        let json = json_encode_ex(data, options.to_flags())
            .map_err(|err| RuntimeException::new(format!("JSON encoding failed: {}", err)))?;

        if options.pretty_print && options.indent != Self::INDENT_DEFAULT {
            // Pretty printing and not using default indentation
            let indent_owned = options.indent;
            return Ok(preg_replace_callback(
                php_regex!(r"#^ {4,}#m"),
                move |m: &PregMatches| -> anyhow::Result<String> {
                    let whole = m.get(0).unwrap_or("");
                    Ok(str_repeat(&indent_owned, (strlen(whole) / 4) as usize))
                },
                &json,
            )
            .expect("the replacement callback cannot fail"));
        }

        Ok(json)
    }

    /// Parses json string and returns hash.
    ///
    /// @param null|string $json json string
    /// @param string $file the json file
    ///
    /// @throws ParsingException
    pub fn parse_json(json: Option<&str>, file: Option<&str>) -> anyhow::Result<PhpMixed> {
        let json = match json {
            None => return Ok(PhpMixed::Null),
            Some(j) => j,
        };
        let mut data = json_decode_assoc(json)?;
        // PHP: `null === $data && JSON_ERROR_NONE !== json_last_error()`, i.e. the decode produced
        // null because of an error rather than because the input was the literal `null`.
        // json_decode_assoc swallows the error into PhpMixed::Null, so detect the failure by
        // comparing the source against `null`, mirroring validateSchema's own
        // `'null' !== $content` check.
        if matches!(data, PhpMixed::Null) && json != "null" {
            // attempt resolving simple conflicts in lock files so that one can run `composer update --lock` and get a valid lock file
            if let Some(file) = file
                && file.ends_with(".lock")
                && json.contains("\"content-hash\"")
            {
                let mut count: usize = 0;
                let replaced = preg_replace2(
                    php_regex!(
                        r#"{\r?\n<<<<<<< [^\r\n]+\r?\n\s+"content-hash": *"[0-9a-f]+", *\r?\n(?:\|{7} [^\r\n]+\r?\n\s+"content-hash": *"[0-9a-f]+", *\r?\n)?=======\r?\n\s+"content-hash": *"[0-9a-f]+", *\r?\n>>>>>>> [^\r\n]+(\r?\n)}"#
                    ),
                    "    \"content-hash\": \"VCS merge conflict detected. Please run `composer update --lock`.\",$1",
                    json,
                    -1,
                    Some(&mut count),
                );
                if count == 1 {
                    data = json_decode_assoc(&replaced)?;
                    if !matches!(data, PhpMixed::Null) {
                        return Ok(data);
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
    fn validate_syntax(json: &str, file: Option<&str>) -> anyhow::Result<bool> {
        // TODO(php-semantics): make json_decode() returns an error object with details.
        let error = match serde_json::from_str::<serde_json::Value>(json) {
            Ok(_) => {
                // TODO(bytes): Rust's &str is guaranteed as UTF-8, but PHP string is not. Change `json`
                // to &[u8] and check UTF-8 validity here.

                // if (defined('JSON_ERROR_UTF8') && JSON_ERROR_UTF8 === json_last_error()) {
                //     if ($file === null) {
                //         throw new \UnexpectedValueException('The input is not UTF-8, could not parse as JSON');
                //     } else {
                //         throw new \UnexpectedValueException('"' . $file . '" is not UTF-8, could not parse as JSON');
                //     }
                // }

                return Ok(true);
            }
            Err(e) => e,
        };

        let details = ParsingExceptionDetails {
            line: Some(error.line() as i64),
            ..Default::default()
        };
        Err(match file {
            None => ParsingException::new(
                format!("The input does not contain valid JSON\n{error}"),
                details,
            ),
            Some(file) => ParsingException::new(
                format!("\"{file}\" does not contain valid JSON\n{error}"),
                details,
            ),
        }
        .into())
    }

    pub fn detect_indenting(json: Option<&str>) -> String {
        if let Some(m) = preg_match(php_regex!(r##"#^([ \t]+)"#m"##), json.unwrap_or("")) {
            return m.get(1).unwrap_or_default().to_string();
        }

        Self::INDENT_DEFAULT.to_string()
    }
}

#[derive(Debug)]
struct SchemaRetriever;

impl jsonschema::Retrieve for SchemaRetriever {
    fn retrieve(
        &self,
        uri: &jsonschema::Uri<String>,
    ) -> anyhow::Result<serde_json::Value, Box<dyn std::error::Error + Send + Sync>> {
        match uri.scheme().as_str() {
            "shirabe" => {
                let contents = match uri.path().as_str() {
                    "/res/composer-schema.json" => JsonFile::COMPOSER_SCHEMA_JSON,
                    "/res/composer-lock-schema.json" => JsonFile::LOCK_SCHEMA_JSON,
                    path => return Err(format!("Unknown embedded resource {path}").into()),
                };
                Ok(serde_json::from_str(contents)?)
            }
            "file" => {
                let file = std::fs::File::open(uri.path().as_str())?;
                Ok(serde_json::from_reader(std::io::BufReader::new(file))?)
            }
            scheme => Err(format!("Unknown scheme {scheme}").into()),
        }
    }
}
