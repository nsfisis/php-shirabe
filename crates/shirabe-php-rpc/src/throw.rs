use crate::value::PluginValue;
use crate::wire::protocol_value;
use bstr::BString;
use indexmap::IndexMap;

/// A PHP exception that crossed the RPC boundary (the recoverable failure lane, as opposed to
/// the fatal `anyhow::Error` lane used for a dead worker or a broken channel).
#[derive(Debug, Clone)]
pub struct PhpThrow {
    pub exception_class: BString,
    pub message: BString,
    pub code: i64,
    /// The state the exception carries beyond `message` and `code`, keyed by the property names
    /// its class declares. The receiving side revives them onto the instance it rebuilds.
    ///
    /// Boxed so a `Result<_, PhpThrow>` stays small; every dispatcher returns one.
    pub properties: Box<IndexMap<BString, PluginValue>>,
    /// Where the exception was created, as `getFile()` and `getLine()` report it. Empty and 0
    /// for a failure that was no exception.
    pub file: BString,
    pub line: i64,
    /// The superclasses of `exception_class`, nearest first, so the receiving side can rebuild
    /// the nearest one it has a type for. Empty from Rust to PHP, where every class is known.
    pub parents: Vec<BString>,
}

impl PhpThrow {
    /// An exception of `exception_class` carrying nothing beyond its message and code.
    pub fn new(
        exception_class: impl Into<BString>,
        message: impl Into<BString>,
        code: i64,
    ) -> PhpThrow {
        PhpThrow {
            exception_class: exception_class.into(),
            message: message.into(),
            code,
            properties: Box::new(IndexMap::new()),
            file: BString::default(),
            line: 0,
            parents: Vec::new(),
        }
    }

    pub fn runtime(message: impl Into<BString>) -> PhpThrow {
        PhpThrow::new("RuntimeException", message, 0)
    }

    /// The exception as it crosses the boundary wherever it does — as the payload of a `Throw`
    /// frame, a job rejection, or a value an entry point answers with: `[class, message, code,
    /// properties, file, line, parents]`. `\Shirabe\MaterializedThrowable::describe()` writes the
    /// same list on the PHP side.
    pub fn to_wire(&self) -> PluginValue {
        PluginValue::List(self.wire_fields())
    }

    /// Rebuilds the exception [`PhpThrow::to_wire`] describes. Both sides write the description
    /// in the same commit of Shirabe, so one that does not decode is a protocol violation.
    pub fn from_wire(value: PluginValue) -> PhpThrow {
        PhpThrow::from_wire_fields(protocol_value(&value, "an exception description"))
    }

    pub(crate) fn wire_fields(&self) -> Vec<PluginValue> {
        vec![
            PluginValue::string(self.exception_class.clone()),
            PluginValue::string(self.message.clone()),
            PluginValue::Int(self.code),
            PluginValue::Array((*self.properties).clone()),
            PluginValue::string(self.file.clone()),
            PluginValue::Int(self.line),
            PluginValue::List(
                self.parents
                    .iter()
                    .cloned()
                    .map(PluginValue::string)
                    .collect(),
            ),
        ]
    }

    pub(crate) fn from_wire_fields(fields: Vec<PluginValue>) -> PhpThrow {
        let Ok([class, message, code, properties, file, line, parents]) =
            <[PluginValue; 7]>::try_from(fields)
        else {
            panic!("PHP RPC: protocol violation — an exception description has not 7 fields");
        };
        PhpThrow {
            exception_class: protocol_value(&class, "the exception class"),
            message: protocol_value(&message, "the exception message"),
            code: protocol_value(&code, "the exception code"),
            properties: Box::new(protocol_value(&properties, "the exception properties")),
            file: protocol_value(&file, "the exception file"),
            line: protocol_value(&line, "the exception line"),
            parents: protocol_value(&parents, "the exception parents"),
        }
    }
}

impl std::fmt::Display for PhpThrow {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{}: {}",
            self.exception_class.display_lossy(),
            self.message.display_lossy()
        )
    }
}

impl std::error::Error for PhpThrow {}
