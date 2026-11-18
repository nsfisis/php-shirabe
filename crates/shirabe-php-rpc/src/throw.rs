use crate::codec::protocol_value;
use crate::value::RpcValue;
use bstr::BString;
use indexmap::IndexMap;

/// Rebuilds the ported exception a [`PhpThrow`] describes.
static THROW_REVIVER: std::sync::OnceLock<fn(PhpThrow) -> anyhow::Error> =
    std::sync::OnceLock::new();

/// Registers the function that rebuilds the ported exception a [`PhpThrow`] from the worker
/// describes. The exception types live in the crates that port them, which this crate does not
/// depend on. A second registration is ignored.
pub fn set_throw_reviver(revive: fn(PhpThrow) -> anyhow::Error) {
    let _ = THROW_REVIVER.set(revive);
}

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
    pub properties: Box<IndexMap<BString, RpcValue>>,
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

    /// The error the exception stands for on the Rust side: the ported exception the function
    /// [`set_throw_reviver`] registered rebuilds from it.
    pub fn into_error(self) -> anyhow::Error {
        let revive = THROW_REVIVER
            .get()
            .expect("an exception crossed from the PHP worker before a reviver was registered");
        revive(self)
    }

    /// The exception as it crosses the boundary wherever it does — as the payload of a `Throw`
    /// frame, a promise rejection, or a value an entry point answers with: `[class, message, code,
    /// properties, file, line, parents]`. `\Shirabe\ThrowableRecord::fromThrowable()` writes the
    /// same list on the PHP side.
    pub fn to_rpc_value(&self) -> RpcValue {
        RpcValue::List(self.payload_fields())
    }

    /// Rebuilds the exception [`PhpThrow::to_rpc_value`] describes. Both sides write the
    /// throwable record in the same commit of Shirabe, so one that does not decode is a protocol
    /// violation.
    pub fn from_rpc_value(value: RpcValue) -> PhpThrow {
        PhpThrow::from_payload_fields(protocol_value(&value, "a throwable record"))
    }

    pub(crate) fn payload_fields(&self) -> Vec<RpcValue> {
        vec![
            RpcValue::string(self.exception_class.clone()),
            RpcValue::string(self.message.clone()),
            RpcValue::Int(self.code),
            RpcValue::Array((*self.properties).clone()),
            RpcValue::string(self.file.clone()),
            RpcValue::Int(self.line),
            RpcValue::List(self.parents.iter().cloned().map(RpcValue::string).collect()),
        ]
    }

    pub(crate) fn from_payload_fields(fields: Vec<RpcValue>) -> PhpThrow {
        let Ok([class, message, code, properties, file, line, parents]) =
            <[RpcValue; 7]>::try_from(fields)
        else {
            panic!("PHP RPC: protocol violation — a throwable record has not 7 fields");
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
