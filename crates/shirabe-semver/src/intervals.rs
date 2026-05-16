//! ref: composer/vendor/composer/semver/src/Intervals.php

use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};

use crate::constraint::constraint::Constraint;
use crate::constraint::constraint_interface::ConstraintInterface;
use crate::constraint::match_all_constraint::MatchAllConstraint;
use crate::constraint::match_none_constraint::MatchNoneConstraint;
use crate::constraint::multi_constraint::MultiConstraint;
use crate::interval::{DevConstraintSet, Interval};
use shirabe_php_shim as php;

#[derive(Debug, Clone)]
pub struct IntervalCollection {
    pub numeric: Vec<Interval>,
    pub branches: DevConstraintSet,
}

static INTERVALS_CACHE: OnceLock<Mutex<HashMap<String, IntervalCollection>>> = OnceLock::new();

fn intervals_cache() -> &'static Mutex<HashMap<String, IntervalCollection>> {
    INTERVALS_CACHE.get_or_init(|| Mutex::new(HashMap::new()))
}

fn op_sort_order(op: &str) -> i64 {
    match op {
        ">=" => -3,
        "<" => -2,
        ">" => 2,
        "<=" => 3,
        _ => 0,
    }
}

#[derive(Debug)]
pub struct Intervals;

impl Intervals {
    pub fn clear() {
        *intervals_cache().lock().unwrap() = HashMap::new();
    }

    pub fn is_subset_of(
        candidate: &dyn ConstraintInterface,
        constraint: &dyn ConstraintInterface,
    ) -> anyhow::Result<bool> {
        if constraint
            .as_any()
            .downcast_ref::<MatchAllConstraint>()
            .is_some()
        {
            return Ok(true);
        }

        if candidate
            .as_any()
            .downcast_ref::<MatchNoneConstraint>()
            .is_some()
            || constraint
                .as_any()
                .downcast_ref::<MatchNoneConstraint>()
                .is_some()
        {
            return Ok(false);
        }

        // Phase B: ConstraintInterface needs clone_box() to create owned copies from references.
        let multi =
            MultiConstraint::new(vec![candidate.clone_box(), constraint.clone_box()], true)?;
        let intersection_intervals = Self::get(&multi)?;
        let candidate_intervals = Self::get(candidate)?;

        if intersection_intervals.numeric.len() != candidate_intervals.numeric.len() {
            return Ok(false);
        }

        for (index, interval) in intersection_intervals.numeric.iter().enumerate() {
            if index >= candidate_intervals.numeric.len() {
                return Ok(false);
            }

            if candidate_intervals.numeric[index].get_start().__to_string()
                != interval.get_start().__to_string()
            {
                return Ok(false);
            }

            if candidate_intervals.numeric[index].get_end().__to_string()
                != interval.get_end().__to_string()
            {
                return Ok(false);
            }
        }

        if intersection_intervals.branches.exclude != candidate_intervals.branches.exclude {
            return Ok(false);
        }
        if intersection_intervals.branches.names.len() != candidate_intervals.branches.names.len() {
            return Ok(false);
        }
        for (index, name) in intersection_intervals.branches.names.iter().enumerate() {
            if name != &candidate_intervals.branches.names[index] {
                return Ok(false);
            }
        }

        Ok(true)
    }

    pub fn have_intersections(
        a: &dyn ConstraintInterface,
        b: &dyn ConstraintInterface,
    ) -> anyhow::Result<bool> {
        if a.as_any().downcast_ref::<MatchAllConstraint>().is_some()
            || b.as_any().downcast_ref::<MatchAllConstraint>().is_some()
        {
            return Ok(true);
        }

        if a.as_any().downcast_ref::<MatchNoneConstraint>().is_some()
            || b.as_any().downcast_ref::<MatchNoneConstraint>().is_some()
        {
            return Ok(false);
        }

        // Phase B: ConstraintInterface needs clone_box().
        let multi = MultiConstraint::new(vec![a.clone_box(), b.clone_box()], true)?;
        let intersection_intervals = Self::generate_intervals(&multi, true)?;

        Ok(!intersection_intervals.numeric.is_empty()
            || intersection_intervals.branches.exclude
            || !intersection_intervals.branches.names.is_empty())
    }

    pub fn compact_constraint(
        constraint: &dyn ConstraintInterface,
    ) -> anyhow::Result<Box<dyn ConstraintInterface>> {
        if constraint
            .as_any()
            .downcast_ref::<MultiConstraint>()
            .is_none()
        {
            return Ok(constraint.clone_box());
        }

        let intervals = Self::generate_intervals(constraint, false)?;
        let mut constraints: Vec<Box<dyn ConstraintInterface>> = Vec::new();
        let mut has_numeric_match_all = false;

        if intervals.numeric.len() == 1
            && intervals.numeric[0].get_start().__to_string() == Interval::from_zero().__to_string()
            && intervals.numeric[0].get_end().__to_string()
                == Interval::until_positive_infinity().__to_string()
        {
            constraints.push(Box::new(intervals.numeric[0].get_start().clone()));
            has_numeric_match_all = true;
        } else {
            let mut un_equal_constraints: Vec<Box<dyn ConstraintInterface>> = Vec::new();
            let count = intervals.numeric.len();
            let mut i = 0;
            while i < count {
                let interval = &intervals.numeric[i];

                // if current interval ends with < N and next interval begins with > N we can swap
                // this out for != N but this needs to happen as a conjunctive expression together
                // with the start of the current interval and end of next interval, so
                // [>=M, <N] || [>N, <P] => [>=M, !=N, <P] but M/P can be skipped if they are
                // zero/+inf
                if interval.get_end().get_operator() == "<" && i + 1 < count {
                    let next_interval = &intervals.numeric[i + 1];
                    if interval.get_end().get_version() == next_interval.get_start().get_version()
                        && next_interval.get_start().get_operator() == ">"
                    {
                        // only add a start if we didn't already do so, can be skipped if we're
                        // looking at second interval in [>=M, <N] || [>N, <P] || [>P, <Q] where
                        // unEqualConstraints currently contains [>=M, !=N] already and we only
                        // want to add !=P right now
                        if un_equal_constraints.is_empty()
                            && interval.get_start().__to_string()
                                != Interval::from_zero().__to_string()
                        {
                            un_equal_constraints.push(Box::new(interval.get_start().clone()));
                        }
                        un_equal_constraints.push(Box::new(Constraint::new(
                            "!=".to_string(),
                            interval.get_end().get_version().to_string(),
                        )?));
                        i += 1;
                        continue;
                    }
                }

                if !un_equal_constraints.is_empty() {
                    // this is where the end of the following interval of a != constraint is added
                    if interval.get_end().__to_string()
                        != Interval::until_positive_infinity().__to_string()
                    {
                        un_equal_constraints.push(Box::new(interval.get_end().clone()));
                    }

                    // count is 1 if entire constraint is just one != expression
                    if un_equal_constraints.len() > 1 {
                        constraints
                            .push(Box::new(MultiConstraint::new(un_equal_constraints, true)?));
                    } else {
                        constraints.push(un_equal_constraints.into_iter().next().unwrap());
                    }

                    un_equal_constraints = Vec::new();
                    i += 1;
                    continue;
                }

                // convert back >= x - <= x intervals to == x
                if interval.get_start().get_version() == interval.get_end().get_version()
                    && interval.get_start().get_operator() == ">="
                    && interval.get_end().get_operator() == "<="
                {
                    constraints.push(Box::new(Constraint::new(
                        "==".to_string(),
                        interval.get_start().get_version().to_string(),
                    )?));
                    i += 1;
                    continue;
                }

                if interval.get_start().__to_string() == Interval::from_zero().__to_string() {
                    constraints.push(Box::new(interval.get_end().clone()));
                } else if interval.get_end().__to_string()
                    == Interval::until_positive_infinity().__to_string()
                {
                    constraints.push(Box::new(interval.get_start().clone()));
                } else {
                    constraints.push(Box::new(MultiConstraint::new(
                        vec![
                            Box::new(interval.get_start().clone()),
                            Box::new(interval.get_end().clone()),
                        ],
                        true,
                    )?));
                }

                i += 1;
            }
        }

        let mut dev_constraints: Vec<Box<dyn ConstraintInterface>> = Vec::new();

        if intervals.branches.names.is_empty() {
            if intervals.branches.exclude && has_numeric_match_all {
                return Ok(Box::new(MatchAllConstraint {
                    pretty_string: None,
                }));
                // otherwise constraint should contain a != operator and already cover this
            }
        } else {
            for branch_name in &intervals.branches.names {
                if intervals.branches.exclude {
                    dev_constraints.push(Box::new(Constraint::new(
                        "!=".to_string(),
                        branch_name.clone(),
                    )?));
                } else {
                    dev_constraints.push(Box::new(Constraint::new(
                        "==".to_string(),
                        branch_name.clone(),
                    )?));
                }
            }

            // excluded branches, e.g. != dev-foo are conjunctive with the interval, so
            // > 2.0 != dev-foo must return a conjunctive constraint
            if intervals.branches.exclude {
                if constraints.len() > 1 {
                    let merged: Vec<Box<dyn ConstraintInterface>> =
                        std::iter::once(Box::new(MultiConstraint::new(constraints, false)?)
                            as Box<dyn ConstraintInterface>)
                        .chain(dev_constraints)
                        .collect();
                    return Ok(Box::new(MultiConstraint::new(merged, true)?));
                }

                if constraints.len() == 1
                    && constraints[0].__to_string() == Interval::from_zero().__to_string()
                {
                    if dev_constraints.len() > 1 {
                        return Ok(Box::new(MultiConstraint::new(dev_constraints, true)?));
                    }
                    return Ok(dev_constraints.into_iter().next().unwrap());
                }

                let merged: Vec<Box<dyn ConstraintInterface>> =
                    constraints.into_iter().chain(dev_constraints).collect();
                return Ok(Box::new(MultiConstraint::new(merged, true)?));
            }

            // otherwise devConstraints contains a list of == operators for branches which are
            // disjunctive with the rest of the constraint
            constraints.extend(dev_constraints);
        }

        if constraints.len() > 1 {
            return Ok(Box::new(MultiConstraint::new(constraints, false)?));
        }

        if constraints.len() == 1 {
            return Ok(constraints.into_iter().next().unwrap());
        }

        Ok(Box::new(MatchNoneConstraint {
            pretty_string: None,
        }))
    }

    pub fn get(constraint: &dyn ConstraintInterface) -> anyhow::Result<IntervalCollection> {
        let key = constraint.__to_string();

        {
            let cache = intervals_cache().lock().unwrap();
            if let Some(cached) = cache.get(&key) {
                return Ok(cached.clone());
            }
        }

        let result = Self::generate_intervals(constraint, false)?;

        {
            let mut cache = intervals_cache().lock().unwrap();
            cache.insert(key, result.clone());
        }

        Ok(result)
    }

    fn generate_intervals(
        constraint: &dyn ConstraintInterface,
        stop_on_first_valid_interval: bool,
    ) -> anyhow::Result<IntervalCollection> {
        if constraint
            .as_any()
            .downcast_ref::<MatchAllConstraint>()
            .is_some()
        {
            return Ok(IntervalCollection {
                numeric: vec![Interval::new(
                    Interval::from_zero().clone(),
                    Interval::until_positive_infinity().clone(),
                )],
                branches: Interval::any_dev(),
            });
        }

        if constraint
            .as_any()
            .downcast_ref::<MatchNoneConstraint>()
            .is_some()
        {
            return Ok(IntervalCollection {
                numeric: vec![],
                branches: Interval::no_dev(),
            });
        }

        if let Some(c) = constraint.as_any().downcast_ref::<Constraint>() {
            return Self::generate_single_constraint_intervals(c);
        }

        let multi = constraint
            .as_any()
            .downcast_ref::<MultiConstraint>()
            .ok_or_else(|| {
                anyhow::anyhow!(
                    "The constraint passed in should be an MatchAllConstraint, Constraint or \
                    MultiConstraint instance, got an unknown type."
                )
            })?;

        let sub_constraints = multi.get_constraints();

        let mut numeric_groups: Vec<Vec<Interval>> = Vec::new();
        let mut constraint_branches: Vec<DevConstraintSet> = Vec::new();
        for c in sub_constraints {
            let res = Self::get(c.as_ref())?;
            numeric_groups.push(res.numeric);
            constraint_branches.push(res.branches);
        }

        let mut branches = if multi.is_disjunctive_mc() {
            let mut branches = Interval::no_dev();
            for b in constraint_branches {
                if b.exclude {
                    if branches.exclude {
                        // disjunctive constraint, so only exclude what's excluded in all constraints
                        // !=a,!=b || !=b,!=c => !=b
                        branches.names = branches
                            .names
                            .into_iter()
                            .filter(|n| b.names.contains(n))
                            .collect();
                    } else {
                        // disjunctive constraint so exclude all names which are not explicitly
                        // included in the alternative
                        // (==b || ==c) || !=a,!=b => !=a
                        branches.exclude = true;
                        branches.names = b
                            .names
                            .into_iter()
                            .filter(|n| !branches.names.contains(n))
                            .collect();
                    }
                } else {
                    if branches.exclude {
                        // disjunctive constraint so exclude all names which are not explicitly
                        // included in the alternative
                        // !=a,!=b || (==b || ==c) => !=a
                        branches.names = branches
                            .names
                            .into_iter()
                            .filter(|n| !b.names.contains(n))
                            .collect();
                    } else {
                        // disjunctive constraint, so just add all the other branches
                        // (==a || ==b) || ==c => ==a || ==b || ==c
                        branches.names.extend(b.names);
                    }
                }
            }
            branches
        } else {
            let mut branches = Interval::any_dev();
            for b in constraint_branches {
                if b.exclude {
                    if branches.exclude {
                        // conjunctive, so just add all branch names to be excluded
                        // !=a && !=b => !=a,!=b
                        branches.names.extend(b.names);
                    } else {
                        // conjunctive, so only keep included names which are not excluded
                        // (==a||==c) && !=a,!=b => ==c
                        branches.names = branches
                            .names
                            .into_iter()
                            .filter(|n| !b.names.contains(n))
                            .collect();
                    }
                } else {
                    if branches.exclude {
                        // conjunctive, so only keep included names which are not excluded
                        // !=a,!=b && (==a||==c) => ==c
                        branches.names = b
                            .names
                            .into_iter()
                            .filter(|n| !branches.names.contains(n))
                            .collect();
                        branches.exclude = false;
                    } else {
                        // conjunctive, so only keep names that are included in both
                        // (==a||==b) && (==a||==c) => ==a
                        branches.names = branches
                            .names
                            .into_iter()
                            .filter(|n| b.names.contains(n))
                            .collect();
                    }
                }
            }
            branches
        };

        branches.names = php::array_unique(&branches.names);

        if numeric_groups.len() == 1 {
            return Ok(IntervalCollection {
                numeric: numeric_groups.into_iter().next().unwrap(),
                branches,
            });
        }

        // Each border is (version, operator, is_start).
        let mut borders: Vec<(String, String, bool)> = Vec::new();
        for group in &numeric_groups {
            for interval in group {
                borders.push((
                    interval.get_start().get_version().to_string(),
                    interval.get_start().get_operator().to_string(),
                    true,
                ));
                borders.push((
                    interval.get_end().get_version().to_string(),
                    interval.get_end().get_operator().to_string(),
                    false,
                ));
            }
        }

        borders.sort_by(|a, b| {
            let order = php::version_compare_2(&a.0, &b.0);
            if order == 0 {
                let diff = op_sort_order(&a.1) - op_sort_order(&b.1);
                diff.cmp(&0)
            } else {
                order.cmp(&0)
            }
        });

        let mut active_intervals: i64 = 0;
        let mut intervals: Vec<Interval> = Vec::new();
        let activation_threshold = if multi.is_conjunctive() {
            numeric_groups.len() as i64
        } else {
            1
        };
        let mut start: Option<Constraint> = None;

        for (version, operator, is_start) in &borders {
            if *is_start {
                active_intervals += 1;
            } else {
                active_intervals -= 1;
            }

            if start.is_none() && active_intervals >= activation_threshold {
                start = Some(Constraint::new(operator.clone(), version.clone())?);
            } else if start.is_some() && active_intervals < activation_threshold {
                let start_c = start.take().unwrap();
                // filter out invalid intervals like > x - <= x, or >= x - < x
                if php::version_compare(start_c.get_version(), version, "=")
                    && ((start_c.get_operator() == ">" && operator == "<=")
                        || (start_c.get_operator() == ">=" && operator == "<"))
                {
                    // skip invalid interval (equivalent to PHP's unset($intervals[$index]))
                } else {
                    intervals.push(Interval::new(
                        start_c,
                        Constraint::new(operator.clone(), version.clone())?,
                    ));

                    if stop_on_first_valid_interval {
                        break;
                    }
                }
            }
        }

        Ok(IntervalCollection {
            numeric: intervals,
            branches,
        })
    }

    fn generate_single_constraint_intervals(
        constraint: &Constraint,
    ) -> anyhow::Result<IntervalCollection> {
        let op = constraint.get_operator();

        // handle branch constraints first
        if constraint.get_version().starts_with("dev-") {
            let mut intervals: Vec<Interval> = Vec::new();
            let mut branches = Interval::no_dev();

            // != dev-foo means any numeric version may match, we treat >/< like != they are not
            // really defined for branches
            if op == "!=" {
                intervals.push(Interval::new(
                    Interval::from_zero().clone(),
                    Interval::until_positive_infinity().clone(),
                ));
                branches = DevConstraintSet {
                    names: vec![constraint.get_version().to_string()],
                    exclude: true,
                };
            } else if op == "==" {
                branches.names.push(constraint.get_version().to_string());
            }

            return Ok(IntervalCollection {
                numeric: intervals,
                branches,
            });
        }

        if op.starts_with('>') {
            // > & >=
            return Ok(IntervalCollection {
                numeric: vec![Interval::new(
                    constraint.clone(),
                    Interval::until_positive_infinity().clone(),
                )],
                branches: Interval::no_dev(),
            });
        }
        if op.starts_with('<') {
            // < & <=
            return Ok(IntervalCollection {
                numeric: vec![Interval::new(
                    Interval::from_zero().clone(),
                    constraint.clone(),
                )],
                branches: Interval::no_dev(),
            });
        }
        if op == "!=" {
            // convert !=x to intervals of 0 - <x && >x - +inf + dev*
            return Ok(IntervalCollection {
                numeric: vec![
                    Interval::new(
                        Interval::from_zero().clone(),
                        Constraint::new("<".to_string(), constraint.get_version().to_string())?,
                    ),
                    Interval::new(
                        Constraint::new(">".to_string(), constraint.get_version().to_string())?,
                        Interval::until_positive_infinity().clone(),
                    ),
                ],
                branches: Interval::any_dev(),
            });
        }

        // convert ==x to an interval of >=x - <=x
        Ok(IntervalCollection {
            numeric: vec![Interval::new(
                Constraint::new(">=".to_string(), constraint.get_version().to_string())?,
                Constraint::new("<=".to_string(), constraint.get_version().to_string())?,
            )],
            branches: Interval::no_dev(),
        })
    }
}
