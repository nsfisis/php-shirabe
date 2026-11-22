//! Typed conversion between Rust values and `RpcValue`, the one codec every crossing goes
//! through: the arguments and the answer of a call in either direction, and the fields of a
//! composite payload.
//!
//! `FromRpcValue` fails with a `DecodeError`, which reaches a PHP caller that handed in a value of
//! the wrong type as a `TypeError`. A caller for which a malformed value is a Shirabe bug rather
//! than a plugin one (a frame, a payload only Shirabe's own worker code writes) turns that failure
//! into a panic instead.

use crate::value::RpcValue;
use crate::{PhpEntityHandle, PhpObject, PhpThrow, RustEntityHandle};
use bstr::{BStr, BString, ByteSlice as _};
use chrono::TimeZone as _;
use indexmap::IndexMap;
use shirabe_php_shim::{
    PhpMixed, ToPhpString, bformat, display_to_bytes, path_from_bytes, path_to_bytes, php_str,
    utf8_text,
};

/// Encodes a Rust value as the PHP value the receiving side declares.
pub trait ToRpcValue {
    fn to_rpc_value(self) -> RpcValue;
}

/// Decodes the PHP value a Rust value stands for. A value that is absent where PHP would read
/// null (an omitted argument, say) is decoded from `RpcValue::Null`.
pub trait FromRpcValue: Sized {
    fn from_rpc_value(value: &RpcValue) -> Result<Self, DecodeError>;
}

/// A value that does not have the type the receiving side declares.
#[derive(Debug, Clone)]
pub struct DecodeError {
    pub message: BString,
}

impl DecodeError {
    pub fn new(message: impl Into<BString>) -> DecodeError {
        DecodeError {
            message: message.into(),
        }
    }
}

/// A PHP caller that hands in a value of the wrong type gets a `TypeError`, as under
/// `declare(strict_types=1)`.
impl From<DecodeError> for PhpThrow {
    fn from(error: DecodeError) -> PhpThrow {
        PhpThrow::new("TypeError", error.message, 0)
    }
}

/// The failure of a value that does not have the shape `expected` names.
pub fn unexpected(expected: impl AsRef<[u8]>, value: &RpcValue) -> DecodeError {
    DecodeError::new(bformat!(
        "expected {}, got {}",
        expected.as_ref(),
        display_to_bytes(format_args!("{value:?}"))
    ))
}

/// Decodes a value only Shirabe's own code on the other side writes, for which a failure is a
/// protocol violation.
pub fn protocol_value<T: FromRpcValue>(value: &RpcValue, what: impl AsRef<[u8]>) -> T {
    T::from_rpc_value(value).unwrap_or_else(|error| {
        panic!(
            "PHP RPC: protocol violation — {}: {}",
            what.as_ref().as_bstr().display_lossy(),
            error.message.display_lossy()
        )
    })
}

impl ToRpcValue for RpcValue {
    fn to_rpc_value(self) -> RpcValue {
        self
    }
}

impl FromRpcValue for RpcValue {
    fn from_rpc_value(value: &RpcValue) -> Result<Self, DecodeError> {
        Ok(value.clone())
    }
}

impl ToRpcValue for bool {
    fn to_rpc_value(self) -> RpcValue {
        RpcValue::Bool(self)
    }
}

impl FromRpcValue for bool {
    fn from_rpc_value(value: &RpcValue) -> Result<Self, DecodeError> {
        match value {
            RpcValue::Bool(value) => Ok(*value),
            other => Err(unexpected("a bool", other)),
        }
    }
}

impl ToRpcValue for i64 {
    fn to_rpc_value(self) -> RpcValue {
        RpcValue::Int(self)
    }
}

impl FromRpcValue for i64 {
    fn from_rpc_value(value: &RpcValue) -> Result<Self, DecodeError> {
        match value {
            RpcValue::Int(value) => Ok(*value),
            other => Err(unexpected("an int", other)),
        }
    }
}

impl ToRpcValue for f64 {
    fn to_rpc_value(self) -> RpcValue {
        RpcValue::Float(self)
    }
}

impl FromRpcValue for f64 {
    fn from_rpc_value(value: &RpcValue) -> Result<Self, DecodeError> {
        match value {
            RpcValue::Float(value) => Ok(*value),
            other => Err(unexpected("a float", other)),
        }
    }
}

impl ToRpcValue for BString {
    fn to_rpc_value(self) -> RpcValue {
        RpcValue::String(self)
    }
}

impl ToRpcValue for &BStr {
    fn to_rpc_value(self) -> RpcValue {
        RpcValue::String(self.to_owned())
    }
}

impl FromRpcValue for BString {
    fn from_rpc_value(value: &RpcValue) -> Result<Self, DecodeError> {
        match value {
            RpcValue::String(bytes) => Ok(bytes.clone()),
            other => Err(unexpected("a string", other)),
        }
    }
}

impl ToRpcValue for std::path::PathBuf {
    fn to_rpc_value(self) -> RpcValue {
        RpcValue::string(path_to_bytes(&self))
    }
}

impl ToRpcValue for &std::path::Path {
    fn to_rpc_value(self) -> RpcValue {
        RpcValue::string(path_to_bytes(self))
    }
}

impl FromRpcValue for std::path::PathBuf {
    fn from_rpc_value(value: &RpcValue) -> Result<Self, DecodeError> {
        match value {
            RpcValue::String(bytes) => Ok(path_from_bytes(bytes).to_path_buf()),
            other => Err(unexpected("a path", other)),
        }
    }
}

impl ToRpcValue for PhpEntityHandle {
    fn to_rpc_value(self) -> RpcValue {
        RpcValue::PhpEntityHandle(self)
    }
}

impl FromRpcValue for PhpEntityHandle {
    fn from_rpc_value(value: &RpcValue) -> Result<Self, DecodeError> {
        match value {
            RpcValue::PhpEntityHandle(handle) => Ok(handle.clone()),
            other => Err(unexpected("a PHP object", other)),
        }
    }
}

impl ToRpcValue for RustEntityHandle {
    fn to_rpc_value(self) -> RpcValue {
        RpcValue::RustEntityHandle(self)
    }
}

/// The plain data a value of the ported code carries. An RPC value has no shape for an object
/// without a class, so `PhpMixed::Object` crosses as an array.
impl ToRpcValue for PhpMixed {
    fn to_rpc_value(self) -> RpcValue {
        match self {
            PhpMixed::Null => RpcValue::Null,
            PhpMixed::Bool(b) => RpcValue::Bool(b),
            PhpMixed::Int(n) => RpcValue::Int(n),
            PhpMixed::Float(f) => RpcValue::Float(f),
            PhpMixed::String(s) => RpcValue::String(s),
            PhpMixed::List(items) => {
                RpcValue::List(items.into_iter().map(PhpMixed::to_rpc_value).collect())
            }
            PhpMixed::Array(map) | PhpMixed::Object(map) => RpcValue::Array(
                map.into_iter()
                    .map(|(key, value)| (key, value.to_rpc_value()))
                    .collect(),
            ),
        }
    }
}

/// Plain data only: an object record or a handle has no `PhpMixed` counterpart.
impl FromRpcValue for PhpMixed {
    fn from_rpc_value(value: &RpcValue) -> Result<Self, DecodeError> {
        Ok(match value {
            RpcValue::Null => PhpMixed::Null,
            RpcValue::Bool(b) => PhpMixed::Bool(*b),
            RpcValue::Int(n) => PhpMixed::Int(*n),
            RpcValue::Float(f) => PhpMixed::Float(*f),
            RpcValue::String(bytes) => PhpMixed::String(bytes.clone()),
            RpcValue::List(items) => PhpMixed::List(
                items
                    .iter()
                    .map(PhpMixed::from_rpc_value)
                    .collect::<Result<_, _>>()?,
            ),
            RpcValue::Array(map) => PhpMixed::Array(
                map.iter()
                    .map(|(key, value)| Ok((key.clone(), PhpMixed::from_rpc_value(value)?)))
                    .collect::<Result<_, DecodeError>>()?,
            ),
            other @ (RpcValue::PhpObject(_)
            | RpcValue::RustEntityHandle(_)
            | RpcValue::PhpEntityHandle(_)
            | RpcValue::PromiseHandle(_)) => return Err(unexpected("plain data", other)),
        })
    }
}

impl<T: ToRpcValue> ToRpcValue for Option<T> {
    fn to_rpc_value(self) -> RpcValue {
        match self {
            Some(value) => value.to_rpc_value(),
            None => RpcValue::Null,
        }
    }
}

impl<T: FromRpcValue> FromRpcValue for Option<T> {
    fn from_rpc_value(value: &RpcValue) -> Result<Self, DecodeError> {
        match value {
            RpcValue::Null => Ok(None),
            value => Ok(Some(T::from_rpc_value(value)?)),
        }
    }
}

impl<T: ToRpcValue> ToRpcValue for Vec<T> {
    fn to_rpc_value(self) -> RpcValue {
        RpcValue::List(self.into_iter().map(T::to_rpc_value).collect())
    }
}

/// A PHP array as a list, accepting the keyed shape PHP allows anywhere a list is documented.
impl<T: FromRpcValue> FromRpcValue for Vec<T> {
    fn from_rpc_value(value: &RpcValue) -> Result<Self, DecodeError> {
        match value {
            RpcValue::List(items) => items.iter().map(T::from_rpc_value).collect(),
            RpcValue::Array(map) => map.values().map(T::from_rpc_value).collect(),
            other => Err(unexpected("an array", other)),
        }
    }
}

/// An `array<string, T>` as PHP shapes it: an empty map crosses as a list, since an empty PHP
/// array is indistinguishable from an empty list.
impl<K: Into<BString>, T: ToRpcValue> ToRpcValue for IndexMap<K, T> {
    fn to_rpc_value(self) -> RpcValue {
        if self.is_empty() {
            RpcValue::List(Vec::new())
        } else {
            RpcValue::Array(
                self.into_iter()
                    .map(|(key, value)| (key.into(), value.to_rpc_value()))
                    .collect(),
            )
        }
    }
}

/// A PHP array as a map. An empty PHP array is indistinguishable from an empty list, so it
/// decodes here as an empty map; a non-empty list decodes under its int keys.
impl<T: FromRpcValue> FromRpcValue for IndexMap<BString, T> {
    fn from_rpc_value(value: &RpcValue) -> Result<Self, DecodeError> {
        match value {
            RpcValue::Array(map) => map
                .iter()
                .map(|(key, item)| Ok((key.clone(), T::from_rpc_value(item)?)))
                .collect(),
            RpcValue::List(items) => items
                .iter()
                .enumerate()
                .map(|(index, item)| Ok((index.to_php_string(), T::from_rpc_value(item)?)))
                .collect(),
            other => Err(unexpected("an array", other)),
        }
    }
}

/// Defines a struct that crosses the boundary as a PHP array keyed by its field names, deriving
/// `ToRpcValue` and `FromRpcValue` from its fields. The PHP side reads and writes the array
/// under the same names, so a record is the one definition of a composite payload's keys on this
/// side; a missing key or a value of the wrong type is a decoding failure naming the record and the
/// key.
#[macro_export]
macro_rules! rpc_value_record {
    (
        $(#[$attr:meta])*
        $vis:vis struct $name:ident {
            $(
                $(#[$field_attr:meta])*
                $field_vis:vis $field:ident: $ty:ty
            ),* $(,)?
        }
    ) => {
        $(#[$attr])*
        $vis struct $name {
            $(
                $(#[$field_attr])*
                $field_vis $field: $ty,
            )*
        }

        impl $crate::ToRpcValue for $name {
            fn to_rpc_value(self) -> $crate::RpcValue {
                $crate::codec::record_to_rpc_value(vec![
                    $((
                        stringify!($field).as_bytes(),
                        $crate::ToRpcValue::to_rpc_value(self.$field),
                    )),*
                ])
            }
        }

        impl $crate::FromRpcValue for $name {
            fn from_rpc_value(value: &$crate::RpcValue) -> Result<Self, $crate::codec::DecodeError> {
                let record = $crate::codec::record_entries(value, stringify!($name).as_bytes())?;
                Ok($name {
                    $(
                        $field: $crate::codec::record_field(
                            record,
                            stringify!($name).as_bytes(),
                            stringify!($field).as_bytes(),
                        )?,
                    )*
                })
            }
        }
    };
}

#[doc(hidden)]
pub fn record_to_rpc_value(fields: Vec<(&'static [u8], RpcValue)>) -> RpcValue {
    RpcValue::Array(
        fields
            .into_iter()
            .map(|(key, value)| (BString::from(key), value))
            .collect(),
    )
}

#[doc(hidden)]
pub fn record_entries<'a>(
    value: &'a RpcValue,
    record: &[u8],
) -> Result<&'a IndexMap<BString, RpcValue>, DecodeError> {
    match value {
        RpcValue::Array(entries) => Ok(entries),
        other => Err(unexpected(bformat!("a {record} record"), other)),
    }
}

#[doc(hidden)]
pub fn record_field<T: FromRpcValue>(
    entries: &IndexMap<BString, RpcValue>,
    record: &[u8],
    field: &[u8],
) -> Result<T, DecodeError> {
    let value = entries
        .get(field)
        .ok_or_else(|| DecodeError::new(bformat!("the {record} record has no `{field}` entry")))?;
    T::from_rpc_value(value).map_err(|error| {
        DecodeError::new(bformat!(
            "the `{field}` entry of the {record} record: {}",
            error.message
        ))
    })
}

const DATE_TIME_IMMUTABLE_CLASS: &BStr = php_str(b"DateTimeImmutable");
const DATE_TIME_CLASS: &BStr = php_str(b"DateTime");

/// The wall clock of a PHP date, at the microsecond precision the other side can represent.
#[expect(clippy::disallowed_types, reason = "chrono takes a text format")]
const DATE_FORMAT: &str = "%Y-%m-%d %H:%M:%S%.6f";

/// PHP's `timezone_type` for a date carrying a named zone rather than an offset or an
/// abbreviation.
const TIMEZONE_TYPE_IDENTIFIER: i64 = 3;

/// The only zone a date may cross in: the port holds instants as `DateTime<Utc>` and carries no
/// timezone database, so the worker rebases every date on UTC before it is serialized.
const TIMEZONE_UTC: &BStr = php_str(b"UTC");

/// A `\DateTimeInterface` crosses as the object record `serialize()` writes for a
/// `\DateTimeImmutable`.
impl ToRpcValue for chrono::DateTime<chrono::Utc> {
    fn to_rpc_value(self) -> RpcValue {
        let mut object = PhpObject::new(DATE_TIME_IMMUTABLE_CLASS);
        object.set_public(
            "date",
            RpcValue::string(display_to_bytes(self.format(DATE_FORMAT))),
        );
        object.set_public("timezone_type", RpcValue::Int(TIMEZONE_TYPE_IDENTIFIER));
        object.set_public("timezone", RpcValue::string(TIMEZONE_UTC));
        RpcValue::PhpObject(object)
    }
}

impl FromRpcValue for chrono::DateTime<chrono::Utc> {
    fn from_rpc_value(value: &RpcValue) -> Result<Self, DecodeError> {
        let RpcValue::PhpObject(object) = value else {
            return Err(unexpected("a date", value));
        };
        if object.class != DATE_TIME_IMMUTABLE_CLASS && object.class != DATE_TIME_CLASS {
            return Err(DecodeError::new(bformat!(
                "the class `{}` cannot cross the plugin boundary as a date",
                object.class
            )));
        }
        let zone = Option::<BString>::from_rpc_value(object.public("timezone").unwrap_or(&NULL))?;
        if object.public("timezone_type") != Some(&RpcValue::Int(TIMEZONE_TYPE_IDENTIFIER))
            || zone.as_ref().is_none_or(|zone| zone != TIMEZONE_UTC)
        {
            return Err(DecodeError::new(bformat!(
                "a date crosses the plugin boundary rebased on {TIMEZONE_UTC}, got the zone {}",
                display_to_bytes(format_args!("{zone:?}"))
            )));
        }
        let rendered = BString::from_rpc_value(object.public("date").unwrap_or(&NULL))?;
        let rendered = utf8_text(&rendered).ok_or_else(|| {
            DecodeError::new(bformat!(
                "the date {} from the plugin is not text",
                display_to_bytes(format_args!("{rendered:?}"))
            ))
        })?;
        let parsed = chrono::NaiveDateTime::parse_from_str(rendered, DATE_FORMAT).map_err(|e| {
            DecodeError::new(bformat!(
                "the date `{rendered}` from the plugin is not in the expected format: {}",
                display_to_bytes(e)
            ))
        })?;
        Ok(chrono::Utc.from_utc_datetime(&parsed))
    }
}

// TODO(plugin): a date crosses rebased on UTC, so its UTC offset is lost in both directions,
// while PHP hands the plugin the `\DateTimeInterface` with the offset it was created with.
impl ToRpcValue for chrono::DateTime<chrono::FixedOffset> {
    fn to_rpc_value(self) -> RpcValue {
        self.to_utc().to_rpc_value()
    }
}

impl FromRpcValue for chrono::DateTime<chrono::FixedOffset> {
    fn from_rpc_value(value: &RpcValue) -> Result<Self, DecodeError> {
        chrono::DateTime::<chrono::Utc>::from_rpc_value(value).map(|date| date.fixed_offset())
    }
}

/// What an absent property or argument decodes from.
pub const NULL: RpcValue = RpcValue::Null;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::value::serialize;
    use chrono::Timelike as _;

    #[test]
    fn php_mixed_conversions() {
        let mixed = PhpMixed::Array(
            [
                ("a".into(), PhpMixed::Int(1)),
                ("b".into(), PhpMixed::List(vec![PhpMixed::Null])),
            ]
            .into_iter()
            .collect(),
        );
        let value = mixed.clone().to_rpc_value();
        assert_eq!(PhpMixed::from_rpc_value(&value).unwrap(), mixed);

        let handle = RpcValue::PromiseHandle(1);
        assert!(PhpMixed::from_rpc_value(&handle).is_err());
    }

    #[test]
    fn a_classless_object_crosses_as_an_array() {
        let mut map: IndexMap<BString, PhpMixed> = IndexMap::new();
        map.insert(b"a".into(), PhpMixed::Int(1));
        assert_eq!(
            PhpMixed::Object(map.clone()).to_rpc_value(),
            PhpMixed::Array(map).to_rpc_value()
        );
    }

    /// The expected bytes are what PHP writes for
    /// `new DateTimeImmutable('2026-08-07 12:34:56.123456', new DateTimeZone('UTC'))`.
    #[test]
    fn a_date_is_written_as_php_writes_it() {
        let date = chrono::Utc
            .with_ymd_and_hms(2026, 8, 7, 12, 34, 56)
            .unwrap()
            .with_nanosecond(123_456_000)
            .unwrap();
        assert_eq!(
            serialize(&date.to_rpc_value()),
            b"O:17:\"DateTimeImmutable\":3:{s:4:\"date\";s:26:\"2026-08-07 12:34:56.123456\";s:13:\"timezone_type\";i:3;s:8:\"timezone\";s:3:\"UTC\";}".as_slice(),
        );
    }

    #[test]
    fn a_date_round_trips_with_microseconds() {
        let date = chrono::Utc
            .with_ymd_and_hms(2024, 3, 3, 20, 6, 7)
            .unwrap()
            .with_nanosecond(123_456_000)
            .unwrap();
        assert_eq!(
            date,
            chrono::DateTime::<chrono::Utc>::from_rpc_value(&date.to_rpc_value()).unwrap()
        );
    }

    #[test]
    fn a_date_outside_utc_is_rejected() {
        let mut object = PhpObject::new(DATE_TIME_IMMUTABLE_CLASS);
        object.set_public("date", RpcValue::string("2024-03-04 05:06:07.123456"));
        object.set_public("timezone_type", RpcValue::Int(3));
        object.set_public("timezone", RpcValue::string("Asia/Tokyo"));
        let err = chrono::DateTime::<chrono::Utc>::from_rpc_value(&RpcValue::PhpObject(object))
            .unwrap_err();
        assert!(
            err.message.contains_str("Asia/Tokyo"),
            "{}",
            err.message.display_lossy()
        );
    }
}
