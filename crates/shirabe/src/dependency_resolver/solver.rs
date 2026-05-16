//! ref: composer/src/Composer/DependencyResolver/Solver.php

use indexmap::IndexMap;

use shirabe_php_shim::{array_pop, array_shift, array_unshift, microtime, spl_object_hash, sprintf, PhpMixed};
use shirabe_semver::constraint::constraint_interface::ConstraintInterface;

use crate::dependency_resolver::decisions::Decisions;
use crate::dependency_resolver::generic_rule::GenericRule;
use crate::dependency_resolver::lock_transaction::LockTransaction;
use crate::dependency_resolver::multi_conflict_rule::MultiConflictRule;
use crate::dependency_resolver::policy_interface::PolicyInterface;
use crate::dependency_resolver::pool::Pool;
use crate::dependency_resolver::problem::Problem;
use crate::dependency_resolver::request::Request;
use crate::dependency_resolver::rule::Rule;
use crate::dependency_resolver::rule_set::RuleSet;
use crate::dependency_resolver::rule_set_generator::RuleSetGenerator;
use crate::dependency_resolver::rule_watch_graph::RuleWatchGraph;
use crate::dependency_resolver::rule_watch_node::RuleWatchNode;
use crate::dependency_resolver::solver_bug_exception::SolverBugException;
use crate::dependency_resolver::solver_problems_exception::SolverProblemsException;
use crate::filter::platform_requirement_filter::ignore_list_platform_requirement_filter::IgnoreListPlatformRequirementFilter;
use crate::filter::platform_requirement_filter::platform_requirement_filter_factory::PlatformRequirementFilterFactory;
use crate::filter::platform_requirement_filter::platform_requirement_filter_interface::PlatformRequirementFilterInterface;
use crate::io::io_interface::IOInterface;
use crate::package::base_package::BasePackage;

#[derive(Debug)]
pub struct Solver {
    pub(crate) policy: Box<dyn PolicyInterface>,
    pub(crate) pool: Pool,

    pub(crate) rules: RuleSet,

    pub(crate) watch_graph: RuleWatchGraph,
    pub(crate) decisions: Decisions,
    pub(crate) fixed_map: IndexMap<i64, Box<BasePackage>>,

    pub(crate) propagate_index: i64,
    /// Pairs of `(literals, level)` — PHP indexes into these with the BRANCH_* constants.
    pub(crate) branches: Vec<(Vec<i64>, i64)>,
    pub(crate) problems: Vec<Problem>,
    pub(crate) learned_pool: Vec<Vec<Rule>>,
    pub(crate) learned_why: IndexMap<String, i64>,

    pub test_flag_learned_positive_literal: bool,

    pub(crate) io: Box<dyn IOInterface>,
}

impl Solver {
    const BRANCH_LITERALS: usize = 0;
    const BRANCH_LEVEL: usize = 1;

    pub fn new(policy: Box<dyn PolicyInterface>, pool: Pool, io: Box<dyn IOInterface>) -> Self {
        Self {
            policy,
            pool,
            rules: RuleSet::new(),
            watch_graph: RuleWatchGraph::new(),
            decisions: Decisions::new(Pool::default()),
            fixed_map: IndexMap::new(),
            propagate_index: 0,
            branches: Vec::new(),
            problems: Vec::new(),
            learned_pool: Vec::new(),
            learned_why: IndexMap::new(),
            test_flag_learned_positive_literal: false,
            io,
        }
    }

    pub fn get_rule_set_size(&self) -> i64 {
        self.rules.count()
    }

    pub fn get_pool(&self) -> &Pool {
        &self.pool
    }

    // aka solver_makeruledecisions

    fn make_assertion_rule_decisions(&mut self) -> anyhow::Result<()> {
        let decision_start = (self.decisions.count() as i64) - 1;

        let rules_count = self.rules.count();
        let mut rule_index = 0_i64;
        while rule_index < rules_count {
            let rule = self.rules.rule_by_id(rule_index).clone();

            if !rule.is_assertion() || rule.is_disabled() {
                rule_index += 1;
                continue;
            }

            let literals = rule.get_literals();
            let literal = literals[0];

            if !self.decisions.decided(literal) {
                self.decisions.decide(literal, 1, rule.clone());
                rule_index += 1;
                continue;
            }

            if self.decisions.satisfy(literal) {
                rule_index += 1;
                continue;
            }

            // found a conflict
            if RuleSet::TYPE_LEARNED == rule.get_type() {
                let mut rule_mut = self.rules.rule_by_id_mut(rule_index);
                rule_mut.disable()?;
                rule_index += 1;
                continue;
            }

            let conflict = self.decisions.decision_rule(literal).clone();

            if RuleSet::TYPE_PACKAGE == conflict.get_type() {
                let mut problem = Problem::new();

                problem.add_rule(rule.clone());
                problem.add_rule(conflict);
                self.rules.rule_by_id_mut(rule_index).disable()?;
                self.problems.push(problem);
                rule_index += 1;
                continue;
            }

            // conflict with another root require/fixed package
            let mut problem = Problem::new();
            problem.add_rule(rule.clone());
            problem.add_rule(conflict);

            // push all of our rules (can only be root require/fixed package rules)
            // asserting this literal on the problem stack
            let request_rules: Vec<i64> = self
                .rules
                .get_iterator_for(vec![RuleSet::TYPE_REQUEST])
                .ids()
                .collect();
            for assert_rule_id in request_rules {
                let assert_rule = self.rules.rule_by_id(assert_rule_id).clone();
                if assert_rule.is_disabled() || !assert_rule.is_assertion() {
                    continue;
                }

                let assert_rule_literals = assert_rule.get_literals();
                let assert_rule_literal = assert_rule_literals[0];

                if literal.abs() != assert_rule_literal.abs() {
                    continue;
                }
                problem.add_rule(assert_rule);
                self.rules.rule_by_id_mut(assert_rule_id).disable()?;
            }
            self.problems.push(problem);

            self.decisions.reset_to_offset(decision_start);
            rule_index = -1;
            rule_index += 1;
        }
        Ok(())
    }

    fn setup_fixed_map(&mut self, request: &Request) {
        self.fixed_map = IndexMap::new();
        for package in request.get_fixed_packages() {
            self.fixed_map.insert(package.id, package.clone());
        }
    }

    fn check_for_root_require_problems(
        &mut self,
        request: &Request,
        platform_requirement_filter: &dyn PlatformRequirementFilterInterface,
    ) {
        for (package_name, constraint) in request.get_requires() {
            let mut constraint: Box<dyn ConstraintInterface> = constraint.clone();
            if platform_requirement_filter.is_ignored(package_name) {
                continue;
            } else if let Some(ignore_filter) = platform_requirement_filter
                .as_any()
                .downcast_ref::<IgnoreListPlatformRequirementFilter>()
            {
                constraint = ignore_filter.filter_constraint(package_name, constraint);
            }

            if self
                .pool
                .what_provides(package_name, Some(constraint.as_ref()))
                .is_empty()
            {
                let mut problem = Problem::new();
                let mut reason_data: IndexMap<String, PhpMixed> = IndexMap::new();
                reason_data.insert(
                    "packageName".to_string(),
                    PhpMixed::String(package_name.clone()),
                );
                // TODO(phase-b): store the constraint inside reason_data; PhpMixed needs to
                // accept a `dyn ConstraintInterface` wrapper.
                reason_data
                    .insert("constraint".to_string(), PhpMixed::Null);
                problem.add_rule(Rule::generic(GenericRule::new(
                    Vec::new(),
                    PhpMixed::Int(Rule::RULE_ROOT_REQUIRE),
                    PhpMixed::Array(
                        reason_data
                            .into_iter()
                            .map(|(k, v)| (k, Box::new(v)))
                            .collect(),
                    ),
                )));
                self.problems.push(problem);
            }
        }
    }

    pub fn solve(
        &mut self,
        request: &Request,
        platform_requirement_filter: Option<Box<dyn PlatformRequirementFilterInterface>>,
    ) -> anyhow::Result<LockTransaction> {
        let platform_requirement_filter = platform_requirement_filter
            .unwrap_or_else(|| PlatformRequirementFilterFactory::ignore_nothing());

        self.setup_fixed_map(request);

        self.io.write_error(
            PhpMixed::String("Generating rules".to_string()),
            true,
            <dyn IOInterface>::DEBUG,
        );
        let mut rule_set_generator = RuleSetGenerator::new(self.policy.clone_box(), self.pool.clone());
        self.rules =
            rule_set_generator.get_rules_for(request, platform_requirement_filter.as_ref())?;
        drop(rule_set_generator);
        self.check_for_root_require_problems(request, platform_requirement_filter.as_ref());
        self.decisions = Decisions::new(self.pool.clone());
        self.watch_graph = RuleWatchGraph::new();

        for rule in self.rules.iter() {
            self.watch_graph
                .insert(RuleWatchNode::new(rule.clone()))?;
        }

        // make decisions based on root require/fix assertions
        self.make_assertion_rule_decisions()?;

        self.io.write_error(
            PhpMixed::String("Resolving dependencies through SAT".to_string()),
            true,
            <dyn IOInterface>::DEBUG,
        );
        let before = microtime(true);
        self.run_sat()?;
        self.io.write_error(
            PhpMixed::String("".to_string()),
            true,
            <dyn IOInterface>::DEBUG,
        );
        self.io.write_error(
            PhpMixed::String(sprintf(
                "Dependency resolution completed in %.3f seconds",
                &[PhpMixed::Float(microtime(true) - before)],
            )),
            true,
            <dyn IOInterface>::VERBOSE,
        );

        if self.problems.len() > 0 {
            return Err(anyhow::anyhow!(SolverProblemsException::new(
                std::mem::take(&mut self.problems),
                self.learned_pool.clone(),
            )));
        }

        Ok(LockTransaction::new(
            self.pool.clone(),
            request.get_present_map(),
            request.get_fixed_packages_map(),
            self.decisions.clone(),
        ))
    }

    /// Makes a decision and propagates it to all rules.
    ///
    /// Evaluates each term affected by the decision (linked through watches)
    /// If we find unit rules we make new decisions based on them
    ///
    /// Returns a `Rule` on conflict, otherwise `None`.
    fn propagate(&mut self, level: i64) -> Option<Rule> {
        while self.decisions.valid_offset(self.propagate_index) {
            let decision = self
                .decisions
                .at_offset(self.propagate_index as usize)
                .clone();

            let conflict = self.watch_graph.propagate_literal(
                decision.0,
                level,
                &mut self.decisions,
            );

            self.propagate_index += 1;

            if conflict.is_some() {
                return conflict;
            }
        }

        None
    }

    /// Reverts a decision at the given level.
    fn revert(&mut self, level: i64) {
        while !self.decisions.is_empty() {
            let literal = self.decisions.last_literal();

            if self.decisions.undecided(literal) {
                break;
            }

            let decision_level = self.decisions.decision_level(literal);

            if decision_level <= level {
                break;
            }

            self.decisions.revert_last();
            self.propagate_index = self.decisions.count() as i64;
        }

        while !self.branches.is_empty()
            && self.branches[self.branches.len() - 1].1 >= level
        {
            // PHP: array_pop($this->branches)
            self.branches.pop();
        }
    }

    /// setpropagatelearn
    ///
    /// add free decision (a positive literal) to decision queue
    /// increase level and propagate decision
    /// return if no conflict.
    ///
    /// in conflict case, analyze conflict rule, add resulting
    /// rule to learnt rule set, make decision from learnt
    /// rule (always unit) and re-propagate.
    ///
    /// returns the new solver level or 0 if unsolvable
    fn set_propagate_learn(&mut self, level: i64, literal: i64, rule: Rule) -> anyhow::Result<i64> {
        let mut level = level + 1;

        self.decisions.decide(literal, level, rule);

        loop {
            let rule = self.propagate(level);

            let rule = match rule {
                None => break,
                Some(r) => r,
            };

            if level == 1 {
                self.analyze_unsolvable(&rule);

                return Ok(0);
            }

            // conflict
            let (learn_literal, new_level, new_rule, why) = self.analyze(level, rule)?;

            if new_level <= 0 || new_level >= level {
                return Err(anyhow::anyhow!(SolverBugException::new(format!(
                    "Trying to revert to invalid level {} from level {}.",
                    new_level, level
                ))));
            }

            level = new_level;

            self.revert(level);

            self.rules
                .add(new_rule.clone().into(), RuleSet::TYPE_LEARNED)?;

            self.learned_why
                .insert(spl_object_hash(&new_rule), why);

            let mut rule_node = RuleWatchNode::new(new_rule.clone().into());
            rule_node.watch2_on_highest(&self.decisions);
            self.watch_graph.insert(rule_node)?;

            self.decisions.decide(learn_literal, level, new_rule.into());
        }

        Ok(level)
    }

    fn select_and_install(
        &mut self,
        level: i64,
        decision_queue: Vec<i64>,
        rule: Rule,
    ) -> anyhow::Result<i64> {
        // choose best package to install from decisionQueue
        let mut literals =
            self.policy
                .select_preferred_packages(&self.pool, decision_queue, rule.get_required_package());

        let selected_literal = array_shift::<i64>(&mut literals);

        // if there are multiple candidates, then branch
        if literals.len() > 0 {
            self.branches.push((literals, level));
        }

        self.set_propagate_learn(level, selected_literal, rule)
    }

    fn analyze(
        &mut self,
        level: i64,
        rule: Rule,
    ) -> anyhow::Result<(i64, i64, GenericRule, i64)> {
        let analyzed_rule = rule.clone();
        let mut rule = rule;
        let mut rule_level = 1_i64;
        let mut num = 0_i64;
        let mut l1num = 0_i64;
        let mut seen: IndexMap<i64, bool> = IndexMap::new();
        let mut learned_literal: Option<i64> = None;
        let mut other_learned_literals: Vec<i64> = Vec::new();

        let mut decision_id = self.decisions.count() as i64;

        self.learned_pool.push(Vec::new());

        'outer: loop {
            let last = self.learned_pool.len() - 1;
            self.learned_pool[last].push(rule.clone());

            for literal in rule.get_literals().clone() {
                // multiconflictrule is really a bunch of rules in one, so some may not have finished propagating yet
                if rule.as_multi_conflict().is_some() && !self.decisions.decided(literal) {
                    continue;
                }

                // skip the one true literal
                if self.decisions.satisfy(literal) {
                    continue;
                }

                if seen.contains_key(&literal.abs()) {
                    continue;
                }
                seen.insert(literal.abs(), true);

                let l = self.decisions.decision_level(literal);

                if 1 == l {
                    l1num += 1;
                } else if level == l {
                    num += 1;
                } else {
                    // not level1 or conflict level, add to new rule
                    other_learned_literals.push(literal);

                    if l > rule_level {
                        rule_level = l;
                    }
                }
            }
            // unset($literal); — no-op in Rust

            let mut l1retry = true;
            let mut literal_for_outer: i64 = 0;
            while l1retry {
                l1retry = false;

                if 0 == num && {
                    l1num -= 1;
                    0 == l1num
                } {
                    // all level 1 literals done
                    break 'outer;
                }

                let inner_literal = loop {
                    if decision_id <= 0 {
                        return Err(anyhow::anyhow!(SolverBugException::new(format!(
                            "Reached invalid decision id {} while looking through {} for a literal present in the analyzed rule {}.",
                            decision_id,
                            rule.to_string(),
                            analyzed_rule.to_string()
                        ))));
                    }

                    decision_id -= 1;

                    let decision = self.decisions.at_offset(decision_id as usize).clone();
                    let lit = decision.0;

                    if seen.contains_key(&lit.abs()) {
                        break lit;
                    }
                };

                seen.shift_remove(&inner_literal.abs());

                if 0 != num && {
                    num -= 1;
                    0 == num
                } {
                    if inner_literal < 0 {
                        self.test_flag_learned_positive_literal = true;
                    }
                    learned_literal = Some(-inner_literal);

                    if 0 == l1num {
                        break 'outer;
                    }

                    for other_literal in &other_learned_literals {
                        seen.shift_remove(&other_literal.abs());
                    }
                    // only level 1 marks left
                    l1num += 1;
                    l1retry = true;
                } else {
                    let decision = self.decisions.at_offset(decision_id as usize).clone();
                    rule = decision.1;

                    if rule.as_multi_conflict().is_some() {
                        // there is only ever exactly one positive decision in a MultiConflictRule
                        for rule_literal in rule.get_literals().clone() {
                            if !seen.contains_key(&rule_literal.abs())
                                && self.decisions.satisfy(-rule_literal)
                            {
                                let last = self.learned_pool.len() - 1;
                                self.learned_pool[last].push(rule.clone());
                                let l = self.decisions.decision_level(rule_literal);
                                if 1 == l {
                                    l1num += 1;
                                } else if level == l {
                                    num += 1;
                                } else {
                                    // not level1 or conflict level, add to new rule
                                    other_learned_literals.push(rule_literal);

                                    if l > rule_level {
                                        rule_level = l;
                                    }
                                }
                                seen.insert(rule_literal.abs(), true);
                                break;
                            }
                        }

                        l1retry = true;
                    }
                }
                literal_for_outer = inner_literal;
            }
            let _ = literal_for_outer;

            let decision = self.decisions.at_offset(decision_id as usize).clone();
            rule = decision.1;
        }

        let why = (self.learned_pool.len() as i64) - 1;

        let learned_literal = match learned_literal {
            Some(l) => l,
            None => {
                return Err(anyhow::anyhow!(SolverBugException::new(format!(
                    "Did not find a learnable literal in analyzed rule {}.",
                    analyzed_rule.to_string()
                ))));
            }
        };

        array_unshift::<i64>(&mut other_learned_literals, learned_literal);
        let new_rule = GenericRule::new(
            other_learned_literals,
            PhpMixed::Int(Rule::RULE_LEARNED),
            PhpMixed::Int(why),
        );

        Ok((learned_literal, rule_level, new_rule, why))
    }

    fn analyze_unsolvable_rule(
        &self,
        problem: &mut Problem,
        conflict_rule: &Rule,
        rule_seen: &mut IndexMap<String, bool>,
    ) {
        let why = spl_object_hash(conflict_rule);
        rule_seen.insert(why.clone(), true);

        if conflict_rule.get_type() == RuleSet::TYPE_LEARNED {
            let learned_why = self.learned_why[&why];
            let problem_rules = self.learned_pool[learned_why as usize].clone();

            for problem_rule in &problem_rules {
                if !rule_seen.contains_key(&spl_object_hash(problem_rule)) {
                    self.analyze_unsolvable_rule(problem, problem_rule, rule_seen);
                }
            }

            return;
        }

        if conflict_rule.get_type() == RuleSet::TYPE_PACKAGE {
            // package rules cannot be part of a problem
            return;
        }

        problem.next_section();
        problem.add_rule(conflict_rule.clone());
    }

    fn analyze_unsolvable(&mut self, conflict_rule: &Rule) {
        let mut problem = Problem::new();
        problem.add_rule(conflict_rule.clone());

        let mut rule_seen: IndexMap<String, bool> = IndexMap::new();

        self.analyze_unsolvable_rule(&mut problem, conflict_rule, &mut rule_seen);

        let mut seen: IndexMap<i64, bool> = IndexMap::new();
        let literals = conflict_rule.get_literals().clone();

        for literal in &literals {
            // skip the one true literal
            if self.decisions.satisfy(*literal) {
                continue;
            }
            seen.insert(literal.abs(), true);
        }

        for decision in self.decisions.iter() {
            let decision_literal = decision.0;

            // skip literals that are not in this rule
            if !seen.contains_key(&decision_literal.abs()) {
                continue;
            }

            let why = decision.1.clone();

            problem.add_rule(why.clone());
            self.analyze_unsolvable_rule(&mut problem, &why, &mut rule_seen);

            let literals = why.get_literals().clone();
            for literal in &literals {
                // skip the one true literal
                if self.decisions.satisfy(*literal) {
                    continue;
                }
                seen.insert(literal.abs(), true);
            }
        }

        self.problems.push(problem);
    }

    fn run_sat(&mut self) -> anyhow::Result<()> {
        self.propagate_index = 0;

        // here's the main loop:
        // 1) propagate new decisions (only needed once)
        // 2) fulfill root requires/fixed packages
        // 3) fulfill all unresolved rules
        // 4) minimalize solution if we had choices
        // if we encounter a problem, we rewind to a safe level and restart
        // with step 1

        let mut level = 1_i64;
        let mut system_level = level + 1;

        loop {
            if 1 == level {
                let conflict_rule = self.propagate(level);
                if let Some(cr) = conflict_rule {
                    self.analyze_unsolvable(&cr);

                    return Ok(());
                }
            }

            // handle root require/fixed package rules
            if level < system_level {
                let mut iterator = self.rules.get_iterator_for(vec![RuleSet::TYPE_REQUEST]);
                let mut broke_inner = false;
                while iterator.valid() {
                    let rule = iterator.current().clone();
                    if rule.is_enabled() {
                        let mut decision_queue: Vec<i64> = Vec::new();
                        let mut none_satisfied = true;

                        for literal in rule.get_literals().clone() {
                            if self.decisions.satisfy(literal) {
                                none_satisfied = false;
                                break;
                            }
                            if literal > 0 && self.decisions.undecided(literal) {
                                decision_queue.push(literal);
                            }
                        }

                        if none_satisfied && decision_queue.len() > 0 {
                            // if any of the options in the decision queue are fixed, only use those
                            let mut pruned_queue: Vec<i64> = Vec::new();
                            for literal in &decision_queue {
                                if self.fixed_map.contains_key(&literal.abs()) {
                                    pruned_queue.push(*literal);
                                }
                            }
                            if pruned_queue.len() > 0 {
                                decision_queue = pruned_queue;
                            }
                        }

                        if none_satisfied && decision_queue.len() > 0 {
                            let o_level = level;
                            level = self.select_and_install(level, decision_queue, rule)?;

                            if 0 == level {
                                return Ok(());
                            }
                            if level <= o_level {
                                broke_inner = true;
                                break;
                            }
                        }
                    }
                    iterator.advance();
                }
                let _ = broke_inner;

                system_level = level + 1;

                // root requires/fixed packages left
                iterator.next();
                if iterator.valid() {
                    continue;
                }
            }

            if level < system_level {
                system_level = level;
            }

            let mut rules_count = self.rules.count();
            let mut pass = 1_i64;

            self.io.write_error(
                PhpMixed::String("Looking at all rules.".to_string()),
                true,
                <dyn IOInterface>::DEBUG,
            );
            let mut i = 0_i64;
            let mut n = 0_i64;
            while n < rules_count {
                if i == rules_count {
                    if 1 == pass {
                        self.io.write_error(
                            PhpMixed::String(format!(
                                "Something's changed, looking at all rules again (pass #{})",
                                pass
                            )),
                            false,
                            <dyn IOInterface>::DEBUG,
                        );
                    } else {
                        self.io.overwrite_error(
                            PhpMixed::String(format!(
                                "Something's changed, looking at all rules again (pass #{})",
                                pass
                            )),
                            false,
                            None,
                            <dyn IOInterface>::DEBUG,
                        );
                    }

                    i = 0;
                    pass += 1;
                }

                let rule = self.rules.rule_by_id(i).clone();
                let literals = rule.get_literals().clone();

                if rule.is_disabled() {
                    i += 1;
                    n += 1;
                    continue;
                }

                let mut decision_queue: Vec<i64> = Vec::new();
                let mut continue_next_rule = false;

                // make sure that
                // * all negative literals are installed
                // * no positive literal is installed
                // i.e. the rule is not fulfilled and we
                // just need to decide on the positive literals
                //
                for literal in &literals {
                    if *literal <= 0 {
                        if !self.decisions.decided_install(*literal) {
                            continue_next_rule = true; // next rule
                            break;
                        }
                    } else {
                        if self.decisions.decided_install(*literal) {
                            continue_next_rule = true; // next rule
                            break;
                        }
                        if self.decisions.undecided(*literal) {
                            decision_queue.push(*literal);
                        }
                    }
                }

                if continue_next_rule {
                    i += 1;
                    n += 1;
                    continue;
                }

                // need to have at least 2 item to pick from
                if decision_queue.len() < 2 {
                    i += 1;
                    n += 1;
                    continue;
                }

                level = self.select_and_install(level, decision_queue, rule)?;

                if 0 == level {
                    return Ok(());
                }

                // something changed, so look at all rules again
                rules_count = self.rules.count();
                n = -1;

                i += 1;
                n += 1;
            }

            if level < system_level {
                continue;
            }

            // minimization step
            if self.branches.len() > 0 {
                let mut last_literal: Option<i64> = None;
                let mut last_level: Option<i64> = None;
                let mut last_branch_index = 0_usize;
                let mut last_branch_offset = 0_usize;

                let mut i = (self.branches.len() as i64) - 1;
                while i >= 0 {
                    let (literals, l) = self.branches[i as usize].clone();

                    for (offset, literal) in literals.iter().enumerate() {
                        if *literal > 0 && self.decisions.decision_level(*literal) > l + 1 {
                            last_literal = Some(*literal);
                            last_branch_index = i as usize;
                            last_branch_offset = offset;
                            last_level = Some(l);
                        }
                    }
                    i -= 1;
                }

                if let Some(last_literal_v) = last_literal {
                    let last_level_v = last_level.expect("lastLevel set with lastLiteral");
                    // unset($this->branches[$lastBranchIndex][BRANCH_LITERALS][$lastBranchOffset])
                    self.branches[last_branch_index]
                        .0
                        .remove(last_branch_offset);

                    level = last_level_v;
                    self.revert(level);

                    let why = self.decisions.last_reason().clone();

                    level = self.set_propagate_learn(level, last_literal_v, why)?;

                    if level == 0 {
                        return Ok(());
                    }

                    continue;
                }
            }

            break;
        }
        Ok(())
    }
}
