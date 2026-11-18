//! The RPC values standing for the domain values that cross the worker boundary by value.
//!
//! Most of them are immutable values that cross as real PHP objects. A proxied entity crosses as a handle, but a value object has no
//! entity to point at — the child has to hold a genuine instance of the real class. Each value
//! therefore crosses as the object record PHP's `serialize()` writes for it: the class name and
//! the property table, which `unserialize()` revives on the other side without running a
//! constructor. `Shirabe\ByValueObject` in the worker is the PHP half of the same protocol,
//! and names the classes allowed to cross.
//!
//! Writing the fields directly is what makes the two sides equivalent: a constructor normalizes
//! its arguments and leaves the rest of the state to setters, so a value whose state a
//! constructor cannot express (an unset pretty string, a `Link` built without a pretty
//! constraint) has no faithful constructor call.
//!
//! Each class that crosses by value has an `encode_*` and a `decode_*` function here, named after
//! its module path (`encode_package_link` for `Composer\Package\Link`); the generated
//! `php_worker::by_value` names them for every class whose treatment is by-value in the classifier
//! report.

use crate::package::{DisplayMode, Link, Mirror};
use crate::util::StreamContextOptions;
use crate::util::http::Response;
use bstr::{BStr, BString, ByteSlice as _};
use indexmap::IndexMap;
use shirabe_class_map_generator::class_map::ClassMap;
use shirabe_php_rpc::{FromRpcValue, PhpObject, PhpThrow, RpcValue, ToRpcValue, codec};
use shirabe_php_shim::{PhpMixed, bformat, display_to_bytes, error_message, php_str};
use shirabe_semver::constraint::{
    AnyConstraint, MatchAllConstraint, MatchNoneConstraint, MultiConstraint, SimpleConstraint,
};

const LINK_CLASS: &BStr = php_str(b"Composer\\Package\\Link");
const RESPONSE_CLASS: &BStr = php_str(b"Composer\\Util\\Http\\Response");
const CLASS_MAP_CLASS: &BStr = php_str(b"Composer\\ClassMapGenerator\\ClassMap");
const CONSTRAINT_CLASS: &BStr = php_str(b"Composer\\Semver\\Constraint\\Constraint");
const MULTI_CONSTRAINT_CLASS: &BStr = php_str(b"Composer\\Semver\\Constraint\\MultiConstraint");
const MATCH_ALL_CLASS: &BStr = php_str(b"Composer\\Semver\\Constraint\\MatchAllConstraint");
const MATCH_NONE_CLASS: &BStr = php_str(b"Composer\\Semver\\Constraint\\MatchNoneConstraint");

fn as_object(value: &RpcValue) -> Option<&PhpObject> {
    match value {
        RpcValue::PhpObject(object) => Some(object),
        _ => None,
    }
}

/// Decodes the property `value`, if any, `context` names.
fn property<T: FromRpcValue>(
    context: impl AsRef<[u8]>,
    value: Option<&RpcValue>,
) -> Result<T, PhpThrow> {
    T::from_rpc_value(value.unwrap_or(&codec::NULL)).map_err(|throw| PhpThrow {
        message: bformat!("{}: {}", context.as_ref(), throw.message),
        ..throw
    })
}

// Whether the pretty string was ever set is observable (`getPrettyString()` falls back to the
// string form), so an unset one crosses as null rather than as an absent property.
fn pretty_string(pretty: Option<BString>) -> RpcValue {
    match pretty {
        Some(pretty) => RpcValue::string(pretty),
        None => RpcValue::Null,
    }
}

fn constraint_to_rpc_value(constraint: AnyConstraint) -> RpcValue {
    match constraint {
        AnyConstraint::Simple(c) => encode_semver_constraint_constraint(c),
        AnyConstraint::Multi(c) => encode_semver_constraint_multi_constraint(c),
        AnyConstraint::MatchAll(c) => encode_semver_constraint_match_all_constraint(c),
        AnyConstraint::MatchNone(c) => encode_semver_constraint_match_none_constraint(c),
    }
}

/// `Composer\Semver\Constraint\Constraint`.
pub(crate) fn encode_semver_constraint_constraint(constraint: SimpleConstraint) -> RpcValue {
    let (operator, version, pretty) = constraint.into_parts();
    let mut object = PhpObject::new(CONSTRAINT_CLASS);
    object.set_protected(
        "operator",
        RpcValue::Int(SimpleConstraint::get_operator_constant(operator)),
    );
    object.set_protected("version", RpcValue::string(version));
    object.set_protected("prettyString", pretty_string(pretty));
    // A cold memo rather than missing state: PHP derives the bounds from the operator and the
    // version the first time anything asks for them, and this port keeps no such cache to carry
    // over.
    object.set_protected("lowerBound", RpcValue::Null);
    object.set_protected("upperBound", RpcValue::Null);
    RpcValue::PhpObject(object)
}

/// `Composer\Semver\Constraint\MultiConstraint`.
pub(crate) fn encode_semver_constraint_multi_constraint(constraint: MultiConstraint) -> RpcValue {
    let (constraints, conjunctive, pretty) = constraint.into_parts();
    let mut object = PhpObject::new(MULTI_CONSTRAINT_CLASS);
    object.set_protected(
        "constraints",
        RpcValue::List(
            constraints
                .into_iter()
                .map(constraint_to_rpc_value)
                .collect(),
        ),
    );
    object.set_protected("prettyString", pretty_string(pretty));
    // A cold memo like the bounds below, of the rendering PHP derives from the constraints and
    // the conjunctive flag.
    object.set_protected("string", RpcValue::Null);
    object.set_protected("conjunctive", RpcValue::Bool(conjunctive));
    object.set_protected("lowerBound", RpcValue::Null);
    object.set_protected("upperBound", RpcValue::Null);
    RpcValue::PhpObject(object)
}

/// `Composer\Semver\Constraint\MatchAllConstraint`.
pub(crate) fn encode_semver_constraint_match_all_constraint(
    constraint: MatchAllConstraint,
) -> RpcValue {
    let mut object = PhpObject::new(MATCH_ALL_CLASS);
    object.set_protected("prettyString", pretty_string(constraint.into_parts()));
    RpcValue::PhpObject(object)
}

/// `Composer\Semver\Constraint\MatchNoneConstraint`.
pub(crate) fn encode_semver_constraint_match_none_constraint(
    constraint: MatchNoneConstraint,
) -> RpcValue {
    let mut object = PhpObject::new(MATCH_NONE_CLASS);
    object.set_protected("prettyString", pretty_string(constraint.into_parts()));
    RpcValue::PhpObject(object)
}

fn constraint_pretty_string(object: &PhpObject) -> Result<Option<BString>, PhpThrow> {
    property(
        "a semver constraint pretty string",
        object.protected("prettyString"),
    )
}

/// `Composer\Semver\Constraint\Constraint`.
pub(crate) fn decode_semver_constraint_constraint(
    object: &PhpObject,
) -> Result<SimpleConstraint, PhpThrow> {
    let code: i64 = property("a semver constraint operator", object.protected("operator"))?;
    let operator = SimpleConstraint::from_operator_constant(code).ok_or_else(|| {
        PhpThrow::runtime(bformat!(
            "a semver constraint has an unknown operator: {code}"
        ))
    })?;
    Ok(SimpleConstraint::from_parts(
        operator,
        property("a semver constraint version", object.protected("version"))?,
        constraint_pretty_string(object)?,
    ))
}

/// `Composer\Semver\Constraint\MultiConstraint`.
pub(crate) fn decode_semver_constraint_multi_constraint(
    object: &PhpObject,
) -> Result<MultiConstraint, PhpThrow> {
    let pretty_string = constraint_pretty_string(object)?;
    let constraints = match object.protected("constraints") {
        Some(RpcValue::List(items)) => items
            .iter()
            .map(constraint_from_rpc_value)
            .collect::<Result<Vec<_>, _>>()?,
        // A MultiConstraint built from a PHP associative array crosses as a map.
        Some(RpcValue::Array(items)) => items
            .values()
            .map(constraint_from_rpc_value)
            .collect::<Result<Vec<_>, _>>()?,
        other => {
            return Err(PhpThrow::runtime(bformat!(
                "a MultiConstraint expects a list of constraints, got {}",
                display_to_bytes(format_args!("{other:?}"))
            )));
        }
    };
    if constraints.len() < 2 {
        return Err(PhpThrow::runtime(
            "a MultiConstraint needs at least two constraints",
        ));
    }
    let conjunctive = property(
        "a MultiConstraint conjunctive flag",
        object.protected("conjunctive"),
    )?;
    Ok(MultiConstraint::new(
        constraints,
        conjunctive,
        pretty_string,
    ))
}

/// `Composer\Semver\Constraint\MatchAllConstraint`.
pub(crate) fn decode_semver_constraint_match_all_constraint(
    object: &PhpObject,
) -> Result<MatchAllConstraint, PhpThrow> {
    Ok(MatchAllConstraint::new(constraint_pretty_string(object)?))
}

/// `Composer\Semver\Constraint\MatchNoneConstraint`.
pub(crate) fn decode_semver_constraint_match_none_constraint(
    object: &PhpObject,
) -> Result<MatchNoneConstraint, PhpThrow> {
    Ok(MatchNoneConstraint::new(constraint_pretty_string(object)?))
}

pub(crate) fn constraint_from_rpc_value(value: &RpcValue) -> Result<AnyConstraint, PhpThrow> {
    let object = as_object(value).ok_or_else(|| {
        PhpThrow::runtime(bformat!(
            "expected a semver constraint from the plugin, got {}",
            display_to_bytes(format_args!("{value:?}"))
        ))
    })?;
    // The pretty string every constraint class carries is checked before the class itself.
    constraint_pretty_string(object)?;
    let class = object.class.as_bstr();
    Ok(match class {
        class if class == CONSTRAINT_CLASS => {
            AnyConstraint::Simple(decode_semver_constraint_constraint(object)?)
        }
        class if class == MULTI_CONSTRAINT_CLASS => {
            AnyConstraint::Multi(decode_semver_constraint_multi_constraint(object)?)
        }
        class if class == MATCH_ALL_CLASS => {
            AnyConstraint::MatchAll(decode_semver_constraint_match_all_constraint(object)?)
        }
        class if class == MATCH_NONE_CLASS => {
            AnyConstraint::MatchNone(decode_semver_constraint_match_none_constraint(object)?)
        }
        // TODO(plugin): a plugin-defined ConstraintInterface implementation has no Rust
        // counterpart; the four composer/semver classes are the whole vocabulary here.
        other => {
            return Err(PhpThrow::runtime(bformat!(
                "the semver constraint class `{other}` cannot cross the plugin boundary"
            )));
        }
    })
}

/// A `Response` is built for one request and never retained by the object graph, so the child
/// holds a real instance rather than a handle, and `collect()` frees the copy it holds.
///
/// TODO(type-model): PHP's `$request` is the whole request array (`url`, `options`, `copyTo`)
/// and this port keeps only the url, which is all `decodeJson()` reads back out of it.
///
/// TODO(port): Composer answers a curl request with a `Composer\Util\Http\CurlResponse`
/// carrying the transfer info; the native downloader that takes those requests here flattens its
/// response into `Response` before the value leaves `HttpDownloader`, so `getCurlInfo()` is gone
/// and `get_class()` differs.
impl ToRpcValue for Response {
    fn to_rpc_value(self) -> RpcValue {
        encode_util_http_response(self)
    }
}

/// `Composer\Util\Http\Response`.
pub(crate) fn encode_util_http_response(response: Response) -> RpcValue {
    let (url, code, headers, body) = response.into_parts();
    let mut object = PhpObject::new(RESPONSE_CLASS);
    let mut request = indexmap::IndexMap::new();
    request.insert("url".into(), RpcValue::string(url));
    object.set_private(RESPONSE_CLASS, "request", RpcValue::Array(request));
    object.set_private(RESPONSE_CLASS, "code", RpcValue::Int(code));
    object.set_private(
        RESPONSE_CLASS,
        "headers",
        RpcValue::List(headers.into_iter().map(RpcValue::string).collect()),
    );
    object.set_private(
        RESPONSE_CLASS,
        "body",
        match body {
            Some(body) => RpcValue::string(body),
            None => RpcValue::Null,
        },
    );
    RpcValue::PhpObject(object)
}

/// `Composer\Util\Http\Response`.
pub(crate) fn decode_util_http_response(object: &PhpObject) -> Result<Response, PhpThrow> {
    let request: IndexMap<BString, RpcValue> = property(
        "a Response request",
        object.private(RESPONSE_CLASS, "request"),
    )?;
    Ok(Response::new(
        property(
            "a Response request url",
            Some(request.get(b"url".as_slice()).unwrap_or(&codec::NULL)),
        )?,
        property("a Response code", object.private(RESPONSE_CLASS, "code"))?,
        property(
            "a Response headers",
            object.private(RESPONSE_CLASS, "headers"),
        )?,
        property::<Option<BString>>("a Response body", object.private(RESPONSE_CLASS, "body"))?
            .map(Vec::from),
    ))
}

/// `Composer\ClassMapGenerator\ClassMap`, which `AutoloadGenerator::dump()` returns. The child
/// owns the revived instance outright: the Rust side keeps no entity behind it.
pub(crate) fn encode_class_map_generator_class_map(class_map: &ClassMap) -> RpcValue {
    let mut object = PhpObject::new(CLASS_MAP_CLASS);
    object.set_public("map", class_map.get_map().clone().to_rpc_value());
    let ambiguous_classes = class_map
        .get_ambiguous_classes(None)
        .expect("no duplicates filter is applied");
    object.set_private(
        CLASS_MAP_CLASS,
        "ambiguousClasses",
        ambiguous_classes.to_rpc_value(),
    );
    let psr_violations: indexmap::IndexMap<bstr::BString, RpcValue> = class_map
        .get_raw_psr_violations()
        .iter()
        .map(|(path, violations)| {
            let violations = violations
                .iter()
                .map(|violation| {
                    RpcValue::Array(indexmap::IndexMap::from([
                        (
                            "warning".into(),
                            RpcValue::string(violation.warning.as_bstr()),
                        ),
                        (
                            "className".into(),
                            RpcValue::string(violation.class_name.clone()),
                        ),
                    ]))
                })
                .collect();
            (path.clone(), RpcValue::List(violations))
        })
        .collect();
    object.set_private(
        CLASS_MAP_CLASS,
        "psrViolations",
        psr_violations.to_rpc_value(),
    );
    RpcValue::PhpObject(object)
}

/// `Composer\ClassMapGenerator\ClassMap`.
pub(crate) fn decode_class_map_generator_class_map(
    object: &PhpObject,
) -> Result<ClassMap, PhpThrow> {
    let mut class_map = ClassMap::new();
    let map: IndexMap<BString, BString> = property("a ClassMap map", object.public("map"))?;
    for (class_name, path) in map {
        class_map.add_class(class_name, path);
    }
    let ambiguous_classes: IndexMap<BString, Vec<BString>> = property(
        "a ClassMap ambiguous classes",
        object.private(CLASS_MAP_CLASS, "ambiguousClasses"),
    )?;
    for (class_name, paths) in ambiguous_classes {
        for path in paths {
            class_map.add_ambiguous_class(class_name.clone(), path);
        }
    }
    let psr_violations: IndexMap<BString, Vec<IndexMap<BString, BString>>> = property(
        "a ClassMap psr violations",
        object.private(CLASS_MAP_CLASS, "psrViolations"),
    )?;
    for (path, violations) in psr_violations {
        for violation in violations {
            let field = |key: &[u8]| {
                violation.get(key).cloned().ok_or_else(|| {
                    PhpThrow::runtime(bformat!(
                        "a ClassMap psr violation lacks `{}`",
                        BStr::new(key)
                    ))
                })
            };
            class_map.add_psr_violation(field(b"warning")?, field(b"className")?, &path);
        }
    }
    Ok(class_map)
}

impl FromRpcValue for Link {
    fn from_rpc_value(value: &RpcValue) -> Result<Self, PhpThrow> {
        let object = as_object(value).ok_or_else(|| {
            PhpThrow::runtime(bformat!(
                "expected a Link from the plugin, got {}",
                display_to_bytes(format_args!("{value:?}"))
            ))
        })?;
        if object.class != LINK_CLASS {
            // TODO(plugin): Link subclasses have no Rust counterpart; the port models links as a
            // single value type.
            return Err(PhpThrow::runtime(bformat!(
                "the class `{}` cannot cross the plugin boundary as a Link",
                object.class
            )));
        }
        decode_package_link(object)
    }
}

/// `Composer\Package\Link`.
pub(crate) fn decode_package_link(object: &PhpObject) -> Result<Link, PhpThrow> {
    let constraint = constraint_from_rpc_value(
        object
            .protected("constraint")
            .ok_or_else(|| PhpThrow::runtime("a Link expects a constraint"))?,
    )?;
    Ok(Link::from_parts(
        property("a Link source", object.protected("source"))?,
        property("a Link target", object.protected("target"))?,
        constraint,
        property("a Link description", object.protected("description"))?,
        property(
            "a Link pretty constraint",
            object.protected("prettyConstraint"),
        )?,
    ))
}

/// The child rebuilds a link as a real `Composer\Package\Link`, constraint included.
///
/// TODO(plugin): links have no entity to intern against, so two calls of the same getter answer
/// with distinct child-side objects where upstream returns the identical one.
impl ToRpcValue for Link {
    fn to_rpc_value(self) -> RpcValue {
        encode_package_link(self)
    }
}

/// `Composer\Package\Link`.
pub(crate) fn encode_package_link(link: Link) -> RpcValue {
    let (source, target, constraint, description, pretty_constraint) = link.into_parts();
    let mut object = PhpObject::new(LINK_CLASS);
    object.set_protected("source", RpcValue::string(source));
    object.set_protected("target", RpcValue::string(target));
    object.set_protected("constraint", constraint_to_rpc_value(constraint));
    object.set_protected("description", RpcValue::string(description));
    let pretty_constraint = match pretty_constraint {
        Some(pretty) => RpcValue::string(pretty),
        None => RpcValue::Null,
    };
    object.set_protected("prettyConstraint", pretty_constraint);
    RpcValue::PhpObject(object)
}

/// `DateTimeImmutable`, the class a date the Rust side hands over crosses as.
pub(crate) fn encode_date_time_immutable(instant: chrono::DateTime<chrono::Utc>) -> RpcValue {
    instant.to_rpc_value()
}

/// `DateTimeImmutable`.
pub(crate) fn decode_date_time_immutable(
    object: &PhpObject,
) -> Result<chrono::DateTime<chrono::Utc>, PhpThrow> {
    chrono::DateTime::<chrono::Utc>::from_rpc_value(&RpcValue::PhpObject(object.clone()))
}

/// `DateTime`: the record of `DateTimeImmutable` under the mutable class, which has the same
/// properties.
pub(crate) fn encode_date_time(instant: chrono::DateTime<chrono::Utc>) -> RpcValue {
    let RpcValue::PhpObject(mut object) = instant.to_rpc_value() else {
        unreachable!("a date is written as an object record");
    };
    object.class = BString::from("DateTime");
    RpcValue::PhpObject(object)
}

/// `DateTime`.
pub(crate) fn decode_date_time(
    object: &PhpObject,
) -> Result<chrono::DateTime<chrono::Utc>, PhpThrow> {
    chrono::DateTime::<chrono::Utc>::from_rpc_value(&RpcValue::PhpObject(object.clone()))
}

/// `array{url: string, preferred: bool}` as PHP shapes it.
impl FromRpcValue for Mirror {
    fn from_rpc_value(value: &RpcValue) -> Result<Self, PhpThrow> {
        let row = IndexMap::<BString, RpcValue>::from_rpc_value(value)?;
        let field = |key: &[u8]| row.get(key).unwrap_or(&codec::NULL);
        Ok(Mirror {
            url: BString::from_rpc_value(field(b"url"))?,
            preferred: Option::<bool>::from_rpc_value(field(b"preferred"))?.unwrap_or(false),
        })
    }
}

impl ToRpcValue for Mirror {
    fn to_rpc_value(self) -> RpcValue {
        RpcValue::Array(IndexMap::from([
            ("url".into(), RpcValue::string(self.url)),
            ("preferred".into(), RpcValue::Bool(self.preferred)),
        ]))
    }
}

impl FromRpcValue for DisplayMode {
    fn from_rpc_value(value: &RpcValue) -> Result<Self, PhpThrow> {
        match value {
            RpcValue::Int(0) => Ok(DisplayMode::SourceRefIfDev),
            RpcValue::Int(1) => Ok(DisplayMode::SourceRef),
            RpcValue::Int(2) => Ok(DisplayMode::DistRef),
            other => Err(codec::unexpected("a display mode of 0..=2", other)),
        }
    }
}

impl ToRpcValue for StreamContextOptions {
    fn to_rpc_value(self) -> RpcValue {
        PhpMixed::Array(self.into_array()).to_rpc_value()
    }
}

impl FromRpcValue for StreamContextOptions {
    fn from_rpc_value(value: &RpcValue) -> Result<Self, PhpThrow> {
        Self::from_array(IndexMap::<BString, PhpMixed>::from_rpc_value(value)?)
            .map_err(|error| PhpThrow::new("TypeError", error_message(&error), 0))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use shirabe_php_rpc::value::serialize;
    use shirabe_php_shim::ToPhpString;

    fn simple(operator: &BStr, version: &BStr, pretty_string: Option<&BStr>) -> AnyConstraint {
        AnyConstraint::Simple(SimpleConstraint::new(
            operator,
            version.to_owned(),
            pretty_string.map(BString::from),
        ))
    }

    /// The expected bytes are what PHP writes for
    /// `new Link('a/b', 'c/d', new Constraint('>=', '1.0.0'), 'requires', '^1.0')`: the property
    /// set, its order and the visibility mangling all have to match, or `unserialize()` would
    /// revive an instance carrying defaults where the value has state.
    #[test]
    fn a_link_is_written_as_php_writes_it() {
        let link = Link::new(
            BString::from("a/b"),
            BString::from("c/d"),
            simple(BStr::new(">="), BStr::new("1.0.0"), None),
            Some(BString::from("requires")),
            Some(BString::from("^1.0")),
        );
        assert_eq!(
            BStr::new(&serialize(&link.clone().to_rpc_value())),
            BStr::new(
                b"O:21:\"Composer\\Package\\Link\":5:{\
                    s:9:\"\0*\0source\";s:3:\"a/b\";\
                    s:9:\"\0*\0target\";s:3:\"c/d\";\
                    s:13:\"\0*\0constraint\";O:37:\"Composer\\Semver\\Constraint\\Constraint\":5:{\
                        s:11:\"\0*\0operator\";i:4;\
                        s:10:\"\0*\0version\";s:5:\"1.0.0\";\
                        s:15:\"\0*\0prettyString\";N;\
                        s:13:\"\0*\0lowerBound\";N;\
                        s:13:\"\0*\0upperBound\";N;}\
                    s:14:\"\0*\0description\";s:8:\"requires\";\
                    s:19:\"\0*\0prettyConstraint\";s:4:\"^1.0\";}"
            ),
        );
    }

    /// The expected bytes are what PHP writes for
    /// `new Link('a/b', 'c/d', new MatchAllConstraint())`: a link built without a pretty
    /// constraint crosses with a null one, so `getPrettyConstraint()` throws in the plugin too.
    #[test]
    fn a_link_without_a_pretty_constraint_is_written_with_null() {
        let link = Link::new(
            BString::from("a/b"),
            BString::from("c/d"),
            AnyConstraint::MatchAll(MatchAllConstraint::new(None)),
            None,
            None,
        );
        assert_eq!(
            BStr::new(&serialize(&link.clone().to_rpc_value())),
            BStr::new(
                b"O:21:\"Composer\\Package\\Link\":5:{\
                    s:9:\"\0*\0source\";s:3:\"a/b\";\
                    s:9:\"\0*\0target\";s:3:\"c/d\";\
                    s:13:\"\0*\0constraint\";O:45:\"Composer\\Semver\\Constraint\\MatchAllConstraint\":1:{\
                        s:15:\"\0*\0prettyString\";N;}\
                    s:14:\"\0*\0description\";s:10:\"relates to\";\
                    s:19:\"\0*\0prettyConstraint\";N;}"
            ),
        );
        let decoded = Link::from_rpc_value(&link.to_rpc_value()).expect("decode failed");
        assert!(decoded.get_pretty_constraint().is_err());
        assert_eq!(None, decoded.into_parts().4);
    }

    /// The expected bytes are what PHP writes for
    /// `new MultiConstraint([new Constraint('>=', '1.0.0'), new Constraint('<', '2.0.0')], true)`
    /// with a pretty string set on it.
    #[test]
    fn a_multi_constraint_is_written_as_php_writes_it() {
        let constraint = AnyConstraint::Multi(MultiConstraint::new(
            vec![
                simple(BStr::new(">="), BStr::new("1.0.0"), None),
                simple(BStr::new("<"), BStr::new("2.0.0"), None),
            ],
            true,
            Some(BString::from("^1.0")),
        ));
        assert_eq!(
            BStr::new(&serialize(&constraint_to_rpc_value(constraint))),
            BStr::new(
                b"O:42:\"Composer\\Semver\\Constraint\\MultiConstraint\":6:{\
                    s:14:\"\0*\0constraints\";a:2:{\
                        i:0;O:37:\"Composer\\Semver\\Constraint\\Constraint\":5:{\
                            s:11:\"\0*\0operator\";i:4;\
                            s:10:\"\0*\0version\";s:5:\"1.0.0\";\
                            s:15:\"\0*\0prettyString\";N;\
                            s:13:\"\0*\0lowerBound\";N;\
                            s:13:\"\0*\0upperBound\";N;}\
                        i:1;O:37:\"Composer\\Semver\\Constraint\\Constraint\":5:{\
                            s:11:\"\0*\0operator\";i:1;\
                            s:10:\"\0*\0version\";s:5:\"2.0.0\";\
                            s:15:\"\0*\0prettyString\";N;\
                            s:13:\"\0*\0lowerBound\";N;\
                            s:13:\"\0*\0upperBound\";N;}}\
                    s:15:\"\0*\0prettyString\";s:4:\"^1.0\";\
                    s:9:\"\0*\0string\";N;\
                    s:14:\"\0*\0conjunctive\";b:1;\
                    s:13:\"\0*\0lowerBound\";N;\
                    s:13:\"\0*\0upperBound\";N;}"
            ),
        );
    }

    #[test]
    fn every_constraint_shape_round_trips() {
        let constraints = [
            simple(BStr::new(">="), BStr::new("1.0.0.0"), None),
            simple(
                BStr::new("!="),
                BStr::new("2.0.0.0"),
                Some(BStr::new("!=2.0")),
            ),
            AnyConstraint::Multi(MultiConstraint::new(
                vec![
                    simple(BStr::new("<"), BStr::new("1.0.0.0"), None),
                    AnyConstraint::Multi(MultiConstraint::new(
                        vec![
                            simple(BStr::new(">="), BStr::new("3.0.0.0"), None),
                            simple(BStr::new("<"), BStr::new("4.0.0.0"), None),
                        ],
                        true,
                        None,
                    )),
                ],
                false,
                Some(BString::from("<1.0 || ^3.0")),
            )),
            AnyConstraint::MatchAll(MatchAllConstraint::new(None)),
            AnyConstraint::MatchNone(MatchNoneConstraint::new(Some(BString::from("nothing")))),
        ];
        for constraint in constraints {
            let link = Link::new(
                BString::from("Vendor/Root"),
                BString::from("Vendor/Dep"),
                constraint,
                Some(Link::TYPE_DEV_REQUIRE.to_owned()),
                Some(BString::from("^1.0")),
            );
            let decoded =
                Link::from_rpc_value(&link.clone().to_rpc_value()).expect("decode failed");
            assert_eq!(link.to_php_string(), decoded.to_php_string());
            assert_eq!(link.get_description(), decoded.get_description());
            assert_eq!(
                link.get_constraint().get_pretty_string(),
                decoded.get_constraint().get_pretty_string()
            );
        }
    }

    #[test]
    fn a_response_round_trips() {
        let response = Response::new(
            BString::from("https://example.org/packages.json"),
            Some(200),
            vec![BString::from("HTTP/1.1 200 OK")],
            Some(b"{}".to_vec()),
        );
        let RpcValue::PhpObject(object) = encode_util_http_response(response) else {
            panic!("a Response is written as an object record");
        };
        let decoded = decode_util_http_response(&object).expect("decode failed");
        assert_eq!(
            decoded.into_parts(),
            (
                BString::from("https://example.org/packages.json"),
                200,
                vec![BString::from("HTTP/1.1 200 OK")],
                Some(b"{}".to_vec()),
            )
        );
    }

    #[test]
    fn a_class_map_round_trips() {
        let mut class_map = ClassMap::new();
        class_map.add_class("Vendor\\Foo", "/src/Foo.php");
        class_map.add_ambiguous_class("Vendor\\Foo", "/lib/Foo.php");
        class_map.add_psr_violation(BString::from("bad"), "Vendor\\Bar", "/src/Bar.php");
        let RpcValue::PhpObject(object) = encode_class_map_generator_class_map(&class_map) else {
            panic!("a ClassMap is written as an object record");
        };
        let decoded = decode_class_map_generator_class_map(&object).expect("decode failed");
        assert_eq!(decoded.get_map(), class_map.get_map());
        assert_eq!(
            decoded.get_ambiguous_classes(None).unwrap(),
            class_map.get_ambiguous_classes(None).unwrap()
        );
        assert_eq!(decoded.get_psr_violations(), class_map.get_psr_violations());
    }

    #[test]
    fn a_date_round_trips_as_either_class() {
        let instant = chrono::DateTime::from_timestamp(1_786_000_000, 123_456_000).unwrap();
        let RpcValue::PhpObject(object) = encode_date_time(instant) else {
            panic!("a date is written as an object record");
        };
        assert_eq!(object.class, "DateTime");
        assert_eq!(decode_date_time(&object).unwrap(), instant);
        let RpcValue::PhpObject(object) = encode_date_time_immutable(instant) else {
            panic!("a date is written as an object record");
        };
        assert_eq!(object.class, "DateTimeImmutable");
        assert_eq!(decode_date_time_immutable(&object).unwrap(), instant);
    }

    #[test]
    fn a_foreign_class_cannot_cross_as_a_value() {
        let mut object = PhpObject::new("MyPlugin\\OddConstraint");
        object.set_protected("prettyString", RpcValue::Null);
        let err = constraint_from_rpc_value(&RpcValue::PhpObject(object)).unwrap_err();
        assert!(
            err.message.contains_str("OddConstraint"),
            "{}",
            err.message.display_lossy()
        );

        let err = Link::from_rpc_value(&RpcValue::List(Vec::new())).unwrap_err();
        assert!(
            err.message.contains_str("expected a Link"),
            "{}",
            err.message.display_lossy()
        );
    }
}
