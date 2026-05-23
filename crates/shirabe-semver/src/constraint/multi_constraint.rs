//! ref: composer/vendor/composer/semver/src/Constraint/MultiConstraint.php

use crate::constraint::AnyConstraint;
use crate::constraint::Bound;
use crate::constraint::MatchAllConstraint;

#[derive(Debug, Clone)]
pub struct MultiConstraint {
    pub(crate) constraints: Vec<AnyConstraint>,
    pub(crate) pretty_string: Option<String>,
    pub(crate) conjunctive: bool,
}

impl MultiConstraint {
    pub fn new(
        constraints: Vec<AnyConstraint>,
        conjunctive: bool,
        pretty_string: Option<String>,
    ) -> Self {
        assert!(
            constraints.len() >= 2,
            "Must provide at least two constraints for a MultiConstraint. Use \
                the regular Constraint class for one constraint only or MatchAllConstraint for none. You may use \
                MultiConstraint::create() which optimizes and handles those cases automatically."
        );

        Self {
            constraints,
            pretty_string,
            conjunctive,
        }
    }

    pub fn get_constraints(&self) -> &[AnyConstraint] {
        &self.constraints
    }

    pub fn is_conjunctive(&self) -> bool {
        self.conjunctive
    }

    pub fn is_disjunctive_mc(&self) -> bool {
        !self.conjunctive
    }

    /// Composer memoizes the result; this port recomputes on every call. It is not heavy
    /// calculation so caching is a premature optimization.
    fn extract_bounds(&self) -> (Bound, Bound) {
        let mut current_lower: Option<Bound> = None;
        let mut current_upper: Option<Bound> = None;

        for constraint in &self.constraints {
            if current_lower.is_none() || current_upper.is_none() {
                current_lower = Some(constraint.get_lower_bound());
                current_upper = Some(constraint.get_upper_bound());
                continue;
            }

            let constraint_lower = constraint.get_lower_bound();
            let is_conj = self.is_conjunctive();
            if constraint_lower
                .compare_to(
                    current_lower.as_ref().unwrap(),
                    if is_conj { ">" } else { "<" },
                )
                .expect("valid operator")
            {
                current_lower = Some(constraint_lower);
            }

            let constraint_upper = constraint.get_upper_bound();
            if constraint_upper
                .compare_to(
                    current_upper.as_ref().unwrap(),
                    if is_conj { "<" } else { ">" },
                )
                .expect("valid operator")
            {
                current_upper = Some(constraint_upper);
            }
        }

        (
            current_lower.expect("MultiConstraint always has at least two constraints"),
            current_upper.expect("MultiConstraint always has at least two constraints"),
        )
    }

    pub fn create(
        constraints: Vec<AnyConstraint>,
        conjunctive: bool,
        pretty_string: Option<String>,
    ) -> anyhow::Result<AnyConstraint> {
        if constraints.is_empty() {
            return Ok(MatchAllConstraint::new(pretty_string).into());
        }

        if constraints.len() == 1 {
            let mut single = constraints.into_iter().next().unwrap();
            if pretty_string.is_some() {
                single.set_pretty_string(pretty_string);
            }
            return Ok(single);
        }

        let (constraints, conjunctive) = Self::optimize_constraints(constraints, conjunctive);

        if constraints.len() == 1 {
            let mut single = constraints.into_iter().next().unwrap();
            if pretty_string.is_some() {
                single.set_pretty_string(pretty_string);
            }
            return Ok(single);
        }

        Ok(MultiConstraint::new(constraints, conjunctive, pretty_string).into())
    }

    // Returns the (possibly optimized) constraints and the effective conjunctive flag.
    // Always returns the constraints vector (consuming it), whether or not optimization was applied.
    // The PHP version returns null for no optimization; here we return the original values unchanged.
    fn optimize_constraints(
        constraints: Vec<AnyConstraint>,
        conjunctive: bool,
    ) -> (Vec<AnyConstraint>, bool) {
        // Parse the two OR groups and if they are contiguous collapse into one constraint.
        // [>= 1 < 2] || [>= 2 < 3] || [>= 3 < 4] => [>= 1 < 4]
        if !conjunctive {
            let mut iter = constraints.into_iter();
            let mut left: AnyConstraint = iter.next().unwrap();
            let mut merged_constraints: Vec<AnyConstraint> = Vec::new();
            let mut optimized = false;

            for right in iter {
                let merged: Option<AnyConstraint> = {
                    let maybe_l_mc = left.as_multi_constraint();
                    let maybe_r_mc = right.as_multi_constraint();

                    if let (Some(l_mc), Some(r_mc)) = (maybe_l_mc, maybe_r_mc) {
                        if l_mc.conjunctive
                            && r_mc.conjunctive
                            && l_mc.constraints.len() == 2
                            && r_mc.constraints.len() == 2
                        {
                            let left0 = l_mc.constraints[0].to_string();
                            let left1 = l_mc.constraints[1].to_string();
                            let right0 = r_mc.constraints[0].to_string();
                            let right1 = r_mc.constraints[1].to_string();

                            if left0.starts_with(">=")
                                && left1.starts_with('<')
                                && right0.starts_with(">=")
                                && right1.starts_with('<')
                                && left1.get(2..) == right0.get(3..)
                            {
                                Some(
                                    MultiConstraint::new(
                                        vec![
                                            l_mc.constraints[0].clone(),
                                            r_mc.constraints[1].clone(),
                                        ],
                                        true,
                                        None,
                                    )
                                    .into(),
                                )
                            } else {
                                None
                            }
                        } else {
                            None
                        }
                    } else {
                        None
                    }
                };

                if let Some(new_left) = merged {
                    optimized = true;
                    left = new_left;
                } else {
                    merged_constraints.push(left);
                    left = right;
                }
            }

            merged_constraints.push(left);

            if optimized {
                return (merged_constraints, false);
            }

            return (merged_constraints, conjunctive);
        }

        // TODO: Here's the place to put more optimizations

        (constraints, conjunctive)
    }

    pub fn compile(&self, other_operator: i64) -> String {
        let mut parts = Vec::new();
        for constraint in &self.constraints {
            let code = constraint.compile(other_operator);
            if code == "true" {
                if !self.conjunctive {
                    return "true".to_string();
                }
            } else if code == "false" {
                if self.conjunctive {
                    return "false".to_string();
                }
            } else {
                parts.push(format!("({})", code));
            }
        }

        if parts.is_empty() {
            return if self.conjunctive {
                "true".to_string()
            } else {
                "false".to_string()
            };
        }

        if self.conjunctive {
            parts.join("&&")
        } else {
            parts.join("||")
        }
    }

    pub fn get_pretty_string(&self) -> String {
        if let Some(ref s) = self.pretty_string
            && !s.is_empty()
        {
            return s.clone();
        }
        self.to_string()
    }

    pub fn get_lower_bound(&self) -> Bound {
        self.extract_bounds().0
    }

    pub fn get_upper_bound(&self) -> Bound {
        self.extract_bounds().1
    }
}

impl std::fmt::Display for MultiConstraint {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let parts: Vec<String> = self.constraints.iter().map(|c| c.to_string()).collect();
        let sep = if self.conjunctive { " " } else { " || " };
        write!(f, "[{}]", parts.join(sep))
    }
}
