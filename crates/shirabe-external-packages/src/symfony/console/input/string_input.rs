use crate::symfony::console::exception::invalid_argument_exception::InvalidArgumentException;
use crate::symfony::console::input::argv_input::ArgvInput;
use crate::symfony::console::input::input_definition::InputDefinition;
use crate::symfony::console::input::input_interface::InputInterface;
use indexmap::IndexMap;
use shirabe_php_shim::PhpMixed;

/// StringInput represents an input provided as a string.
///
/// Usage:
///
///     $input = new StringInput('foo --bar="foobar"');
#[derive(Debug)]
pub struct StringInput {
    pub(crate) inner: ArgvInput,
}

impl StringInput {
    pub const REGEX_STRING: &'static str = r#"([^\s]+?)(?:\s|(?<!\\)"|(?<!\\)'|$)"#;
    pub const REGEX_UNQUOTED_STRING: &'static str = r#"([^\s\\]+?)"#;
    pub const REGEX_QUOTED_STRING: &'static str =
        r#"(?:"([^"\\]*(?:\\.[^"\\]*)*)"|'([^'\\]*(?:\\.[^'\\]*)*)')"#;

    pub fn new(input: &str) -> anyhow::Result<Self> {
        // parent::__construct([])
        let inner = ArgvInput::new(Some(vec![]), None)?;

        let mut string_input = StringInput { inner };

        let tokens = string_input.tokenize(input)?;
        string_input.inner.set_tokens(tokens);

        Ok(string_input)
    }

    /// Tokenizes a string.
    fn tokenize(&self, input: &str) -> anyhow::Result<Vec<String>> {
        let bytes = input.as_bytes();
        let mut tokens: Vec<String> = vec![];
        let length = shirabe_php_shim::strlen(input);
        let mut cursor: i64 = 0;
        let mut token: Option<String> = None;
        while cursor < length {
            if bytes[cursor as usize] == b'\\' {
                cursor += 1;
                let next: String = match bytes.get(cursor as usize) {
                    Some(b) => String::from_utf8_lossy(&[*b]).into_owned(),
                    None => String::new(),
                };
                token = Some(format!("{}{}", token.unwrap_or_default(), next));
                cursor += 1;
                continue;
            }

            let mut m: Vec<String> = vec![];
            if shirabe_php_shim::preg_match_offset(r"/\s+/A", input, &mut m, 0, cursor) {
                if token.is_some() {
                    tokens.push(token.take().unwrap());
                }
                cursor += shirabe_php_shim::strlen(&m[0]);
            } else if shirabe_php_shim::preg_match_offset(
                &format!(r#"/([^="'\s]+?)(=?)({}+)/A"#, Self::REGEX_QUOTED_STRING),
                input,
                &mut m,
                0,
                cursor,
            ) {
                let inner = shirabe_php_shim::substr(&m[3], 1, Some(-1));
                let replaced =
                    shirabe_php_shim::str_replace_arr(&["\"'", "'\"", "''", "\"\""], "", &inner);
                token = Some(format!(
                    "{}{}{}{}",
                    token.unwrap_or_default(),
                    m[1],
                    m[2],
                    shirabe_php_shim::stripcslashes(&replaced)
                ));
                cursor += shirabe_php_shim::strlen(&m[0]);
            } else if shirabe_php_shim::preg_match_offset(
                &format!(r"/{}/A", Self::REGEX_QUOTED_STRING),
                input,
                &mut m,
                0,
                cursor,
            ) {
                token = Some(format!(
                    "{}{}",
                    token.unwrap_or_default(),
                    shirabe_php_shim::stripcslashes(&shirabe_php_shim::substr(&m[0], 1, Some(-1)))
                ));
                cursor += shirabe_php_shim::strlen(&m[0]);
            } else if shirabe_php_shim::preg_match_offset(
                &format!(r"/{}/A", Self::REGEX_UNQUOTED_STRING),
                input,
                &mut m,
                0,
                cursor,
            ) {
                token = Some(format!("{}{}", token.unwrap_or_default(), m[1]));
                cursor += shirabe_php_shim::strlen(&m[0]);
            } else {
                // should never happen
                return Err(
                    InvalidArgumentException(shirabe_php_shim::InvalidArgumentException {
                        message: shirabe_php_shim::sprintf(
                            "Unable to parse input near \"... %s ...\".",
                            &[PhpMixed::String(shirabe_php_shim::substr(
                                input,
                                cursor,
                                Some(10),
                            ))],
                        ),
                        code: 0,
                    })
                    .into(),
                );
            }
        }

        if let Some(token) = token {
            tokens.push(token);
        }

        Ok(tokens)
    }
}

impl InputInterface for StringInput {
    fn get_first_argument(&self) -> Option<String> {
        self.inner.get_first_argument()
    }

    fn has_parameter_option(&self, values: PhpMixed, only_params: bool) -> bool {
        InputInterface::has_parameter_option(&self.inner, values, only_params)
    }

    fn get_parameter_option(
        &self,
        values: PhpMixed,
        default: PhpMixed,
        only_params: bool,
    ) -> PhpMixed {
        InputInterface::get_parameter_option(&self.inner, values, default, only_params)
    }

    fn bind(&mut self, definition: &InputDefinition) -> anyhow::Result<()> {
        InputInterface::bind(&mut self.inner, definition)
    }

    fn validate(&mut self) -> anyhow::Result<()> {
        self.inner.validate()
    }

    fn get_arguments(&self) -> IndexMap<String, PhpMixed> {
        InputInterface::get_arguments(&self.inner)
    }

    fn get_argument(&self, name: &str) -> anyhow::Result<PhpMixed> {
        self.inner.get_argument(name)
    }

    fn set_argument(&mut self, name: &str, value: PhpMixed) -> anyhow::Result<()> {
        self.inner.set_argument(name, value)
    }

    fn has_argument(&self, name: &str) -> bool {
        self.inner.has_argument(name)
    }

    fn get_options(&self) -> IndexMap<String, PhpMixed> {
        InputInterface::get_options(&self.inner)
    }

    fn get_option(&self, name: &str) -> anyhow::Result<PhpMixed> {
        self.inner.get_option(name)
    }

    fn set_option(&mut self, name: &str, value: PhpMixed) -> anyhow::Result<()> {
        self.inner.set_option(name, value)
    }

    fn has_option(&self, name: &str) -> bool {
        self.inner.has_option(name)
    }

    fn is_interactive(&self) -> bool {
        self.inner.is_interactive()
    }

    fn set_interactive(&mut self, interactive: bool) {
        self.inner.set_interactive(interactive)
    }
}
