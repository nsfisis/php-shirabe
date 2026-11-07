//! Argument decoding for the entity methods, and the wire forms of the value types they exchange.

use crate::package::{DisplayMode, Link, Mirror};
use crate::plugin::php_plugin_value::{link_from_wire, link_to_wire};
use bstr::{BStr, BString};
use indexmap::IndexMap;
use shirabe_php_rpc::{FromWire, PhpThrow, PluginValue, ToWire, wire};
use shirabe_php_shim::bformat;

/// Decodes the argument at `position`. A missing argument decodes the same way PHP's `null`
/// does; a parameter's own default belongs to `arg_or`.
pub(crate) fn arg<T: FromWire>(
    method: &BStr,
    args: &[PluginValue],
    position: usize,
) -> Result<T, PhpThrow> {
    T::from_wire(args.get(position).unwrap_or(&wire::NULL))
        .map_err(|throw| arg_throw(method, position, throw))
}

/// Decodes the argument at `position` for a parameter whose PHP declaration carries a default:
/// an omitted argument, and the `null` PHP passes in its place, take `default`.
pub(crate) fn arg_or<T: FromWire>(
    method: &BStr,
    args: &[PluginValue],
    position: usize,
    default: T,
) -> Result<T, PhpThrow> {
    match args.get(position) {
        None | Some(PluginValue::Null) => Ok(default),
        Some(value) => T::from_wire(value).map_err(|throw| arg_throw(method, position, throw)),
    }
}

/// Names the argument a decoding failure is about.
pub(crate) fn arg_throw(method: &BStr, position: usize, throw: PhpThrow) -> PhpThrow {
    PhpThrow {
        message: bformat!("{method} argument {position}: {}", throw.message),
        ..throw
    }
}

impl FromWire for Link {
    fn from_wire(value: &PluginValue) -> Result<Self, PhpThrow> {
        link_from_wire(value)
    }
}

/// The child rebuilds a link as a real `Composer\Package\Link`, constraint included.
///
/// TODO(plugin): links have no entity to intern against, so two calls of the same getter answer
/// with distinct child-side objects where upstream returns the identical one.
impl ToWire for Link {
    fn to_wire(self) -> PluginValue {
        link_to_wire(&self)
    }
}

/// `array{url: string, preferred: bool}` as PHP shapes it.
impl FromWire for Mirror {
    fn from_wire(value: &PluginValue) -> Result<Self, PhpThrow> {
        let row = IndexMap::<BString, PluginValue>::from_wire(value)?;
        let field = |key: &[u8]| row.get(key).unwrap_or(&wire::NULL);
        Ok(Mirror {
            url: BString::from_wire(field(b"url"))?,
            preferred: Option::<bool>::from_wire(field(b"preferred"))?.unwrap_or(false),
        })
    }
}

impl ToWire for Mirror {
    fn to_wire(self) -> PluginValue {
        PluginValue::Array(IndexMap::from([
            ("url".into(), PluginValue::string(self.url)),
            ("preferred".into(), PluginValue::Bool(self.preferred)),
        ]))
    }
}

impl FromWire for DisplayMode {
    fn from_wire(value: &PluginValue) -> Result<Self, PhpThrow> {
        match value {
            PluginValue::Int(0) => Ok(DisplayMode::SourceRefIfDev),
            PluginValue::Int(1) => Ok(DisplayMode::SourceRef),
            PluginValue::Int(2) => Ok(DisplayMode::DistRef),
            other => Err(wire::unexpected("a display mode of 0..=2", other)),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_data_argument_keeps_its_bytes() {
        let bytes = BString::from(b"vendor/\xffname".as_slice());
        let args = [PluginValue::String(bytes.clone())];

        assert_eq!(
            bytes,
            arg::<BString>(BStr::new("method"), &args, 0).unwrap()
        );
    }
}
