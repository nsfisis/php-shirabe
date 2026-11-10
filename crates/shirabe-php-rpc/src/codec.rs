//! Typed conversion between Rust values and `PluginValue`, the one codec every crossing goes
//! through: the arguments and the answer of a call in either direction, and the fields of a
//! composite payload.
//!
//! `FromPluginValue` fails with the `PhpThrow` a PHP caller gets for a value of the wrong shape. A
//! caller for which a malformed value is a Shirabe bug rather than a plugin one (a frame, a
//! payload only Shirabe's own worker code writes) turns that failure into a panic instead.

use crate::value::PluginValue;
use crate::{PhpObjHandle, PhpObject, PhpThrow, RustObjHandle};
use bstr::{BStr, BString, ByteSlice as _};
use chrono::TimeZone as _;
use indexmap::IndexMap;
use shirabe_php_shim::{
    PhpMixed, ToPhpString, bformat, display_to_bytes, path_from_bytes, path_to_bytes, php_str,
    utf8_text,
};

/// Encodes a Rust value as the PHP value the receiving side declares.
pub trait ToPluginValue {
    fn to_plugin_value(self) -> PluginValue;
}

/// Decodes the PHP value a Rust value stands for. A value that is absent where PHP would read
/// null (an omitted argument, say) is decoded from `PluginValue::Null`.
pub trait FromPluginValue: Sized {
    fn from_plugin_value(value: &PluginValue) -> Result<Self, PhpThrow>;
}

/// The failure of a value that does not have the shape `expected` names.
pub fn unexpected(expected: impl AsRef<[u8]>, value: &PluginValue) -> PhpThrow {
    PhpThrow::runtime(bformat!(
        "expected {}, got {}",
        expected.as_ref(),
        display_to_bytes(format_args!("{value:?}"))
    ))
}

/// Decodes a value only Shirabe's own code on the other side writes, for which a failure is a
/// protocol violation.
pub fn protocol_value<T: FromPluginValue>(value: &PluginValue, what: impl AsRef<[u8]>) -> T {
    T::from_plugin_value(value).unwrap_or_else(|throw| {
        panic!(
            "PHP RPC: protocol violation — {}: {}",
            what.as_ref().as_bstr().display_lossy(),
            throw.message.display_lossy()
        )
    })
}

impl ToPluginValue for PluginValue {
    fn to_plugin_value(self) -> PluginValue {
        self
    }
}

impl FromPluginValue for PluginValue {
    fn from_plugin_value(value: &PluginValue) -> Result<Self, PhpThrow> {
        Ok(value.clone())
    }
}

impl ToPluginValue for bool {
    fn to_plugin_value(self) -> PluginValue {
        PluginValue::Bool(self)
    }
}

impl FromPluginValue for bool {
    fn from_plugin_value(value: &PluginValue) -> Result<Self, PhpThrow> {
        match value {
            PluginValue::Bool(value) => Ok(*value),
            other => Err(unexpected("a bool", other)),
        }
    }
}

impl ToPluginValue for i64 {
    fn to_plugin_value(self) -> PluginValue {
        PluginValue::Int(self)
    }
}

impl FromPluginValue for i64 {
    fn from_plugin_value(value: &PluginValue) -> Result<Self, PhpThrow> {
        match value {
            PluginValue::Int(value) => Ok(*value),
            other => Err(unexpected("an int", other)),
        }
    }
}

impl ToPluginValue for f64 {
    fn to_plugin_value(self) -> PluginValue {
        PluginValue::Float(self)
    }
}

impl FromPluginValue for f64 {
    fn from_plugin_value(value: &PluginValue) -> Result<Self, PhpThrow> {
        match value {
            PluginValue::Float(value) => Ok(*value),
            other => Err(unexpected("a float", other)),
        }
    }
}

impl ToPluginValue for BString {
    fn to_plugin_value(self) -> PluginValue {
        PluginValue::String(self)
    }
}

impl ToPluginValue for &BStr {
    fn to_plugin_value(self) -> PluginValue {
        PluginValue::String(self.to_owned())
    }
}

impl FromPluginValue for BString {
    fn from_plugin_value(value: &PluginValue) -> Result<Self, PhpThrow> {
        match value {
            PluginValue::String(bytes) => Ok(bytes.clone()),
            other => Err(unexpected("a string", other)),
        }
    }
}

impl ToPluginValue for std::path::PathBuf {
    fn to_plugin_value(self) -> PluginValue {
        PluginValue::string(path_to_bytes(&self))
    }
}

impl ToPluginValue for &std::path::Path {
    fn to_plugin_value(self) -> PluginValue {
        PluginValue::string(path_to_bytes(self))
    }
}

impl FromPluginValue for std::path::PathBuf {
    fn from_plugin_value(value: &PluginValue) -> Result<Self, PhpThrow> {
        match value {
            PluginValue::String(bytes) => Ok(path_from_bytes(bytes).to_path_buf()),
            other => Err(unexpected("a path", other)),
        }
    }
}

impl ToPluginValue for PhpObjHandle {
    fn to_plugin_value(self) -> PluginValue {
        PluginValue::PhpHandle(self)
    }
}

impl FromPluginValue for PhpObjHandle {
    fn from_plugin_value(value: &PluginValue) -> Result<Self, PhpThrow> {
        match value {
            PluginValue::PhpHandle(handle) => Ok(handle.clone()),
            other => Err(unexpected("a PHP object", other)),
        }
    }
}

impl ToPluginValue for RustObjHandle {
    fn to_plugin_value(self) -> PluginValue {
        PluginValue::RustHandle(self)
    }
}

/// The plain data a value of the ported code carries. A plugin value has no shape for an object
/// without a class, so `PhpMixed::Object` crosses as an array.
impl ToPluginValue for PhpMixed {
    fn to_plugin_value(self) -> PluginValue {
        match self {
            PhpMixed::Null => PluginValue::Null,
            PhpMixed::Bool(b) => PluginValue::Bool(b),
            PhpMixed::Int(n) => PluginValue::Int(n),
            PhpMixed::Float(f) => PluginValue::Float(f),
            PhpMixed::String(s) => PluginValue::String(s),
            PhpMixed::List(items) => {
                PluginValue::List(items.into_iter().map(PhpMixed::to_plugin_value).collect())
            }
            PhpMixed::Array(map) | PhpMixed::Object(map) => PluginValue::Array(
                map.into_iter()
                    .map(|(key, value)| (key, value.to_plugin_value()))
                    .collect(),
            ),
        }
    }
}

/// Plain data only: an object record or a handle has no `PhpMixed` counterpart.
impl FromPluginValue for PhpMixed {
    fn from_plugin_value(value: &PluginValue) -> Result<Self, PhpThrow> {
        Ok(match value {
            PluginValue::Null => PhpMixed::Null,
            PluginValue::Bool(b) => PhpMixed::Bool(*b),
            PluginValue::Int(n) => PhpMixed::Int(*n),
            PluginValue::Float(f) => PhpMixed::Float(*f),
            PluginValue::String(bytes) => PhpMixed::String(bytes.clone()),
            PluginValue::List(items) => PhpMixed::List(
                items
                    .iter()
                    .map(PhpMixed::from_plugin_value)
                    .collect::<Result<_, _>>()?,
            ),
            PluginValue::Array(map) => PhpMixed::Array(
                map.iter()
                    .map(|(key, value)| Ok((key.clone(), PhpMixed::from_plugin_value(value)?)))
                    .collect::<Result<_, PhpThrow>>()?,
            ),
            other @ (PluginValue::PhpObject(_)
            | PluginValue::RustHandle(_)
            | PluginValue::PhpHandle(_)
            | PluginValue::Job(_)) => return Err(unexpected("plain data", other)),
        })
    }
}

impl<T: ToPluginValue> ToPluginValue for Option<T> {
    fn to_plugin_value(self) -> PluginValue {
        match self {
            Some(value) => value.to_plugin_value(),
            None => PluginValue::Null,
        }
    }
}

impl<T: FromPluginValue> FromPluginValue for Option<T> {
    fn from_plugin_value(value: &PluginValue) -> Result<Self, PhpThrow> {
        match value {
            PluginValue::Null => Ok(None),
            value => Ok(Some(T::from_plugin_value(value)?)),
        }
    }
}

impl<T: ToPluginValue> ToPluginValue for Vec<T> {
    fn to_plugin_value(self) -> PluginValue {
        PluginValue::List(self.into_iter().map(T::to_plugin_value).collect())
    }
}

/// A PHP array as a list, accepting the keyed shape PHP allows anywhere a list is documented.
impl<T: FromPluginValue> FromPluginValue for Vec<T> {
    fn from_plugin_value(value: &PluginValue) -> Result<Self, PhpThrow> {
        match value {
            PluginValue::List(items) => items.iter().map(T::from_plugin_value).collect(),
            PluginValue::Array(map) => map.values().map(T::from_plugin_value).collect(),
            other => Err(unexpected("an array", other)),
        }
    }
}

/// An `array<string, T>` as PHP shapes it: an empty map crosses as a list, since an empty PHP
/// array is indistinguishable from an empty list on the wire.
impl<K: Into<BString>, T: ToPluginValue> ToPluginValue for IndexMap<K, T> {
    fn to_plugin_value(self) -> PluginValue {
        if self.is_empty() {
            PluginValue::List(Vec::new())
        } else {
            PluginValue::Array(
                self.into_iter()
                    .map(|(key, value)| (key.into(), value.to_plugin_value()))
                    .collect(),
            )
        }
    }
}

/// A PHP array as a map. An empty PHP array is indistinguishable from an empty list on the
/// wire, so it decodes here as an empty map; a non-empty list decodes under its int keys.
impl<T: FromPluginValue> FromPluginValue for IndexMap<BString, T> {
    fn from_plugin_value(value: &PluginValue) -> Result<Self, PhpThrow> {
        match value {
            PluginValue::Array(map) => map
                .iter()
                .map(|(key, item)| Ok((key.clone(), T::from_plugin_value(item)?)))
                .collect(),
            PluginValue::List(items) => items
                .iter()
                .enumerate()
                .map(|(index, item)| Ok((index.to_php_string(), T::from_plugin_value(item)?)))
                .collect(),
            other => Err(unexpected("an array", other)),
        }
    }
}

/// Defines a struct that crosses the boundary as a PHP array keyed by its field names, deriving
/// `ToPluginValue` and `FromPluginValue` from its fields. The PHP side reads and writes the array
/// under the same names, so a record is the one definition of a composite payload's keys on this
/// side; a missing key or a value of the wrong type is a decoding failure naming the record and the
/// key.
#[macro_export]
macro_rules! plugin_value_record {
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

        impl $crate::ToPluginValue for $name {
            fn to_plugin_value(self) -> $crate::PluginValue {
                $crate::codec::record_to_plugin_value(vec![
                    $((
                        stringify!($field).as_bytes(),
                        $crate::ToPluginValue::to_plugin_value(self.$field),
                    )),*
                ])
            }
        }

        impl $crate::FromPluginValue for $name {
            fn from_plugin_value(value: &$crate::PluginValue) -> Result<Self, $crate::PhpThrow> {
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
pub fn record_to_plugin_value(fields: Vec<(&'static [u8], PluginValue)>) -> PluginValue {
    PluginValue::Array(
        fields
            .into_iter()
            .map(|(key, value)| (BString::from(key), value))
            .collect(),
    )
}

#[doc(hidden)]
pub fn record_entries<'a>(
    value: &'a PluginValue,
    record: &[u8],
) -> Result<&'a IndexMap<BString, PluginValue>, PhpThrow> {
    match value {
        PluginValue::Array(entries) => Ok(entries),
        other => Err(unexpected(bformat!("a {record} record"), other)),
    }
}

#[doc(hidden)]
pub fn record_field<T: FromPluginValue>(
    entries: &IndexMap<BString, PluginValue>,
    record: &[u8],
    field: &[u8],
) -> Result<T, PhpThrow> {
    let value = entries
        .get(field)
        .ok_or_else(|| PhpThrow::runtime(bformat!("the {record} record has no `{field}` entry")))?;
    T::from_plugin_value(value).map_err(|throw| PhpThrow {
        message: bformat!(
            "the `{field}` entry of the {record} record: {}",
            throw.message
        ),
        ..throw
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
impl ToPluginValue for chrono::DateTime<chrono::Utc> {
    fn to_plugin_value(self) -> PluginValue {
        let mut object = PhpObject::new(DATE_TIME_IMMUTABLE_CLASS);
        object.set_public(
            "date",
            PluginValue::string(display_to_bytes(self.format(DATE_FORMAT))),
        );
        object.set_public("timezone_type", PluginValue::Int(TIMEZONE_TYPE_IDENTIFIER));
        object.set_public("timezone", PluginValue::string(TIMEZONE_UTC));
        PluginValue::PhpObject(object)
    }
}

impl FromPluginValue for chrono::DateTime<chrono::Utc> {
    fn from_plugin_value(value: &PluginValue) -> Result<Self, PhpThrow> {
        let PluginValue::PhpObject(object) = value else {
            return Err(unexpected("a date", value));
        };
        if object.class != DATE_TIME_IMMUTABLE_CLASS && object.class != DATE_TIME_CLASS {
            return Err(PhpThrow::runtime(bformat!(
                "the class `{}` cannot cross the plugin boundary as a date",
                object.class
            )));
        }
        let zone =
            Option::<BString>::from_plugin_value(object.public("timezone").unwrap_or(&NULL))?;
        if object.public("timezone_type") != Some(&PluginValue::Int(TIMEZONE_TYPE_IDENTIFIER))
            || zone.as_ref().is_none_or(|zone| zone != TIMEZONE_UTC)
        {
            return Err(PhpThrow::runtime(bformat!(
                "a date crosses the plugin boundary rebased on {TIMEZONE_UTC}, got the zone {}",
                display_to_bytes(format_args!("{zone:?}"))
            )));
        }
        let rendered = BString::from_plugin_value(object.public("date").unwrap_or(&NULL))?;
        let rendered = utf8_text(&rendered).ok_or_else(|| {
            PhpThrow::runtime(bformat!(
                "the date {} from the plugin is not text",
                display_to_bytes(format_args!("{rendered:?}"))
            ))
        })?;
        let parsed = chrono::NaiveDateTime::parse_from_str(rendered, DATE_FORMAT).map_err(|e| {
            PhpThrow::runtime(bformat!(
                "the date `{rendered}` from the plugin is not in the expected format: {}",
                display_to_bytes(e)
            ))
        })?;
        Ok(chrono::Utc.from_utc_datetime(&parsed))
    }
}

/// What an absent property or argument decodes from.
pub const NULL: PluginValue = PluginValue::Null;

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
        let value = mixed.clone().to_plugin_value();
        assert_eq!(PhpMixed::from_plugin_value(&value).unwrap(), mixed);

        let handle = PluginValue::Job(1);
        assert!(PhpMixed::from_plugin_value(&handle).is_err());
    }

    #[test]
    fn a_classless_object_crosses_as_an_array() {
        let mut map: IndexMap<BString, PhpMixed> = IndexMap::new();
        map.insert(b"a".into(), PhpMixed::Int(1));
        assert_eq!(
            PhpMixed::Object(map.clone()).to_plugin_value(),
            PhpMixed::Array(map).to_plugin_value()
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
            serialize(&date.to_plugin_value()),
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
            chrono::DateTime::<chrono::Utc>::from_plugin_value(&date.to_plugin_value()).unwrap()
        );
    }

    #[test]
    fn a_date_outside_utc_is_rejected() {
        let mut object = PhpObject::new(DATE_TIME_IMMUTABLE_CLASS);
        object.set_public("date", PluginValue::string("2024-03-04 05:06:07.123456"));
        object.set_public("timezone_type", PluginValue::Int(3));
        object.set_public("timezone", PluginValue::string("Asia/Tokyo"));
        let err =
            chrono::DateTime::<chrono::Utc>::from_plugin_value(&PluginValue::PhpObject(object))
                .unwrap_err();
        assert!(
            err.message.contains_str("Asia/Tokyo"),
            "{}",
            err.message.display_lossy()
        );
    }
}
