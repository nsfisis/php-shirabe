//! ref: composer/vendor/composer/semver/src/Constraint/ConstraintInterface.php

use crate::constraint::Bound;
use crate::constraint::MatchAllConstraint;
use crate::constraint::MatchNoneConstraint;
use crate::constraint::MultiConstraint;
use crate::constraint::SimpleConstraint;

/// Corresponds to PHP's `ConstraintInterface`.
#[derive(Clone, Debug)]
pub enum AnyConstraint {
    Simple(SimpleConstraint),
    Multi(MultiConstraint),
    MatchAll(MatchAllConstraint),
    MatchNone(MatchNoneConstraint),
}

impl AnyConstraint {
    pub fn matches(&self, provider: &AnyConstraint) -> bool {
        match self {
            Self::MatchAll(_) => true,
            Self::MatchNone(_) => false,
            Self::Simple(c) => match provider.as_constraint() {
                Some(p) => c.match_specific(p, false),
                None => provider.matches(self),
            },
            Self::Multi(m) => {
                if !m.conjunctive {
                    m.constraints.iter().any(|sub| provider.matches(sub))
                } else if provider.is_disjunctive() {
                    provider.matches(self)
                } else {
                    m.constraints.iter().all(|sub| provider.matches(sub))
                }
            }
        }
    }

    pub fn compile(&self, other_operator: i64) -> String {
        match self {
            Self::Simple(c) => c.compile(other_operator),
            Self::Multi(c) => c.compile(other_operator),
            Self::MatchAll(c) => c.compile(other_operator),
            Self::MatchNone(c) => c.compile(other_operator),
        }
    }

    pub fn get_upper_bound(&self) -> Bound {
        match self {
            Self::Simple(c) => c.get_upper_bound(),
            Self::Multi(c) => c.get_upper_bound(),
            Self::MatchAll(c) => c.get_upper_bound(),
            Self::MatchNone(c) => c.get_upper_bound(),
        }
    }

    pub fn get_lower_bound(&self) -> Bound {
        match self {
            Self::Simple(c) => c.get_lower_bound(),
            Self::Multi(c) => c.get_lower_bound(),
            Self::MatchAll(c) => c.get_lower_bound(),
            Self::MatchNone(c) => c.get_lower_bound(),
        }
    }

    pub fn get_pretty_string(&self) -> String {
        match self {
            Self::Simple(c) => c.get_pretty_string(),
            Self::Multi(c) => c.get_pretty_string(),
            Self::MatchAll(c) => c.get_pretty_string(),
            Self::MatchNone(c) => c.get_pretty_string(),
        }
    }

    /// PHP `$c instanceof MultiConstraint && $c->isDisjunctive()`.
    pub fn is_disjunctive(&self) -> bool {
        matches!(self, Self::Multi(m) if !m.conjunctive)
    }

    /// PHP `$c instanceof Constraint`.
    pub fn is_constraint(&self) -> bool {
        matches!(self, Self::Simple(_))
    }

    pub fn get_operator(&self) -> &'static str {
        match self {
            Self::Simple(c) => c.get_operator(),
            _ => "",
        }
    }

    pub fn get_version(&self) -> &str {
        match self {
            Self::Simple(c) => c.get_version(),
            _ => "",
        }
    }

    pub fn as_constraint(&self) -> Option<&SimpleConstraint> {
        match self {
            Self::Simple(c) => Some(c),
            _ => None,
        }
    }

    pub fn as_multi_constraint(&self) -> Option<&MultiConstraint> {
        match self {
            Self::Multi(c) => Some(c),
            _ => None,
        }
    }

    pub fn is_match_all(&self) -> bool {
        matches!(self, Self::MatchAll(_))
    }

    pub fn is_match_none(&self) -> bool {
        matches!(self, Self::MatchNone(_))
    }

    /// PHP exposes `ConstraintInterface::setPrettyString()` and defaults the
    /// pretty string to the constraint's string form when unset. This port takes
    /// the pretty string at construction instead; this setter exists only so
    /// `MultiConstraint::create()` can apply the pretty string PHP sets on its
    /// (possibly polymorphic) result.
    pub(crate) fn set_pretty_string(&mut self, pretty_string: Option<String>) {
        match self {
            Self::Simple(c) => c.pretty_string = pretty_string,
            Self::Multi(c) => c.pretty_string = pretty_string,
            Self::MatchAll(c) => c.pretty_string = pretty_string,
            Self::MatchNone(c) => c.pretty_string = pretty_string,
        }
    }
}

impl From<SimpleConstraint> for AnyConstraint {
    fn from(c: SimpleConstraint) -> Self {
        Self::Simple(c)
    }
}

impl From<MultiConstraint> for AnyConstraint {
    fn from(c: MultiConstraint) -> Self {
        Self::Multi(c)
    }
}

impl From<MatchAllConstraint> for AnyConstraint {
    fn from(c: MatchAllConstraint) -> Self {
        Self::MatchAll(c)
    }
}

impl From<MatchNoneConstraint> for AnyConstraint {
    fn from(c: MatchNoneConstraint) -> Self {
        Self::MatchNone(c)
    }
}

impl std::fmt::Display for AnyConstraint {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Simple(c) => write!(f, "{}", c),
            Self::Multi(c) => write!(f, "{}", c),
            Self::MatchAll(c) => write!(f, "{}", c),
            Self::MatchNone(c) => write!(f, "{}", c),
        }
    }
}
