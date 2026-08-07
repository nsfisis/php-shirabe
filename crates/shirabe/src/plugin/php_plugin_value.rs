//! Codec for the immutable values that cross the plugin boundary as real PHP objects.
//!
//! This module has no Composer counterpart: it is part of the plugin runtime split (see
//! `docs/dev/php-rpc.md`). A proxied entity crosses as a handle, but a value object has no
//! entity to point at — the child has to hold a genuine instance of the real class. Each value
//! therefore crosses as the object record PHP's `serialize()` writes for it: the class name and
//! the property table, which `unserialize()` revives on the other side without running a
//! constructor. `Shirabe\MaterializedValue` in the worker is the PHP half of the same protocol,
//! and names the classes allowed to cross.
//!
//! Writing the fields directly is what makes the two sides equivalent: a constructor normalizes
//! its arguments and leaves the rest of the state to setters, so a value whose state a
//! constructor cannot express (an unset pretty string, a `Link` built without a pretty
//! constraint) has no faithful constructor call.

use crate::package::Link;
use chrono::{DateTime, NaiveDateTime, TimeZone, Utc};
use shirabe_php_rpc::{PhpObject, PhpThrow, PluginValue};
use shirabe_semver::constraint::{
    AnyConstraint, MatchAllConstraint, MatchNoneConstraint, MultiConstraint, SimpleConstraint,
};

const LINK_CLASS: &str = "Composer\\Package\\Link";
const CONSTRAINT_CLASS: &str = "Composer\\Semver\\Constraint\\Constraint";
const MULTI_CONSTRAINT_CLASS: &str = "Composer\\Semver\\Constraint\\MultiConstraint";
const MATCH_ALL_CLASS: &str = "Composer\\Semver\\Constraint\\MatchAllConstraint";
const MATCH_NONE_CLASS: &str = "Composer\\Semver\\Constraint\\MatchNoneConstraint";
const DATE_TIME_IMMUTABLE_CLASS: &str = "DateTimeImmutable";
const DATE_TIME_CLASS: &str = "DateTime";

/// The wall clock of a PHP date, at the microsecond precision the other side can represent.
const DATE_FORMAT: &str = "%Y-%m-%d %H:%M:%S%.6f";

/// PHP's `timezone_type` for a date carrying a named zone rather than an offset or an
/// abbreviation.
const TIMEZONE_TYPE_IDENTIFIER: i64 = 3;

/// The only zone a date may cross in: this port holds instants as `DateTime<Utc>` and carries no
/// timezone database, so the worker rebases every date on UTC before it is serialized.
const TIMEZONE_UTC: &str = "UTC";

fn throw(message: String) -> PhpThrow {
    PhpThrow {
        exception_class: "RuntimeException".to_string(),
        message,
        code: 0,
    }
}

fn as_object(value: &PluginValue) -> Option<&PhpObject> {
    match value {
        PluginValue::PhpObject(object) => Some(object),
        _ => None,
    }
}

fn optional_string(value: Option<&PluginValue>) -> Option<String> {
    match value {
        Some(PluginValue::String(bytes)) => Some(String::from_utf8_lossy(bytes).into_owned()),
        _ => None,
    }
}

fn required_string(context: &str, value: Option<&PluginValue>) -> Result<String, PhpThrow> {
    match value {
        Some(PluginValue::String(bytes)) => Ok(String::from_utf8_lossy(bytes).into_owned()),
        other => Err(throw(format!("{context} expects a string, got {other:?}"))),
    }
}

fn constraint_to_wire(constraint: &AnyConstraint) -> PluginValue {
    // Whether the pretty string was ever set is observable (`getPrettyString()` falls back to the
    // string form), so an unset one crosses as null rather than as an absent property.
    let pretty_string = match constraint.pretty_string() {
        Some(pretty) => PluginValue::string(pretty),
        None => PluginValue::Null,
    };
    let object = match constraint {
        AnyConstraint::Simple(c) => {
            let mut object = PhpObject::new(CONSTRAINT_CLASS);
            object.set_protected(
                "operator",
                PluginValue::Int(SimpleConstraint::get_operator_constant(c.get_operator())),
            );
            object.set_protected("version", PluginValue::string(c.get_version()));
            object.set_protected("prettyString", pretty_string);
            // A cold memo rather than missing state: PHP derives the bounds from the operator
            // and the version the first time anything asks for them, and this port keeps no
            // such cache to carry over.
            object.set_protected("lowerBound", PluginValue::Null);
            object.set_protected("upperBound", PluginValue::Null);
            object
        }
        AnyConstraint::Multi(c) => {
            let mut object = PhpObject::new(MULTI_CONSTRAINT_CLASS);
            object.set_protected(
                "constraints",
                PluginValue::List(c.get_constraints().iter().map(constraint_to_wire).collect()),
            );
            object.set_protected("prettyString", pretty_string);
            // A cold memo like the bounds below, of the rendering PHP derives from the
            // constraints and the conjunctive flag.
            object.set_protected("string", PluginValue::Null);
            object.set_protected("conjunctive", PluginValue::Bool(c.is_conjunctive()));
            object.set_protected("lowerBound", PluginValue::Null);
            object.set_protected("upperBound", PluginValue::Null);
            object
        }
        AnyConstraint::MatchAll(_) => {
            let mut object = PhpObject::new(MATCH_ALL_CLASS);
            object.set_protected("prettyString", pretty_string);
            object
        }
        AnyConstraint::MatchNone(_) => {
            let mut object = PhpObject::new(MATCH_NONE_CLASS);
            object.set_protected("prettyString", pretty_string);
            object
        }
    };
    PluginValue::PhpObject(object)
}

fn constraint_from_wire(value: &PluginValue) -> Result<AnyConstraint, PhpThrow> {
    let object = as_object(value).ok_or_else(|| {
        throw(format!(
            "expected a semver constraint from the plugin, got {value:?}"
        ))
    })?;
    let pretty_string = optional_string(object.protected("prettyString"));
    Ok(match object.class.as_str() {
        CONSTRAINT_CLASS => {
            let operator = match object.protected("operator") {
                Some(PluginValue::Int(code)) => SimpleConstraint::from_operator_constant(*code)
                    .ok_or_else(|| {
                        throw(format!(
                            "a semver constraint has an unknown operator: {code}"
                        ))
                    })?,
                other => {
                    return Err(throw(format!(
                        "a semver constraint operator is not an int, got {other:?}"
                    )));
                }
            };
            AnyConstraint::Simple(SimpleConstraint::new(
                operator.to_string(),
                required_string("a semver constraint version", object.protected("version"))?,
                pretty_string,
            ))
        }
        MULTI_CONSTRAINT_CLASS => {
            let constraints = match object.protected("constraints") {
                Some(PluginValue::List(items)) => items
                    .iter()
                    .map(constraint_from_wire)
                    .collect::<Result<Vec<_>, _>>()?,
                // A MultiConstraint built from a PHP associative array crosses as a map.
                Some(PluginValue::Array(items)) => items
                    .values()
                    .map(constraint_from_wire)
                    .collect::<Result<Vec<_>, _>>()?,
                other => {
                    return Err(throw(format!(
                        "a MultiConstraint expects a list of constraints, got {other:?}"
                    )));
                }
            };
            if constraints.len() < 2 {
                return Err(throw(
                    "a MultiConstraint needs at least two constraints".to_string(),
                ));
            }
            let conjunctive = match object.protected("conjunctive") {
                Some(PluginValue::Bool(conjunctive)) => *conjunctive,
                other => {
                    return Err(throw(format!(
                        "a MultiConstraint expects a bool conjunctive flag, got {other:?}"
                    )));
                }
            };
            AnyConstraint::Multi(MultiConstraint::new(
                constraints,
                conjunctive,
                pretty_string,
            ))
        }
        MATCH_ALL_CLASS => AnyConstraint::MatchAll(MatchAllConstraint::new(pretty_string)),
        MATCH_NONE_CLASS => AnyConstraint::MatchNone(MatchNoneConstraint::new(pretty_string)),
        // TODO(plugin): a plugin-defined ConstraintInterface implementation has no Rust
        // counterpart; the four composer/semver classes are the whole vocabulary here.
        other => {
            return Err(throw(format!(
                "the semver constraint class `{other}` cannot cross the plugin boundary"
            )));
        }
    })
}

pub(crate) fn link_to_wire(link: &Link) -> PluginValue {
    let mut object = PhpObject::new(LINK_CLASS);
    object.set_protected("source", PluginValue::string(link.get_source()));
    object.set_protected("target", PluginValue::string(link.get_target()));
    object.set_protected("constraint", constraint_to_wire(link.get_constraint()));
    object.set_protected("description", PluginValue::string(link.get_description()));
    object.set_protected(
        "prettyConstraint",
        PluginValue::string(link.get_pretty_constraint()),
    );
    PluginValue::PhpObject(object)
}

pub(crate) fn link_from_wire(value: &PluginValue) -> Result<Link, PhpThrow> {
    let object = as_object(value)
        .ok_or_else(|| throw(format!("expected a Link from the plugin, got {value:?}")))?;
    if object.class != LINK_CLASS {
        // TODO(plugin): Link subclasses have no Rust counterpart; the port models links as a
        // single value type.
        return Err(throw(format!(
            "the class `{}` cannot cross the plugin boundary as a Link",
            object.class
        )));
    }
    let constraint = constraint_from_wire(
        object
            .protected("constraint")
            .ok_or_else(|| throw("a Link expects a constraint".to_string()))?,
    )?;
    // TODO(plugin): PHP's Link leaves $prettyConstraint nullable and throws from
    // getPrettyConstraint() when it was never given, but this port stores a string, so a link
    // a plugin built without one comes back carrying the empty string instead.
    let pretty_constraint =
        optional_string(object.protected("prettyConstraint")).unwrap_or_default();
    Ok(Link::new(
        required_string("a Link source", object.protected("source"))?,
        required_string("a Link target", object.protected("target"))?,
        constraint,
        optional_string(object.protected("description")),
        pretty_constraint,
    ))
}

pub(crate) fn date_time_to_wire(date: &DateTime<Utc>) -> PluginValue {
    let mut object = PhpObject::new(DATE_TIME_IMMUTABLE_CLASS);
    object.set_public(
        "date",
        PluginValue::string(date.format(DATE_FORMAT).to_string()),
    );
    object.set_public("timezone_type", PluginValue::Int(TIMEZONE_TYPE_IDENTIFIER));
    object.set_public("timezone", PluginValue::string(TIMEZONE_UTC));
    PluginValue::PhpObject(object)
}

pub(crate) fn date_time_from_wire(value: &PluginValue) -> Result<DateTime<Utc>, PhpThrow> {
    let object = as_object(value)
        .ok_or_else(|| throw(format!("expected a date from the plugin, got {value:?}")))?;
    if object.class != DATE_TIME_IMMUTABLE_CLASS && object.class != DATE_TIME_CLASS {
        return Err(throw(format!(
            "the class `{}` cannot cross the plugin boundary as a date",
            object.class
        )));
    }
    let zone = optional_string(object.public("timezone"));
    if object.public("timezone_type") != Some(&PluginValue::Int(TIMEZONE_TYPE_IDENTIFIER))
        || zone.as_deref() != Some(TIMEZONE_UTC)
    {
        return Err(throw(format!(
            "a date crosses the plugin boundary rebased on {TIMEZONE_UTC}, got the zone {zone:?}"
        )));
    }
    let rendered = required_string("a date", object.public("date"))?;
    let parsed = NaiveDateTime::parse_from_str(&rendered, DATE_FORMAT).map_err(|e| {
        throw(format!(
            "the date `{rendered}` from the plugin is not in the expected format: {e}"
        ))
    })?;
    Ok(Utc.from_utc_datetime(&parsed))
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Timelike;
    use shirabe_php_rpc::value::serialize;

    fn simple(operator: &str, version: &str, pretty_string: Option<&str>) -> AnyConstraint {
        AnyConstraint::Simple(SimpleConstraint::new(
            operator.to_string(),
            version.to_string(),
            pretty_string.map(str::to_string),
        ))
    }

    /// The expected bytes are what PHP writes for
    /// `new Link('a/b', 'c/d', new Constraint('>=', '1.0.0'), 'requires', '^1.0')`: the property
    /// set, its order and the visibility mangling all have to match, or `unserialize()` would
    /// revive an instance carrying defaults where the value has state.
    #[test]
    fn a_link_is_written_as_php_writes_it() {
        let link = Link::new(
            "a/b".to_string(),
            "c/d".to_string(),
            simple(">=", "1.0.0", None),
            Some("requires".to_string()),
            "^1.0".to_string(),
        );
        assert_eq!(
            String::from_utf8_lossy(&serialize(&link_to_wire(&link))),
            String::from_utf8_lossy(
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
    /// `new MultiConstraint([new Constraint('>=', '1.0.0'), new Constraint('<', '2.0.0')], true)`
    /// with a pretty string set on it.
    #[test]
    fn a_multi_constraint_is_written_as_php_writes_it() {
        let constraint = AnyConstraint::Multi(MultiConstraint::new(
            vec![simple(">=", "1.0.0", None), simple("<", "2.0.0", None)],
            true,
            Some("^1.0".to_string()),
        ));
        assert_eq!(
            String::from_utf8_lossy(&serialize(&constraint_to_wire(&constraint))),
            String::from_utf8_lossy(
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

    /// The expected bytes are what PHP writes for
    /// `new DateTimeImmutable('2026-08-07 12:34:56.123456', new DateTimeZone('UTC'))`.
    #[test]
    fn a_date_is_written_as_php_writes_it() {
        let date = Utc
            .with_ymd_and_hms(2026, 8, 7, 12, 34, 56)
            .unwrap()
            .with_nanosecond(123_456_000)
            .unwrap();
        assert_eq!(
            serialize(&date_time_to_wire(&date)),
            b"O:17:\"DateTimeImmutable\":3:{s:4:\"date\";s:26:\"2026-08-07 12:34:56.123456\";s:13:\"timezone_type\";i:3;s:8:\"timezone\";s:3:\"UTC\";}".as_slice(),
        );
    }

    #[test]
    fn every_constraint_shape_round_trips() {
        let constraints = [
            simple(">=", "1.0.0.0", None),
            simple("!=", "2.0.0.0", Some("!=2.0")),
            AnyConstraint::Multi(MultiConstraint::new(
                vec![
                    simple("<", "1.0.0.0", None),
                    AnyConstraint::Multi(MultiConstraint::new(
                        vec![simple(">=", "3.0.0.0", None), simple("<", "4.0.0.0", None)],
                        true,
                        None,
                    )),
                ],
                false,
                Some("<1.0 || ^3.0".to_string()),
            )),
            AnyConstraint::MatchAll(MatchAllConstraint::new(None)),
            AnyConstraint::MatchNone(MatchNoneConstraint::new(Some("nothing".to_string()))),
        ];
        for constraint in constraints {
            let link = Link::new(
                "Vendor/Root".to_string(),
                "Vendor/Dep".to_string(),
                constraint,
                Some(Link::TYPE_DEV_REQUIRE.to_string()),
                "^1.0".to_string(),
            );
            let decoded = link_from_wire(&link_to_wire(&link)).expect("decode failed");
            assert_eq!(link.to_string(), decoded.to_string());
            assert_eq!(link.get_description(), decoded.get_description());
            assert_eq!(
                link.get_constraint().get_pretty_string(),
                decoded.get_constraint().get_pretty_string()
            );
        }
    }

    #[test]
    fn a_date_round_trips_with_microseconds() {
        let date = Utc
            .with_ymd_and_hms(2024, 3, 3, 20, 6, 7)
            .unwrap()
            .with_nanosecond(123_456_000)
            .unwrap();
        assert_eq!(
            date,
            date_time_from_wire(&date_time_to_wire(&date)).unwrap()
        );
    }

    #[test]
    fn a_date_outside_utc_is_rejected() {
        let mut object = PhpObject::new(DATE_TIME_IMMUTABLE_CLASS);
        object.set_public("date", PluginValue::string("2024-03-04 05:06:07.123456"));
        object.set_public("timezone_type", PluginValue::Int(3));
        object.set_public("timezone", PluginValue::string("Asia/Tokyo"));
        let err = date_time_from_wire(&PluginValue::PhpObject(object)).unwrap_err();
        assert!(err.message.contains("Asia/Tokyo"), "{}", err.message);
    }

    #[test]
    fn a_foreign_class_cannot_cross_as_a_value() {
        let mut object = PhpObject::new("MyPlugin\\OddConstraint");
        object.set_protected("prettyString", PluginValue::Null);
        let err = constraint_from_wire(&PluginValue::PhpObject(object)).unwrap_err();
        assert!(err.message.contains("OddConstraint"), "{}", err.message);

        let err = link_from_wire(&PluginValue::List(Vec::new())).unwrap_err();
        assert!(err.message.contains("expected a Link"), "{}", err.message);
    }
}
