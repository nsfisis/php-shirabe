//! Codec for the immutable values that cross the plugin boundary as real PHP objects.
//!
//! This module has no Composer counterpart: it is part of the plugin runtime split (see
//! `docs/dev/php-rpc.md`). A proxied entity crosses as a handle, but a value object has no
//! entity to point at — the child has to hold a genuine instance of the real class. Each value
//! is therefore described structurally (class name, constructor arguments, post-construction
//! calls) and rebuilt on the other side; `Shirabe\MaterializedValue` in the worker is the PHP
//! half of the same protocol, and describes those instances back in the same shape.

use crate::package::Link;
use chrono::{DateTime, TimeZone, Utc};
use indexmap::IndexMap;
use shirabe_php_rpc::{PhpThrow, PluginValue};
use shirabe_semver::constraint::{
    AnyConstraint, MatchAllConstraint, MatchNoneConstraint, MultiConstraint, SimpleConstraint,
};

const CLASS_KEY: &[u8] = b"__pnew";
const ARGS_KEY: &[u8] = b"__args";
const CALLS_KEY: &[u8] = b"__calls";

const LINK_CLASS: &str = "Composer\\Package\\Link";
const CONSTRAINT_CLASS: &str = "Composer\\Semver\\Constraint\\Constraint";
const MULTI_CONSTRAINT_CLASS: &str = "Composer\\Semver\\Constraint\\MultiConstraint";
const MATCH_ALL_CLASS: &str = "Composer\\Semver\\Constraint\\MatchAllConstraint";
const MATCH_NONE_CLASS: &str = "Composer\\Semver\\Constraint\\MatchNoneConstraint";
const DATE_TIME_CLASS: &str = "DateTimeImmutable";

/// PHP's `DateTimeInterface::ATOM` widened by the microseconds PHP dates carry: the child
/// reconstructs the instant from this rendering, so it must keep both the offset and the full
/// precision the other side can represent.
const DATE_TIME_FORMAT: &str = "%Y-%m-%dT%H:%M:%S%.6f%:z";

fn throw(message: String) -> PhpThrow {
    PhpThrow {
        exception_class: "RuntimeException".to_string(),
        message,
        code: 0,
    }
}

fn materialized(
    class: &str,
    args: Vec<PluginValue>,
    calls: Vec<(&str, Vec<PluginValue>)>,
) -> PluginValue {
    let mut map = IndexMap::new();
    map.insert(CLASS_KEY.to_vec(), PluginValue::string(class));
    map.insert(ARGS_KEY.to_vec(), PluginValue::List(args));
    if !calls.is_empty() {
        map.insert(
            CALLS_KEY.to_vec(),
            PluginValue::List(
                calls
                    .into_iter()
                    .map(|(method, args)| {
                        PluginValue::List(vec![
                            PluginValue::string(method),
                            PluginValue::List(args),
                        ])
                    })
                    .collect(),
            ),
        );
    }
    PluginValue::Array(map)
}

/// The class name and constructor arguments of a materialized value, or `None` for any other
/// value shape.
fn as_materialized(value: &PluginValue) -> Option<(String, &[PluginValue])> {
    let map = match value {
        PluginValue::Array(map) => map,
        _ => return None,
    };
    let class = match map.get(CLASS_KEY) {
        Some(PluginValue::String(bytes)) => String::from_utf8_lossy(bytes).into_owned(),
        _ => return None,
    };
    let args = match map.get(ARGS_KEY) {
        Some(PluginValue::List(args)) => args.as_slice(),
        None => &[],
        _ => return None,
    };
    Some((class, args))
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

/// The `setPrettyString` call a constraint needs after construction. PHP stores the pretty
/// string in a protected field that no constructor takes, and leaving it unset is observable
/// (`getPrettyString()` falls back to the string form), so an unset one is sent as null.
fn pretty_string_call(constraint: &AnyConstraint) -> Vec<(&'static str, Vec<PluginValue>)> {
    let value = match constraint.pretty_string() {
        Some(pretty) => PluginValue::string(pretty),
        None => PluginValue::Null,
    };
    vec![("setPrettyString", vec![value])]
}

fn constraint_to_wire(constraint: &AnyConstraint) -> PluginValue {
    let calls = pretty_string_call(constraint);
    match constraint {
        AnyConstraint::Simple(c) => materialized(
            CONSTRAINT_CLASS,
            vec![
                PluginValue::string(c.get_operator()),
                PluginValue::string(c.get_version()),
            ],
            calls,
        ),
        AnyConstraint::Multi(c) => materialized(
            MULTI_CONSTRAINT_CLASS,
            vec![
                PluginValue::List(c.get_constraints().iter().map(constraint_to_wire).collect()),
                PluginValue::Bool(c.is_conjunctive()),
            ],
            calls,
        ),
        AnyConstraint::MatchAll(_) => materialized(MATCH_ALL_CLASS, Vec::new(), calls),
        AnyConstraint::MatchNone(_) => materialized(MATCH_NONE_CLASS, Vec::new(), calls),
    }
}

/// The pretty string carried by a materialized constraint's `setPrettyString` call.
fn wire_pretty_string(value: &PluginValue) -> Option<String> {
    let map = match value {
        PluginValue::Array(map) => map,
        _ => return None,
    };
    let calls = match map.get(CALLS_KEY) {
        Some(PluginValue::List(calls)) => calls,
        _ => return None,
    };
    calls.iter().find_map(|call| match call {
        PluginValue::List(parts) => match (parts.first(), parts.get(1)) {
            (Some(PluginValue::String(method)), Some(PluginValue::List(args)))
                if method == b"setPrettyString" =>
            {
                optional_string(args.first())
            }
            _ => None,
        },
        _ => None,
    })
}

fn constraint_from_wire(value: &PluginValue) -> Result<AnyConstraint, PhpThrow> {
    let (class, args) = as_materialized(value).ok_or_else(|| {
        throw(format!(
            "expected a semver constraint from the plugin, got {value:?}"
        ))
    })?;
    let pretty_string = wire_pretty_string(value);
    Ok(match class.as_str() {
        CONSTRAINT_CLASS => AnyConstraint::Simple(SimpleConstraint::new(
            required_string("a semver constraint operator", args.first())?,
            required_string("a semver constraint version", args.get(1))?,
            pretty_string,
        )),
        MULTI_CONSTRAINT_CLASS => {
            let constraints = match args.first() {
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
            let conjunctive = match args.get(1) {
                Some(PluginValue::Bool(conjunctive)) => *conjunctive,
                // PHP defaults the parameter to true.
                None => true,
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
    materialized(
        LINK_CLASS,
        vec![
            PluginValue::string(link.get_source()),
            PluginValue::string(link.get_target()),
            constraint_to_wire(link.get_constraint()),
            PluginValue::string(link.get_description()),
            PluginValue::string(link.get_pretty_constraint()),
        ],
        Vec::new(),
    )
}

pub(crate) fn link_from_wire(value: &PluginValue) -> Result<Link, PhpThrow> {
    let (class, args) = as_materialized(value)
        .ok_or_else(|| throw(format!("expected a Link from the plugin, got {value:?}")))?;
    if class != LINK_CLASS {
        // TODO(plugin): Link subclasses have no Rust counterpart; the port models links as a
        // single value type.
        return Err(throw(format!(
            "the class `{class}` cannot cross the plugin boundary as a Link"
        )));
    }
    let constraint = constraint_from_wire(
        args.get(2)
            .ok_or_else(|| throw("a Link expects a constraint argument".to_string()))?,
    )?;
    // TODO(plugin): PHP's Link leaves $prettyConstraint nullable and throws from
    // getPrettyConstraint() when it was never given, but this port stores a string, so a link
    // a plugin built without one comes back carrying the empty string instead.
    let pretty_constraint = optional_string(args.get(4)).unwrap_or_default();
    Ok(Link::new(
        required_string("a Link source", args.first())?,
        required_string("a Link target", args.get(1))?,
        constraint,
        optional_string(args.get(3)),
        pretty_constraint,
    ))
}

pub(crate) fn date_time_to_wire(date: &DateTime<Utc>) -> PluginValue {
    materialized(
        DATE_TIME_CLASS,
        vec![PluginValue::string(
            date.format(DATE_TIME_FORMAT).to_string(),
        )],
        Vec::new(),
    )
}

pub(crate) fn date_time_from_wire(value: &PluginValue) -> Result<DateTime<Utc>, PhpThrow> {
    let (class, args) = as_materialized(value)
        .ok_or_else(|| throw(format!("expected a date from the plugin, got {value:?}")))?;
    if class != DATE_TIME_CLASS {
        return Err(throw(format!(
            "the class `{class}` cannot cross the plugin boundary as a date"
        )));
    }
    let rendered = required_string("a date", args.first())?;
    let parsed = DateTime::parse_from_str(&rendered, DATE_TIME_FORMAT).map_err(|e| {
        throw(format!(
            "the date `{rendered}` from the plugin is not in the expected format: {e}"
        ))
    })?;
    Ok(Utc.from_utc_datetime(&parsed.naive_utc()))
}
