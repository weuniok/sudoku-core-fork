//! Solver orchestrator.
//!
//! Dispatches to Fish, ALS, AIC, and arithmetic engines alongside basic
//! techniques, uniqueness patterns, and backtracking.

mod aic_engine;
mod als_engine;
mod arithmetic;
mod arithmetic_replay;
pub(crate) mod backtrack;
mod basic;
pub(crate) mod explain;
pub(crate) mod fabric;
mod fish_engine;
mod search_budget;
mod teaching;
mod teaching_contract;
mod teaching_forcing;
mod teaching_proof;
#[cfg(test)]
mod teaching_tests;
mod types;
mod uniqueness;

use std::collections::HashMap;

use crate::{Grid, Position};
use explain::{Finding, InferenceResult};
use fabric::{idx_to_pos, CandidateFabric};

pub use arithmetic::{
    ArithmeticCheck, ArithmeticProof, ArithmeticRequirement, ArithmeticSearchOptions,
    ArithmeticSearchResult, ArithmeticTerm, ArithmeticTerminal,
};
pub use arithmetic_replay::{ArithmeticReplay, ArithmeticState};
pub use explain::{AlsProofDescriptor, ForcingSource, LinkType, Polarity, ProofCertificate};
pub use teaching::{search_teaching_hint, search_teaching_hint_with_options, TEACHING_TECHNIQUES};
pub use teaching_contract::{
    TeachingBranch, TeachingCandidate, TeachingConjugate, TeachingContradiction, TeachingDeduction,
    TeachingEvidence, TeachingGroup, TeachingHint, TeachingHouse, TeachingLink, TeachingNode,
    TeachingProof, TeachingRelation, TeachingSearchResult, TeachingUniqueness,
};
pub use types::{Difficulty, Hint, HintType, Technique};

/// Unit struct solver — stateless, all state is per-call.
pub struct Solver;

impl Default for Solver {
    fn default() -> Self {
        Self::new()
    }
}

impl Solver {
    /// Create a new solver.
    pub fn new() -> Self {
        Self
    }

    /// Solve the puzzle, returning the solved grid if successful.
    pub fn solve(&self, grid: &Grid) -> Option<Grid> {
        let mut working = grid.deep_clone();
        working.recalculate_candidates();
        if backtrack::solve_recursive(&mut working) {
            Some(working)
        } else {
            None
        }
    }

    /// Count solutions up to a limit.
    pub fn count_solutions(&self, grid: &Grid, limit: usize) -> usize {
        let mut working = grid.deep_clone();
        working.recalculate_candidates();
        let mut count = 0;
        backtrack::count_solutions_recursive(&mut working, &mut count, limit);
        count
    }

    /// Check if the puzzle has exactly one solution.
    pub fn has_unique_solution(&self, grid: &Grid) -> bool {
        self.count_solutions(grid, 2) == 1
    }

    /// Get a hint for the current position.
    pub fn get_hint(&self, grid: &Grid) -> Option<Hint> {
        let mut working = grid.deep_clone();
        working.recalculate_candidates();

        if let Some(finding) = self.find_first_technique(&working) {
            return Some(finding.to_hint());
        }

        // Last resort: backtracking hint
        if let Some(finding) = backtrack::find_backtracking_hint(&working) {
            return Some(finding.to_hint());
        }

        None
    }

    /// Find a verified arithmetic hint using the grid's current candidate masks.
    ///
    /// Existing candidate eliminations are preserved: this method does not
    /// recalculate candidates or mutate the grid. The default search is bounded;
    /// `None` does not establish that no arithmetic deduction exists. Use
    /// [`Self::search_arithmetic`] for explicit search limits and budget status.
    pub fn get_arithmetic_hint(&self, grid: &Grid) -> Option<Hint> {
        arithmetic::search(grid, &ArithmeticSearchOptions::default()).hint
    }

    /// Search for an arithmetic hint with explicit resource limits.
    ///
    /// The search preserves the supplied candidate masks and reports work and
    /// budget status alongside any verified hint. Returning no hint does not
    /// prove that no arithmetic deduction exists; inspect the search status.
    pub fn search_arithmetic(
        &self,
        grid: &Grid,
        options: &ArithmeticSearchOptions,
    ) -> ArithmeticSearchResult {
        arithmetic::search(grid, options)
    }

    /// Get the next placement hint by chaining through elimination techniques.
    ///
    /// Unlike `get_hint` which returns the first technique found (which may be
    /// an elimination), this method applies eliminations internally and keeps
    /// searching until it finds a placement (SetValue) hint. This prevents
    /// callers from looping on the same elimination when they recalculate
    /// candidates from scratch between calls.
    pub fn get_next_placement(&self, grid: &Grid) -> Option<Hint> {
        let mut working = grid.deep_clone();
        working.recalculate_candidates();

        // Solve once via backtracking to verify chained results.
        // This prevents subtle technique bugs from producing wrong hints.
        let solution = self.solve(grid)?;

        for _ in 0..500 {
            if working.is_complete() {
                return None;
            }

            if let Some(finding) = self.find_first_technique(&working) {
                match &finding.inference {
                    InferenceResult::Placement { cell, value } => {
                        let pos = idx_to_pos(*cell);
                        if solution.get(pos) == Some(*value) {
                            return Some(finding.to_hint());
                        }
                        // Unsound placement — chaining corrupted state
                        break;
                    }
                    InferenceResult::Elimination { cell, values } => {
                        let pos = idx_to_pos(*cell);
                        let sol_val = solution.get(pos);
                        if values.iter().any(|&v| sol_val == Some(v)) {
                            // Unsound elimination — stop chaining
                            break;
                        }
                        apply_finding(&mut working, &finding);
                    }
                }
            } else {
                break;
            }
        }

        // Fall back to backtracking hint (always correct)
        backtrack::find_backtracking_hint(&working).map(|f| f.to_hint())
    }

    /// Rate the difficulty of a puzzle.
    pub fn rate_difficulty(&self, grid: &Grid) -> Difficulty {
        let empty_count = grid.empty_positions().len();
        let mut working = grid.deep_clone();
        let max_tech = self.solve_with_techniques(&mut working);
        Self::technique_to_difficulty(max_tech, empty_count)
    }

    /// Rate the puzzle using the engine's SE-style numerical scale.
    ///
    /// Arithmetic Counting uses an uncalibrated engine-local estimate, not a
    /// published Sudoku Explainer rating. See [`Technique::se_rating`].
    pub fn rate_se(&self, grid: &Grid) -> f32 {
        let mut working = grid.deep_clone();
        let max_tech = self.solve_with_techniques(&mut working);
        max_tech.se_rating()
    }

    /// Analyze a puzzle: returns (difficulty, se_rating) with a single solve_with_techniques pass.
    pub fn analyze(&self, grid: &Grid) -> (Difficulty, f32) {
        let empty_count = grid.empty_positions().len();
        let mut working = grid.deep_clone();
        let max_tech = self.solve_with_techniques(&mut working);
        (
            Self::technique_to_difficulty(max_tech, empty_count),
            max_tech.se_rating(),
        )
    }

    /// Collect a full technique profile by solving the puzzle step-by-step.
    ///
    /// Returns a map of technique display names to usage counts, along with the
    /// hardest technique used. Returns `None` if the puzzle cannot be solved.
    pub fn collect_technique_profile(
        &self,
        grid: &Grid,
    ) -> Option<(HashMap<String, u32>, Technique)> {
        let mut working = grid.deep_clone();
        let mut techniques: HashMap<String, u32> = HashMap::new();
        let max = self.solve_with_techniques_inner(&mut working, Some(&mut techniques));

        if working.is_complete() {
            return Some((techniques, max));
        }

        // The inline technique chain can fail when uniqueness techniques produce
        // unsound eliminations on accumulated candidate state.  When backtracking
        // can't recover, fall back to solving from the original grid.
        if self.solve(grid).is_some() {
            techniques.clear();
            techniques.insert(Technique::Backtracking.to_string(), 1);
            Some((techniques, Technique::Backtracking))
        } else {
            None
        }
    }

    // ==================== Internal dispatch ====================

    /// Find the first applicable technique for a hint (does not mutate grid).
    fn find_first_technique(&self, grid: &Grid) -> Option<Finding> {
        let fab = CandidateFabric::from_grid(grid);

        // Phase 1: Basic
        if let Some(f) = basic::find_naked_single(&fab) {
            return Some(f);
        }
        if let Some(f) = basic::find_hidden_single(&fab) {
            return Some(f);
        }

        // Phase 2: Subsets
        if let Some(f) = basic::find_naked_subset(&fab, 2) {
            return Some(f);
        }
        if let Some(f) = basic::find_hidden_subset(&fab, 2) {
            return Some(f);
        }
        if let Some(f) = basic::find_naked_subset(&fab, 3) {
            return Some(f);
        }
        if let Some(f) = basic::find_hidden_subset(&fab, 3) {
            return Some(f);
        }

        // Phase 3: Intersections (size-1 fish)
        if let Some(f) = fish_engine::find_pointing_pair(&fab) {
            return Some(f);
        }
        if let Some(f) = fish_engine::find_box_line_reduction(&fab) {
            return Some(f);
        }

        // Phase 4: Fish (size 2+) + quads
        if let Some(f) = fish_engine::find_basic_fish(&fab, 2) {
            return Some(f);
        }
        if let Some(f) = fish_engine::find_finned_fish(&fab, 2) {
            return Some(f);
        }
        if let Some(f) = fish_engine::find_basic_fish(&fab, 3) {
            return Some(f);
        }
        if let Some(f) = fish_engine::find_finned_fish(&fab, 3) {
            return Some(f);
        }
        if let Some(f) = fish_engine::find_basic_fish(&fab, 4) {
            return Some(f);
        }
        if let Some(f) = fish_engine::find_finned_fish(&fab, 4) {
            return Some(f);
        }
        if let Some(f) = basic::find_naked_subset(&fab, 4) {
            return Some(f);
        }
        if let Some(f) = basic::find_hidden_subset(&fab, 4) {
            return Some(f);
        }

        // Phase 5: Uniqueness
        if let Some(f) = aic_engine::find_empty_rectangle(&fab) {
            return Some(f);
        }
        if let Some(f) = uniqueness::find_avoidable_rectangle(&fab) {
            return Some(f);
        }
        if let Some(f) = uniqueness::find_unique_rectangle(&fab) {
            return Some(f);
        }
        if let Some(f) = uniqueness::find_hidden_rectangle(&fab) {
            return Some(f);
        }

        // Phase 6: Master
        if let Some(f) = als_engine::find_xy_wing(&fab) {
            return Some(f);
        }
        if let Some(f) = als_engine::find_xyz_wing(&fab) {
            return Some(f);
        }
        if let Some(f) = als_engine::find_wxyz_wing(&fab) {
            return Some(f);
        }
        if let Some(f) = aic_engine::find_w_wing(&fab) {
            return Some(f);
        }
        // AIC family: shared link graph for X-Chain, 3D Medusa, AIC
        let graph = aic_engine::build_link_graph(&fab);
        if let Some(f) = aic_engine::find_x_chain(&fab, &graph) {
            return Some(f);
        }
        // Legacy: retained for SE compatibility, subsumed by AIC
        #[allow(deprecated)]
        if let Some(f) = aic_engine::find_medusa(&fab, &graph) {
            return Some(f);
        }
        if let Some(f) = als_engine::find_sue_de_coq(&fab) {
            return Some(f);
        }
        if let Some(f) = aic_engine::find_aic(&fab, &graph) {
            return Some(f);
        }
        if let Some(f) = fish_engine::find_franken_fish(&fab) {
            return Some(f);
        }
        if let Some(f) = fish_engine::find_siamese_fish(&fab) {
            return Some(f);
        }
        if let Some(f) = als_engine::find_als_xz(&fab) {
            return Some(f);
        }
        if let Some(f) = uniqueness::find_extended_unique_rectangle(&fab) {
            return Some(f);
        }
        if let Some(f) = uniqueness::find_bug(&fab) {
            return Some(f);
        }

        // Phase 7: Extreme
        if let Some(f) = als_engine::find_als_xy_wing(&fab) {
            return Some(f);
        }
        if let Some(f) = als_engine::find_als_chain(&fab) {
            return Some(f);
        }
        if let Some(f) = fish_engine::find_mutant_fish(&fab) {
            return Some(f);
        }
        // Legacy: retained for SE compatibility, subsumed by ALS chains
        #[allow(deprecated)]
        if let Some(f) = als_engine::find_aligned_pair_exclusion(&fab) {
            return Some(f);
        }
        // Legacy: retained for SE compatibility, subsumed by ALS chains
        #[allow(deprecated)]
        if let Some(f) = als_engine::find_aligned_triplet_exclusion(&fab) {
            return Some(f);
        }
        if let Some(f) = als_engine::find_death_blossom(&fab) {
            return Some(f);
        }
        if let Some(f) = arithmetic::find(grid, &fab) {
            return Some(f);
        }

        // Forcing chains need the Grid for propagation
        let propagate_singles = |g: &Grid, pos: Position, val: u8| -> (Grid, bool) {
            backtrack::propagate_singles(g, pos, val)
        };
        if let Some(f) = aic_engine::find_nishio_fc(grid, &propagate_singles) {
            return Some(f);
        }
        if let Some(f) = aic_engine::find_kraken_fish(grid, &propagate_singles) {
            return Some(f);
        }
        if let Some(f) = aic_engine::find_region_fc(grid, &propagate_singles) {
            return Some(f);
        }
        if let Some(f) = aic_engine::find_cell_fc(grid, &propagate_singles) {
            return Some(f);
        }
        // Dynamic FC uses full technique propagation
        let prop_full =
            |g: &Grid, pos: Position, val: u8| -> (Grid, bool) { propagate_full(g, pos, val) };
        if let Some(f) = aic_engine::find_dynamic_fc(grid, &prop_full) {
            return Some(f);
        }

        None
    }

    /// Solve the puzzle using human techniques, returning the hardest technique used.
    fn solve_with_techniques(&self, grid: &mut Grid) -> Technique {
        self.solve_with_techniques_inner(grid, None)
    }

    /// Shared implementation: solve with techniques, optionally recording usage counts.
    fn solve_with_techniques_inner(
        &self,
        grid: &mut Grid,
        mut track: Option<&mut HashMap<String, u32>>,
    ) -> Technique {
        grid.recalculate_candidates();
        let mut max_technique = Technique::NakedSingle;

        while !grid.is_complete() {
            let fab = CandidateFabric::from_grid(grid);

            // Try techniques in priority order via dispatch table
            let finding = None
                // Phase 1: Basic
                .or_else(|| basic::find_naked_single(&fab))
                .or_else(|| basic::find_hidden_single(&fab))
                // Phase 2: Subsets
                .or_else(|| basic::find_naked_subset(&fab, 2))
                .or_else(|| basic::find_hidden_subset(&fab, 2))
                .or_else(|| basic::find_naked_subset(&fab, 3))
                .or_else(|| basic::find_hidden_subset(&fab, 3))
                // Phase 3: Intersections (size-1 fish)
                .or_else(|| fish_engine::find_pointing_pair(&fab))
                .or_else(|| fish_engine::find_box_line_reduction(&fab))
                // Phase 4: Fish (size 2+) + quads
                .or_else(|| fish_engine::find_basic_fish(&fab, 2))
                .or_else(|| fish_engine::find_finned_fish(&fab, 2))
                .or_else(|| fish_engine::find_basic_fish(&fab, 3))
                .or_else(|| fish_engine::find_finned_fish(&fab, 3))
                .or_else(|| fish_engine::find_basic_fish(&fab, 4))
                .or_else(|| fish_engine::find_finned_fish(&fab, 4))
                .or_else(|| basic::find_naked_subset(&fab, 4))
                .or_else(|| basic::find_hidden_subset(&fab, 4))
                // Phase 5: Uniqueness
                .or_else(|| aic_engine::find_empty_rectangle(&fab))
                .or_else(|| uniqueness::find_avoidable_rectangle(&fab))
                .or_else(|| uniqueness::find_unique_rectangle(&fab))
                .or_else(|| uniqueness::find_hidden_rectangle(&fab))
                // Phase 6: Master
                .or_else(|| als_engine::find_xy_wing(&fab))
                .or_else(|| als_engine::find_xyz_wing(&fab))
                .or_else(|| als_engine::find_wxyz_wing(&fab))
                .or_else(|| aic_engine::find_w_wing(&fab))
                // AIC family: shared link graph for X-Chain, 3D Medusa, AIC
                .or_else(|| {
                    let graph = aic_engine::build_link_graph(&fab);
                    None.or_else(|| aic_engine::find_x_chain(&fab, &graph))
                        // Legacy: retained for SE compatibility, subsumed by AIC
                        .or_else(|| {
                            #[allow(deprecated)]
                            aic_engine::find_medusa(&fab, &graph)
                        })
                        .or_else(|| als_engine::find_sue_de_coq(&fab))
                        .or_else(|| aic_engine::find_aic(&fab, &graph))
                })
                .or_else(|| fish_engine::find_franken_fish(&fab))
                .or_else(|| fish_engine::find_siamese_fish(&fab))
                .or_else(|| als_engine::find_als_xz(&fab))
                .or_else(|| uniqueness::find_extended_unique_rectangle(&fab))
                .or_else(|| uniqueness::find_bug(&fab))
                // Phase 7: Extreme
                .or_else(|| als_engine::find_als_xy_wing(&fab))
                .or_else(|| als_engine::find_als_chain(&fab))
                .or_else(|| fish_engine::find_mutant_fish(&fab))
                // Legacy: retained for SE compatibility, subsumed by ALS chains
                .or_else(|| {
                    #[allow(deprecated)]
                    als_engine::find_aligned_pair_exclusion(&fab)
                })
                // Legacy: retained for SE compatibility, subsumed by ALS chains
                .or_else(|| {
                    #[allow(deprecated)]
                    als_engine::find_aligned_triplet_exclusion(&fab)
                })
                .or_else(|| als_engine::find_death_blossom(&fab))
                .or_else(|| arithmetic::find(grid, &fab))
                // Forcing chains (singles propagation)
                .or_else(|| {
                    let prop = |g: &Grid, pos: Position, val: u8| -> (Grid, bool) {
                        backtrack::propagate_singles(g, pos, val)
                    };
                    None.or_else(|| aic_engine::find_nishio_fc(grid, &prop))
                        .or_else(|| aic_engine::find_kraken_fish(grid, &prop))
                        .or_else(|| aic_engine::find_region_fc(grid, &prop))
                        .or_else(|| aic_engine::find_cell_fc(grid, &prop))
                })
                // Dynamic FC: full technique propagation
                .or_else(|| {
                    let prop_full = |g: &Grid, pos: Position, val: u8| -> (Grid, bool) {
                        propagate_full(g, pos, val)
                    };
                    aic_engine::find_dynamic_fc(grid, &prop_full)
                });

            match finding {
                Some(f) => {
                    if let Some(ref mut map) = track {
                        *map.entry(f.technique.to_string()).or_insert(0) += 1;
                    }
                    if f.technique > max_technique {
                        max_technique = f.technique;
                    }
                    apply_finding(grid, &f);
                }
                None => {
                    // No technique found, use backtracking to finish
                    backtrack::solve_recursive(grid);
                    if let Some(ref mut map) = track {
                        *map.entry(Technique::Backtracking.to_string()).or_insert(0) += 1;
                    }
                    return Technique::Backtracking;
                }
            }
        }

        max_technique
    }

    /// Map a technique + puzzle characteristics to a difficulty level.
    #[allow(deprecated)]
    fn technique_to_difficulty(tech: Technique, empty_count: usize) -> Difficulty {
        match tech {
            Technique::NakedSingle => {
                if empty_count <= 35 {
                    Difficulty::Beginner
                } else {
                    Difficulty::Easy
                }
            }
            Technique::HiddenSingle => Difficulty::Medium,
            Technique::NakedPair
            | Technique::HiddenPair
            | Technique::NakedTriple
            | Technique::HiddenTriple => Difficulty::Intermediate,
            Technique::PointingPair | Technique::BoxLineReduction => Difficulty::Hard,
            Technique::XWing
            | Technique::FinnedXWing
            | Technique::Swordfish
            | Technique::FinnedSwordfish
            | Technique::Jellyfish
            | Technique::FinnedJellyfish
            | Technique::NakedQuad
            | Technique::HiddenQuad
            | Technique::EmptyRectangle
            | Technique::AvoidableRectangle
            | Technique::UniqueRectangle
            | Technique::HiddenRectangle => Difficulty::Expert,
            Technique::XYWing
            | Technique::XYZWing
            | Technique::WXYZWing
            | Technique::WWing
            | Technique::XChain
            | Technique::ThreeDMedusa
            | Technique::SueDeCoq
            | Technique::AIC
            | Technique::FrankenFish
            | Technique::SiameseFish
            | Technique::AlsXz
            | Technique::ExtendedUniqueRectangle
            | Technique::BivalueUniversalGrave => Difficulty::Master,
            Technique::AlsXyWing
            | Technique::AlsChain
            | Technique::MutantFish
            | Technique::AlignedPairExclusion
            | Technique::AlignedTripletExclusion
            | Technique::DeathBlossom
            | Technique::ArithmeticCounting
            | Technique::NishioForcingChain
            | Technique::KrakenFish
            | Technique::RegionForcingChain
            | Technique::CellForcingChain
            | Technique::DynamicForcingChain
            | Technique::Backtracking => Difficulty::Extreme,
        }
    }
}

/// Apply a Finding to a mutable Grid.
fn apply_finding(grid: &mut Grid, finding: &Finding) {
    match &finding.inference {
        InferenceResult::Placement { cell, value } => {
            let pos = idx_to_pos(*cell);
            grid.set_cell_unchecked(pos, Some(*value));
            grid.recalculate_candidates();
        }
        InferenceResult::Elimination { cell, values } => {
            let pos = idx_to_pos(*cell);
            for &v in values {
                grid.cell_mut(pos).remove_candidate(v);
            }
        }
    }
}

/// Propagate using the established technique set (for Dynamic Forcing Chains).
///
/// Makes an assumption (set cell value), then loops applying all techniques
/// except forcing chains (to avoid infinite recursion) until no more progress.
/// Arithmetic search is also omitted deliberately: its bounded search cost
/// should not be repeated inside every forcing-chain assumption.
fn propagate_full(grid: &Grid, pos: Position, val: u8) -> (Grid, bool) {
    let mut g = grid.deep_clone();
    g.set_cell_unchecked(pos, Some(val));
    g.recalculate_candidates();

    for _ in 0..200 {
        if backtrack::has_contradiction(&g) {
            return (g, true);
        }
        if g.is_complete() {
            return (g, false);
        }

        let fab = CandidateFabric::from_grid(&g);

        // Try all techniques except forcing chains (avoids infinite recursion)
        let finding = None
            .or_else(|| basic::find_naked_single(&fab))
            .or_else(|| basic::find_hidden_single(&fab))
            .or_else(|| basic::find_naked_subset(&fab, 2))
            .or_else(|| basic::find_hidden_subset(&fab, 2))
            .or_else(|| basic::find_naked_subset(&fab, 3))
            .or_else(|| basic::find_hidden_subset(&fab, 3))
            .or_else(|| fish_engine::find_pointing_pair(&fab))
            .or_else(|| fish_engine::find_box_line_reduction(&fab))
            .or_else(|| fish_engine::find_basic_fish(&fab, 2))
            .or_else(|| fish_engine::find_finned_fish(&fab, 2))
            .or_else(|| fish_engine::find_basic_fish(&fab, 3))
            .or_else(|| fish_engine::find_finned_fish(&fab, 3))
            .or_else(|| fish_engine::find_basic_fish(&fab, 4))
            .or_else(|| fish_engine::find_finned_fish(&fab, 4))
            .or_else(|| basic::find_naked_subset(&fab, 4))
            .or_else(|| basic::find_hidden_subset(&fab, 4))
            .or_else(|| aic_engine::find_empty_rectangle(&fab))
            .or_else(|| uniqueness::find_avoidable_rectangle(&fab))
            .or_else(|| uniqueness::find_unique_rectangle(&fab))
            .or_else(|| uniqueness::find_hidden_rectangle(&fab))
            .or_else(|| als_engine::find_xy_wing(&fab))
            .or_else(|| als_engine::find_xyz_wing(&fab))
            .or_else(|| als_engine::find_wxyz_wing(&fab))
            .or_else(|| aic_engine::find_w_wing(&fab))
            .or_else(|| {
                let graph = aic_engine::build_link_graph(&fab);
                None.or_else(|| aic_engine::find_x_chain(&fab, &graph))
                    // Legacy: retained for SE compatibility, subsumed by AIC
                    .or_else(|| {
                        #[allow(deprecated)]
                        aic_engine::find_medusa(&fab, &graph)
                    })
                    .or_else(|| als_engine::find_sue_de_coq(&fab))
                    .or_else(|| aic_engine::find_aic(&fab, &graph))
            })
            .or_else(|| fish_engine::find_franken_fish(&fab))
            .or_else(|| fish_engine::find_siamese_fish(&fab))
            .or_else(|| als_engine::find_als_xz(&fab))
            .or_else(|| uniqueness::find_extended_unique_rectangle(&fab))
            .or_else(|| uniqueness::find_bug(&fab))
            .or_else(|| als_engine::find_als_xy_wing(&fab))
            .or_else(|| als_engine::find_als_chain(&fab))
            .or_else(|| fish_engine::find_mutant_fish(&fab))
            // Legacy: retained for SE compatibility, subsumed by ALS chains
            .or_else(|| {
                #[allow(deprecated)]
                als_engine::find_aligned_pair_exclusion(&fab)
            })
            // Legacy: retained for SE compatibility, subsumed by ALS chains
            .or_else(|| {
                #[allow(deprecated)]
                als_engine::find_aligned_triplet_exclusion(&fab)
            })
            .or_else(|| als_engine::find_death_blossom(&fab));
        // Note: forcing chains excluded to avoid infinite recursion

        match finding {
            Some(f) => apply_finding(&mut g, &f),
            None => break,
        }
    }

    let contradiction = backtrack::has_contradiction(&g);
    (g, contradiction)
}

#[cfg(test)]
#[path = "mod_tests.rs"]
mod tests;

#[cfg(test)]
mod technique_tests;

#[cfg(test)]
mod behavior_tests;
