//! ref: composer/vendor/symfony/console/Input/StringInput.php sha1:eaf7d8184333badfa4275d8a44a50612768b7004

use crate::exception::InvalidArgumentException;
use crate::input::{
    ArgvInput, InputDefinition, InputInterface, InputInterfaceByName, InputValue,
    StreamableInputInterface,
};
use bstr::{BStr, BString};
use indexmap::IndexMap;
use shirabe_php_shim::{
    PhpResource, bconcat, php_regex, php_str, preg_match, str_replace_arr, stripcslashes, strlen,
    substr,
};

/// StringInput represents an input provided as a string.
///
/// Usage:
///
/// ```php
/// $input = new StringInput('foo --bar="foobar"');
/// ```
#[derive(Debug, Clone)]
pub struct StringInput {
    inner: ArgvInput,
}

impl StringInput {
    pub const REGEX_STRING: &'static BStr = php_str(br#"([^\s]+?)(?:\s|(?<!\\)"|(?<!\\)'|$)"#);
    pub const REGEX_UNQUOTED_STRING: &'static BStr = php_str(br#"([^\s\\]+?)"#);
    pub const REGEX_QUOTED_STRING: &'static BStr =
        php_str(br#"(?:"([^"\\]*(?:\\.[^"\\]*)*)"|'([^'\\]*(?:\\.[^'\\]*)*)')"#);

    pub fn new(input: impl AsRef<[u8]>) -> anyhow::Result<Self> {
        // parent::__construct([])
        let inner = ArgvInput::new(Some(vec![]), None)?;

        let mut string_input = StringInput { inner };

        let tokens = string_input.tokenize(BStr::new(input.as_ref()))?;
        string_input.inner.set_tokens(tokens);

        Ok(string_input)
    }

    /// Tokenizes a string.
    fn tokenize(&self, input: &BStr) -> anyhow::Result<Vec<BString>> {
        let bytes: &[u8] = input;
        let mut tokens: Vec<BString> = vec![];
        let length = strlen(input);
        let mut cursor: i64 = 0;
        let mut token: Option<BString> = None;
        while cursor < length {
            if bytes[cursor as usize] == b'\\' {
                cursor += 1;
                let mut t = token.unwrap_or_default();
                if let Some(b) = bytes.get(cursor as usize) {
                    t.push(*b);
                }
                token = Some(t);
                cursor += 1;
                continue;
            }

            // Regex pattern compatibility:
            // PHP runs these patterns anchored (`A`) at `$cursor`, so each one must match starting
            // exactly there. The `regex` crate anchors only at the head of the haystack, so the
            // search runs over the part of the input that begins at the cursor and each pattern
            // carries a leading `^` instead of the `A` modifier.
            let rest = &bytes[cursor as usize..];

            if let Some(m) = preg_match(php_regex!(r"/^\s+/"), rest) {
                if token.is_some() {
                    tokens.push(token.take().unwrap());
                }
                cursor += strlen(m.get(0).unwrap_or_default());
            } else if let Some(m) = preg_match(
                bconcat!(
                    r#"/^([^="'\s]+?)(=?)("#,
                    Self::REGEX_QUOTED_STRING,
                    r#"+)/"#
                ),
                rest,
            ) {
                let inner = substr(m.get(3).unwrap_or_default(), 1, Some(-1));
                let replaced = str_replace_arr(&["\"'", "'\"", "''", "\"\""], "", &inner);
                token = Some(bconcat!(
                    token.unwrap_or_default(),
                    m.get(1).unwrap_or_default(),
                    m.get(2).unwrap_or_default(),
                    stripcslashes(&replaced)
                ));
                cursor += strlen(m.get(0).unwrap_or_default());
            } else if let Some(m) =
                preg_match(bconcat!(r"/^", Self::REGEX_QUOTED_STRING, r"/"), rest)
            {
                token = Some(bconcat!(
                    token.unwrap_or_default(),
                    stripcslashes(substr(m.get(0).unwrap_or_default(), 1, Some(-1)))
                ));
                cursor += strlen(m.get(0).unwrap_or_default());
            } else if let Some(m) =
                preg_match(bconcat!(r"/^", Self::REGEX_UNQUOTED_STRING, r"/"), rest)
            {
                token = Some(bconcat!(
                    token.unwrap_or_default(),
                    m.get(1).unwrap_or_default()
                ));
                cursor += strlen(m.get(0).unwrap_or_default());
            } else {
                // should never happen
                return Err(InvalidArgumentException::new(bconcat!(
                    "Unable to parse input near \"... ",
                    substr(input, cursor, Some(10)),
                    " ...\"."
                ))
                .into());
            }
        }

        if let Some(token) = token {
            tokens.push(token);
        }

        Ok(tokens)
    }
}

impl InputInterface for StringInput {
    fn dup(&self) -> std::rc::Rc<std::cell::RefCell<dyn InputInterface>> {
        std::rc::Rc::new(std::cell::RefCell::new(self.clone()))
    }

    fn get_first_argument(&self) -> Option<BString> {
        self.inner.get_first_argument()
    }

    fn has_parameter_option_bytes(&self, values: &[&BStr], only_params: bool) -> bool {
        InputInterface::has_parameter_option_bytes(&self.inner, values, only_params)
    }

    fn get_parameter_option_bytes(
        &self,
        values: &[&BStr],
        default: InputValue,
        only_params: bool,
    ) -> InputValue {
        InputInterface::get_parameter_option_bytes(&self.inner, values, default, only_params)
    }

    fn bind(&mut self, definition: &InputDefinition) -> anyhow::Result<()> {
        InputInterface::bind(&mut self.inner, definition)
    }

    fn validate(&mut self) -> anyhow::Result<()> {
        self.inner.validate()
    }

    fn get_arguments(&self) -> IndexMap<BString, InputValue> {
        InputInterface::get_arguments(&self.inner)
    }

    fn get_argument_bytes(&self, name: &BStr) -> anyhow::Result<InputValue> {
        self.inner.get_argument(name)
    }

    fn set_argument_bytes(&mut self, name: &BStr, value: InputValue) -> anyhow::Result<()> {
        self.inner.set_argument(name, value)
    }

    fn has_argument_bytes(&self, name: &BStr) -> bool {
        self.inner.has_argument(name)
    }

    fn get_options(&self) -> IndexMap<BString, InputValue> {
        InputInterface::get_options(&self.inner)
    }

    fn get_option_bytes(&self, name: &BStr) -> anyhow::Result<InputValue> {
        self.inner.get_option(name)
    }

    fn set_option_bytes(&mut self, name: &BStr, value: InputValue) -> anyhow::Result<()> {
        self.inner.set_option(name, value)
    }

    fn has_option_bytes(&self, name: &BStr) -> bool {
        self.inner.has_option(name)
    }

    fn is_interactive(&self) -> bool {
        self.inner.is_interactive()
    }

    fn set_interactive(&mut self, interactive: bool) {
        self.inner.set_interactive(interactive)
    }

    fn __to_string(&self) -> BString {
        self.inner.__to_string()
    }

    fn as_streamable(&self) -> Option<&dyn StreamableInputInterface> {
        Some(self)
    }

    fn as_streamable_mut(&mut self) -> Option<&mut dyn StreamableInputInterface> {
        Some(self)
    }
}

impl StreamableInputInterface for StringInput {
    fn set_stream(&mut self, stream: PhpResource) {
        self.inner.set_stream(stream)
    }

    fn get_stream(&self) -> Option<PhpResource> {
        self.inner.get_stream()
    }
}
