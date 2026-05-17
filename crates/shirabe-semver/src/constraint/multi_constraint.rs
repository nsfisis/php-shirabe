//! ref: composer/vendor/composer/semver/src/Constraint/MultiConstraint.php

use std::cell::RefCell;

use anyhow::bail;

use crate::constraint::bound::Bound;
use crate::constraint::constraint_interface::ConstraintInterface;
use crate::constraint::match_all_constraint::MatchAllConstraint;

pub struct MultiConstraint {
    pub(crate) constraints: Vec<Box<dyn ConstraintInterface>>,
    pub(crate) pretty_string: Option<String>,
    string: RefCell<Option<String>>,
    pub(crate) conjunctive: bool,
    lower_bound: RefCell<Option<Bound>>,
    upper_bound: RefCell<Option<Bound>>,
}

impl std::fmt::Debug for MultiConstraint {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("MultiConstraint")
            .field("conjunctive", &self.conjunctive)
            .finish()
    }
}

impl MultiConstraint {
    pub fn new(
        constraints: Vec<Box<dyn ConstraintInterface>>,
        conjunctive: bool,
    ) -> anyhow::Result<Self> {
        if constraints.len() < 2 {
            bail!(
                "Must provide at least two constraints for a MultiConstraint. Use \
                the regular Constraint class for one constraint only or MatchAllConstraint for none. You may use \
                MultiConstraint::create() which optimizes and handles those cases automatically."
            );
        }

        Ok(Self {
            constraints,
            pretty_string: None,
            string: RefCell::new(None),
            conjunctive,
            lower_bound: RefCell::new(None),
            upper_bound: RefCell::new(None),
        })
    }

    pub fn get_constraints(&self) -> &[Box<dyn ConstraintInterface>] {
        &self.constraints
    }

    pub fn is_conjunctive(&self) -> bool {
        self.conjunctive
    }

    pub fn is_disjunctive_mc(&self) -> bool {
        !self.conjunctive
    }

    fn extract_bounds(&self) {
        if self.lower_bound.borrow().is_some() {
            return;
        }

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

        *self.lower_bound.borrow_mut() = current_lower;
        *self.upper_bound.borrow_mut() = current_upper;
    }

    pub fn create(
        constraints: Vec<Box<dyn ConstraintInterface>>,
        conjunctive: bool,
    ) -> anyhow::Result<Box<dyn ConstraintInterface>> {
        if constraints.is_empty() {
            return Ok(Box::new(MatchAllConstraint {
                pretty_string: None,
            }));
        }

        if constraints.len() == 1 {
            return Ok(constraints.into_iter().next().unwrap());
        }

        let (constraints, conjunctive) = Self::optimize_constraints(constraints, conjunctive);

        if constraints.len() == 1 {
            return Ok(constraints.into_iter().next().unwrap());
        }

        Ok(Box::new(MultiConstraint::new(constraints, conjunctive)?))
    }

    // Returns the (possibly optimized) constraints and the effective conjunctive flag.
    // Always returns the constraints vector (consuming it), whether or not optimization was applied.
    // The PHP version returns null for no optimization; here we return the original values unchanged.
    fn optimize_constraints(
        constraints: Vec<Box<dyn ConstraintInterface>>,
        conjunctive: bool,
    ) -> (Vec<Box<dyn ConstraintInterface>>, bool) {
        // Parse the two OR groups and if they are contiguous collapse into one constraint.
        // [>= 1 < 2] || [>= 2 < 3] || [>= 3 < 4] => [>= 1 < 4]
        if !conjunctive {
            let mut iter = constraints.into_iter();
            let mut left: Box<dyn ConstraintInterface> = iter.next().unwrap();
            let mut merged_constraints: Vec<Box<dyn ConstraintInterface>> = Vec::new();
            let mut optimized = false;

            for right in iter {
                let merged: Option<Box<dyn ConstraintInterface>> = {
                    let maybe_l_mc = left.as_any().downcast_ref::<MultiConstraint>();
                    let maybe_r_mc = right.as_any().downcast_ref::<MultiConstraint>();

                    if let (Some(l_mc), Some(r_mc)) = (maybe_l_mc, maybe_r_mc) {
                        if l_mc.conjunctive
                            && r_mc.conjunctive
                            && l_mc.constraints.len() == 2
                            && r_mc.constraints.len() == 2
                        {
                            let left0 = l_mc.constraints[0].__to_string();
                            let left1 = l_mc.constraints[1].__to_string();
                            let right0 = r_mc.constraints[0].__to_string();
                            let right1 = r_mc.constraints[1].__to_string();

                            if left0.starts_with(">=")
                                && left1.starts_with('<')
                                && right0.starts_with(">=")
                                && right1.starts_with('<')
                                && left1.get(2..) == right0.get(3..)
                            {
                                Some(Box::new(
                                    MultiConstraint::new(
                                        vec![
                                            l_mc.constraints[0].clone_box(),
                                            r_mc.constraints[1].clone_box(),
                                        ],
                                        true,
                                    )
                                    .unwrap(),
                                )
                                    as Box<dyn ConstraintInterface>)
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
}

impl ConstraintInterface for MultiConstraint {
    fn compile(&self, other_operator: i64) -> String {
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

    fn matches(&self, provider: &dyn ConstraintInterface) -> bool {
        if !self.conjunctive {
            for constraint in &self.constraints {
                if provider.matches(constraint.as_ref()) {
                    return true;
                }
            }
            return false;
        }

        if provider.is_disjunctive() {
            return provider.matches(self);
        }

        for constraint in &self.constraints {
            if !provider.matches(constraint.as_ref()) {
                return false;
            }
        }

        true
    }

    fn set_pretty_string(&mut self, pretty_string: Option<String>) {
        self.pretty_string = pretty_string;
    }

    fn get_pretty_string(&self) -> String {
        if let Some(ref s) = self.pretty_string
            && !s.is_empty()
        {
            return s.clone();
        }
        self.__to_string()
    }

    fn __to_string(&self) -> String {
        if let Some(ref s) = *self.string.borrow() {
            return s.clone();
        }

        let parts: Vec<String> = self.constraints.iter().map(|c| c.__to_string()).collect();
        let sep = if self.conjunctive { " " } else { " || " };
        let result = format!("[{}]", parts.join(sep));

        *self.string.borrow_mut() = Some(result.clone());
        result
    }

    fn get_lower_bound(&self) -> Bound {
        self.extract_bounds();
        self.lower_bound
            .borrow()
            .clone()
            .expect("extractBounds should have populated the lowerBound property")
    }

    fn get_upper_bound(&self) -> Bound {
        self.extract_bounds();
        self.upper_bound
            .borrow()
            .clone()
            .expect("extractBounds should have populated the upperBound property")
    }

    fn is_disjunctive(&self) -> bool {
        !self.conjunctive
    }

    fn clone_box(&self) -> Box<dyn ConstraintInterface> {
        Box::new(MultiConstraint {
            constraints: self.constraints.iter().map(|c| c.clone_box()).collect(),
            pretty_string: self.pretty_string.clone(),
            string: RefCell::new(self.string.borrow().clone()),
            conjunctive: self.conjunctive,
            lower_bound: RefCell::new(self.lower_bound.borrow().clone()),
            upper_bound: RefCell::new(self.upper_bound.borrow().clone()),
        })
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
}
