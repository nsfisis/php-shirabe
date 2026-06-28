//! ref: composer/vendor/composer/semver/src/Intervals.php

use crate::constraint::AnyConstraint;
use crate::constraint::MatchAllConstraint;
use crate::constraint::MatchNoneConstraint;
use crate::constraint::MultiConstraint;
use crate::constraint::SimpleConstraint;
use crate::interval::{DevConstraintSet, Interval};
use indexmap::IndexMap;
use std::sync::{Mutex, OnceLock};

#[derive(Debug, Clone)]
pub struct IntervalCollection {
    pub numeric: Vec<Interval>,
    pub branches: DevConstraintSet,
}

static INTERVALS_CACHE: OnceLock<Mutex<IndexMap<String, IntervalCollection>>> = OnceLock::new();

fn intervals_cache() -> &'static Mutex<IndexMap<String, IntervalCollection>> {
    INTERVALS_CACHE.get_or_init(|| Mutex::new(IndexMap::new()))
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
        *intervals_cache().lock().unwrap() = IndexMap::new();
    }

    pub fn is_subset_of(
        candidate: &AnyConstraint,
        constraint: &AnyConstraint,
    ) -> anyhow::Result<bool> {
        if constraint.is_match_all() {
            return Ok(true);
        }

        if candidate.is_match_none() || constraint.is_match_none() {
            return Ok(false);
        }

        let multi =
            MultiConstraint::new(vec![candidate.clone(), constraint.clone()], true, None).into();
        let intersection_intervals = Self::get(&multi)?;
        let candidate_intervals = Self::get(candidate)?;

        if intersection_intervals.numeric.len() != candidate_intervals.numeric.len() {
            return Ok(false);
        }

        for (index, interval) in intersection_intervals.numeric.iter().enumerate() {
            if index >= candidate_intervals.numeric.len() {
                return Ok(false);
            }

            if candidate_intervals.numeric[index].get_start().to_string()
                != interval.get_start().to_string()
            {
                return Ok(false);
            }

            if candidate_intervals.numeric[index].get_end().to_string()
                != interval.get_end().to_string()
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

    pub fn have_intersections(a: &AnyConstraint, b: &AnyConstraint) -> anyhow::Result<bool> {
        if a.is_match_all() || b.is_match_all() {
            return Ok(true);
        }

        if a.is_match_none() || b.is_match_none() {
            return Ok(false);
        }

        let multi = MultiConstraint::new(vec![a.clone(), b.clone()], true, None).into();
        let intersection_intervals = Self::generate_intervals(&multi, true)?;

        Ok(!intersection_intervals.numeric.is_empty()
            || intersection_intervals.branches.exclude
            || !intersection_intervals.branches.names.is_empty())
    }

    pub fn compact_constraint(constraint: &AnyConstraint) -> anyhow::Result<AnyConstraint> {
        if constraint.as_multi_constraint().is_none() {
            return Ok(constraint.clone());
        }

        let intervals = Self::generate_intervals(constraint, false)?;
        let mut constraints: Vec<AnyConstraint> = Vec::new();
        let mut has_numeric_match_all = false;

        if intervals.numeric.len() == 1
            && intervals.numeric[0].get_start().to_string() == Interval::from_zero().to_string()
            && intervals.numeric[0].get_end().to_string()
                == Interval::until_positive_infinity().to_string()
        {
            constraints.push(intervals.numeric[0].get_start().clone().into());
            has_numeric_match_all = true;
        } else {
            let mut un_equal_constraints: Vec<AnyConstraint> = Vec::new();
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
                            && interval.get_start().to_string() != Interval::from_zero().to_string()
                        {
                            un_equal_constraints.push(interval.get_start().clone().into());
                        }
                        un_equal_constraints.push(
                            SimpleConstraint::new(
                                "!=".to_string(),
                                interval.get_end().get_version().to_string(),
                                None,
                            )
                            .into(),
                        );
                        i += 1;
                        continue;
                    }
                }

                if !un_equal_constraints.is_empty() {
                    // this is where the end of the following interval of a != constraint is added
                    if interval.get_end().to_string()
                        != Interval::until_positive_infinity().to_string()
                    {
                        un_equal_constraints.push(interval.get_end().clone().into());
                    }

                    // count is 1 if entire constraint is just one != expression
                    if un_equal_constraints.len() > 1 {
                        constraints
                            .push(MultiConstraint::new(un_equal_constraints, true, None).into());
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
                    constraints.push(
                        SimpleConstraint::new(
                            "==".to_string(),
                            interval.get_start().get_version().to_string(),
                            None,
                        )
                        .into(),
                    );
                    i += 1;
                    continue;
                }

                if interval.get_start().to_string() == Interval::from_zero().to_string() {
                    constraints.push(interval.get_end().clone().into());
                } else if interval.get_end().to_string()
                    == Interval::until_positive_infinity().to_string()
                {
                    constraints.push(interval.get_start().clone().into());
                } else {
                    constraints.push(
                        MultiConstraint::new(
                            vec![
                                AnyConstraint::Simple(interval.get_start().clone()),
                                AnyConstraint::Simple(interval.get_end().clone()),
                            ],
                            true,
                            None,
                        )
                        .into(),
                    );
                }

                i += 1;
            }
        }

        let mut dev_constraints: Vec<AnyConstraint> = Vec::new();

        if intervals.branches.names.is_empty() {
            if intervals.branches.exclude && has_numeric_match_all {
                return Ok(MatchAllConstraint::new(None).into());
                // otherwise constraint should contain a != operator and already cover this
            }
        } else {
            for branch_name in &intervals.branches.names {
                if intervals.branches.exclude {
                    dev_constraints.push(
                        SimpleConstraint::new("!=".to_string(), branch_name.clone(), None).into(),
                    );
                } else {
                    dev_constraints.push(
                        SimpleConstraint::new("==".to_string(), branch_name.clone(), None).into(),
                    );
                }
            }

            // excluded branches, e.g. != dev-foo are conjunctive with the interval, so
            // > 2.0 != dev-foo must return a conjunctive constraint
            if intervals.branches.exclude {
                if constraints.len() > 1 {
                    let merged: Vec<AnyConstraint> =
                        std::iter::once(MultiConstraint::new(constraints, false, None).into())
                            .chain(dev_constraints)
                            .collect();
                    return Ok(MultiConstraint::new(merged, true, None).into());
                }

                if constraints.len() == 1
                    && constraints[0].to_string() == Interval::from_zero().to_string()
                {
                    if dev_constraints.len() > 1 {
                        return Ok(MultiConstraint::new(dev_constraints, true, None).into());
                    }
                    return Ok(dev_constraints.into_iter().next().unwrap());
                }

                let merged: Vec<AnyConstraint> =
                    constraints.into_iter().chain(dev_constraints).collect();
                return Ok(MultiConstraint::new(merged, true, None).into());
            }

            // otherwise devConstraints contains a list of == operators for branches which are
            // disjunctive with the rest of the constraint
            constraints.extend(dev_constraints);
        }

        if constraints.len() > 1 {
            return Ok(MultiConstraint::new(constraints, false, None).into());
        }

        if constraints.len() == 1 {
            return Ok(constraints.into_iter().next().unwrap());
        }

        Ok(MatchNoneConstraint::new(None).into())
    }

    pub fn get(constraint: &AnyConstraint) -> anyhow::Result<IntervalCollection> {
        let key = constraint.to_string();

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
        constraint: &AnyConstraint,
        stop_on_first_valid_interval: bool,
    ) -> anyhow::Result<IntervalCollection> {
        if constraint.is_match_all() {
            return Ok(IntervalCollection {
                numeric: vec![Interval::new(
                    Interval::from_zero().clone(),
                    Interval::until_positive_infinity().clone(),
                )],
                branches: Interval::any_dev(),
            });
        }

        if constraint.is_match_none() {
            return Ok(IntervalCollection {
                numeric: vec![],
                branches: Interval::no_dev(),
            });
        }

        if let Some(c) = constraint.as_constraint() {
            return Self::generate_single_constraint_intervals(c);
        }

        let multi = constraint.as_multi_constraint().ok_or_else(|| {
            anyhow::anyhow!(
                "The constraint passed in should be an MatchAllConstraint, Constraint or \
                    MultiConstraint instance, got an unknown type."
            )
        })?;

        let sub_constraints = multi.get_constraints();

        let mut numeric_groups: Vec<Vec<Interval>> = Vec::new();
        let mut constraint_branches: Vec<DevConstraintSet> = Vec::new();
        for c in sub_constraints {
            let res = Self::get(c)?;
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
                        branches.names.retain(|n| b.names.contains(n));
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
                        branches.names.retain(|n| !b.names.contains(n));
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
                        branches.names.retain(|n| !b.names.contains(n));
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
                        branches.names.retain(|n| b.names.contains(n));
                    }
                }
            }
            branches
        };

        branches.names = shirabe_php_shim::array_unique(&branches.names);

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
            let order = shirabe_php_shim::version_compare_2(&a.0, &b.0);
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
        let mut start: Option<SimpleConstraint> = None;

        for (version, operator, is_start) in &borders {
            if *is_start {
                active_intervals += 1;
            } else {
                active_intervals -= 1;
            }

            if start.is_none() && active_intervals >= activation_threshold {
                start = Some(SimpleConstraint::new(
                    operator.clone(),
                    version.clone(),
                    None,
                ));
            } else if start.is_some() && active_intervals < activation_threshold {
                let start_c = start.take().unwrap();
                // filter out invalid intervals like > x - <= x, or >= x - < x
                if shirabe_php_shim::version_compare(start_c.get_version(), version, "=")
                    && ((start_c.get_operator() == ">" && operator == "<=")
                        || (start_c.get_operator() == ">=" && operator == "<"))
                {
                    // skip invalid interval (equivalent to PHP's unset($intervals[$index]))
                } else {
                    intervals.push(Interval::new(
                        start_c,
                        SimpleConstraint::new(operator.clone(), version.clone(), None),
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
        constraint: &SimpleConstraint,
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
                        SimpleConstraint::new(
                            "<".to_string(),
                            constraint.get_version().to_string(),
                            None,
                        ),
                    ),
                    Interval::new(
                        SimpleConstraint::new(
                            ">".to_string(),
                            constraint.get_version().to_string(),
                            None,
                        ),
                        Interval::until_positive_infinity().clone(),
                    ),
                ],
                branches: Interval::any_dev(),
            });
        }

        // convert ==x to an interval of >=x - <=x
        Ok(IntervalCollection {
            numeric: vec![Interval::new(
                SimpleConstraint::new(">=".to_string(), constraint.get_version().to_string(), None),
                SimpleConstraint::new("<=".to_string(), constraint.get_version().to_string(), None),
            )],
            branches: Interval::no_dev(),
        })
    }
}
