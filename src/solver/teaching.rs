//! Versioned, read-only mobile teaching API. Technique priority belongs to the
//! caller; this dispatcher never invokes the globally preferred hint or guesses.
use super::explain::{
    ExplanationData, Finding, InferenceResult, LinkType, Polarity, ProofCertificate,
};
use super::fabric::{sector_cells, CandidateFabric};
use super::teaching_contract::{
    TeachingBranch, TeachingDeduction, TeachingHint, TeachingProof, TeachingSearchResult,
};
use super::teaching_identity::hint_key;
use super::teaching_proof::adapt;
use super::{aic_engine, als_engine, basic, fish_engine, uniqueness, Technique};
use crate::{Grid, Position};

pub const TEACHING_TECHNIQUES: &[&str] = &[
    "naked_single",
    "hidden_single",
    "locked_candidates",
    "naked_pair",
    "x_wing",
    "xy_wing",
    "wxyz_wing",
    "aic",
    "hidden_pair",
    "naked_triple",
    "hidden_triple",
    "naked_quad",
    "hidden_quad",
    "finned_x_wing",
    "swordfish",
    "finned_swordfish",
    "jellyfish",
    "finned_jellyfish",
    "franken_fish",
    "siamese_fish",
    "mutant_fish",
    "empty_rectangle",
    "avoidable_rectangle",
    "unique_rectangle",
    "hidden_rectangle",
    "extended_unique_rectangle",
    "xyz_wing",
    "w_wing",
    "x_chain",
    "sue_de_coq",
    "als_xz",
    "als_xy_wing",
    "als_chain",
    "death_blossom",
    "bug",
    "nishio_forcing_chain",
    "kraken_fish",
    "region_forcing_chain",
    "cell_forcing_chain",
    "dynamic_forcing_chain",
];

fn result(status: &str, work_used: u32) -> TeachingSearchResult {
    TeachingSearchResult {
        status: status.into(),
        hints: vec![],
        work_used,
    }
}

/// Searches exactly one technique on a fresh grid preserving supplied trusted
/// exclusions. Masks use app bits 0..8, unlike the core BitSet's bits 1..9.
pub fn search_teaching_hint(
    puzzle: &str,
    values: &str,
    exclusions: &[u16],
    technique: &str,
    work_limit: u32,
) -> Result<TeachingSearchResult, String> {
    search_teaching_hint_with_options(
        puzzle,
        values,
        exclusions,
        technique,
        work_limit,
        TEACHING_TECHNIQUES,
    )
}

pub fn search_teaching_hint_with_options(
    puzzle: &str,
    values: &str,
    exclusions: &[u16],
    technique: &str,
    work_limit: u32,
    allowed_techniques: &[&str],
) -> Result<TeachingSearchResult, String> {
    if puzzle.len() != 81
        || values.len() != 81
        || !puzzle.bytes().all(|b| b.is_ascii_digit())
        || !values.bytes().all(|b| b.is_ascii_digit())
        || exclusions.len() != 81
        || exclusions.iter().any(|&m| m > 511)
        || work_limit == 0
        || work_limit > 2_000_000
        || allowed_techniques
            .iter()
            .any(|id| !TEACHING_TECHNIQUES.contains(id))
    {
        return Err("Invalid hint request".into());
    }
    if !TEACHING_TECHNIQUES.contains(&technique) || !allowed_techniques.contains(&technique) {
        return Ok(result("unsupported", 0));
    }
    let budget = super::search_budget::RequestBudget::enter(work_limit);
    let mut grid = Grid::from_string(puzzle).ok_or("Invalid original puzzle")?;
    for (cell, byte) in values.bytes().enumerate() {
        let given = puzzle.as_bytes()[cell];
        if given != b'0' && given != byte {
            return Err("Original clues cannot change".into());
        }
        if given == b'0' {
            grid.set_cell_unchecked(
                Position::new(cell / 9, cell % 9),
                if byte == b'0' {
                    None
                } else {
                    Some(byte - b'0')
                },
            );
        }
    }
    if !grid.validate().is_valid {
        return Ok(result("contradictory", 0));
    }
    grid.recalculate_candidates();
    for (cell, &mask) in exclusions.iter().enumerate() {
        let pos = Position::new(cell / 9, cell % 9);
        if grid.get(pos).is_some() {
            if mask != 0 {
                return Err("Filled cells cannot have exclusions".into());
            }
        } else {
            for digit in 1..=9 {
                if mask & (1 << (digit - 1)) != 0 {
                    grid.cell_mut(pos).remove_candidate(digit);
                }
            }
            if grid.get_candidates(pos).is_empty() {
                return Ok(result("contradictory", 0));
            }
        }
    }
    let fab = CandidateFabric::from_grid(&grid);
    for sector in 0..27 {
        for digit in 1..=9 {
            if !sector_cells(sector)
                .iter()
                .any(|&c| fab.values[c] == Some(digit))
                && fab.sector_cand_count(sector, digit) == 0
            {
                return Ok(result("contradictory", 0));
            }
        }
    }
    let unique = if requires_uniqueness(technique)
        || (is_forcing(technique) && allowed_techniques.iter().any(|id| requires_uniqueness(id)))
    {
        let count = count_original_solutions(puzzle);
        let (used, exhausted) = budget.usage();
        if exhausted {
            return Ok(result("budget_exhausted", used));
        }
        if requires_uniqueness(technique) && count != 1 {
            return Ok(result(
                if count == 0 {
                    "contradictory"
                } else {
                    "non_unique_puzzle"
                },
                used,
            ));
        }
        count == 1
    } else {
        false
    };
    let mut branches = Vec::new();
    let finding = if is_forcing(technique) {
        let (finding, traces) =
            super::teaching_forcing::find(&grid, technique, allowed_techniques, unique)?;
        branches = traces;
        finding
    } else {
        find_direct(&fab, technique)
    };
    let (work, exhausted) = budget.usage();
    if exhausted {
        return Ok(result("budget_exhausted", work));
    }
    let Some(finding) = finding else {
        return Ok(result("not_found", work));
    };
    let position_key = format!(
        "{puzzle}:{values}:{}",
        exclusions
            .iter()
            .map(u16::to_string)
            .collect::<Vec<_>>()
            .join(",")
    );
    let deduction = adapt(&finding, &fab, technique)?;
    let hint = build_hint(&finding, deduction, position_key, branches);
    Ok(TeachingSearchResult {
        status: "found".into(),
        work_used: work,
        hints: vec![hint],
    })
}

fn build_hint(
    finding: &Finding,
    deduction: TeachingDeduction,
    position_key: String,
    branches: Vec<TeachingBranch>,
) -> TeachingHint {
    let key = hint_key(&position_key, &deduction, &branches);
    let TeachingDeduction {
        technique_id,
        source_cells,
        mut candidates,
        placements,
        eliminations,
        houses,
        proof: evidence,
    } = deduction;
    let variant = match (&finding.proof, &finding.explanation) {
        (Some(ProofCertificate::Aic { .. }), ExplanationData::Chain { variant, .. }) => {
            variant.clone()
        }
        _ => finding.technique.to_string(),
    };
    if !branches.is_empty() {
        candidates.extend(branches.iter().map(|branch| branch.assumption.clone()));
        candidates.sort_by_key(|candidate| (candidate.cell, candidate.digit));
        candidates.dedup();
    }
    TeachingHint {
        schema_version: 2,
        technique_id,
        variant,
        position_key,
        key,
        source_cells,
        candidates,
        placements,
        eliminations,
        houses,
        proof: TeachingProof { evidence, branches },
        explanation: finding.to_hint().explanation,
    }
}

pub(super) fn find_direct(fab: &CandidateFabric, technique: &str) -> Option<Finding> {
    match technique {
        "naked_single" => basic::find_naked_single(fab),
        "hidden_single" => basic::find_hidden_single(fab),
        "locked_candidates" => fish_engine::find_pointing_pair(fab)
            .or_else(|| fish_engine::find_box_line_reduction(fab)),
        "naked_pair" => basic::find_naked_subset(fab, 2),
        "hidden_pair" => basic::find_hidden_subset(fab, 2),
        "naked_triple" => basic::find_naked_subset(fab, 3),
        "hidden_triple" => basic::find_hidden_subset(fab, 3),
        "naked_quad" => basic::find_naked_subset(fab, 4),
        "hidden_quad" => basic::find_hidden_subset(fab, 4),
        "x_wing" | "finned_x_wing" | "swordfish" | "finned_swordfish" | "jellyfish"
        | "finned_jellyfish" | "franken_fish" | "mutant_fish" => {
            fish_engine::find_teaching_fish(fab, technique)
        }
        "siamese_fish" => fish_engine::find_siamese_fish(fab),
        "xy_wing" => als_engine::find_wing_bounded(fab, Technique::XYWing, u32::MAX).0,
        "xyz_wing" => als_engine::find_wing_bounded(fab, Technique::XYZWing, u32::MAX).0,
        "wxyz_wing" => als_engine::find_wing_bounded(fab, Technique::WXYZWing, u32::MAX).0,
        "als_xz" => als_engine::find_als_xz(fab),
        "als_xy_wing" => als_engine::find_als_xy_wing_teaching(fab),
        "als_chain" => als_engine::find_als_chain_teaching(fab),
        "sue_de_coq" => als_engine::find_sue_de_coq(fab),
        "death_blossom" => als_engine::find_death_blossom(fab),
        "empty_rectangle" => aic_engine::find_empty_rectangle(fab),
        "w_wing" => {
            let mut finding = aic_engine::find_w_wing(fab)?;
            let cells = &finding.involved_cells;
            let InferenceResult::Elimination { values, .. } = &finding.inference else {
                return None;
            };
            let other = values[0];
            let link = fab.cell_cands[cells[0]].iter().find(|&d| d != other)?;
            let (a, b) = if fab.sees(cells[0], cells[2]) {
                (cells[2], cells[3])
            } else {
                (cells[3], cells[2])
            };
            finding.proof = Some(ProofCertificate::Aic {
                chain: vec![
                    (cells[0], other, Polarity::Off),
                    (cells[0], link, Polarity::On),
                    (a, link, Polarity::Off),
                    (b, link, Polarity::On),
                    (cells[1], link, Polarity::Off),
                    (cells[1], other, Polarity::On),
                ],
                link_types: vec![
                    LinkType::Strong,
                    LinkType::WeakInference,
                    LinkType::Strong,
                    LinkType::WeakInference,
                    LinkType::Strong,
                ],
            });
            Some(finding)
        }
        "x_chain" => aic_engine::find_x_chain(fab, &aic_engine::build_link_graph(fab)),
        "aic" => aic_engine::find_aic_bounded(fab, u32::MAX).0,
        "avoidable_rectangle" => uniqueness::find_avoidable_rectangle(fab),
        "unique_rectangle" => uniqueness::find_unique_rectangle(fab),
        "hidden_rectangle" => uniqueness::find_hidden_rectangle(fab),
        "extended_unique_rectangle" => uniqueness::find_extended_unique_rectangle(fab),
        "bug" => uniqueness::find_bug(fab),
        _ => None,
    }
}

pub(super) fn is_forcing(id: &str) -> bool {
    matches!(
        id,
        "nishio_forcing_chain"
            | "kraken_fish"
            | "cell_forcing_chain"
            | "region_forcing_chain"
            | "dynamic_forcing_chain"
    )
}
pub(super) fn requires_uniqueness(id: &str) -> bool {
    matches!(
        id,
        "avoidable_rectangle"
            | "unique_rectangle"
            | "hidden_rectangle"
            | "extended_unique_rectangle"
            | "bug"
    )
}

// Bounded exact solution counting verifies an assumption of uniqueness. It is
// never used to select a hint, fill the board, or manufacture a deduction.
fn count_original_solutions(puzzle: &str) -> usize {
    fn count(values: &mut [u8; 81]) -> usize {
        if !super::search_budget::tick() {
            return 0;
        }
        let mut best = None;
        let mut best_mask = 0u16;
        for cell in 0..81 {
            if !super::search_budget::tick() {
                return 0;
            }
            if values[cell] != 0 {
                continue;
            }
            let mut mask = 511u16;
            for peer in 0..81 {
                if !super::search_budget::tick() {
                    return 0;
                }
                if values[peer] == 0 {
                    continue;
                }
                if cell / 9 == peer / 9
                    || cell % 9 == peer % 9
                    || (cell / 27 == peer / 27 && cell % 9 / 3 == peer % 9 / 3)
                {
                    mask &= !(1 << (values[peer] - 1));
                }
            }
            if mask == 0 {
                return 0;
            }
            if best.is_none() || mask.count_ones() < best_mask.count_ones() {
                best = Some(cell);
                best_mask = mask;
            }
        }
        let Some(cell) = best else {
            return 1;
        };
        let mut total = 0;
        for digit in 1..=9 {
            if best_mask & (1 << (digit - 1)) != 0 {
                values[cell] = digit;
                total += count(values);
                values[cell] = 0;
                if total >= 2 || !super::search_budget::tick() {
                    break;
                }
            }
        }
        total.min(2)
    }
    let mut values = [0u8; 81];
    for (cell, digit) in puzzle.bytes().enumerate() {
        values[cell] = digit - b'0';
    }
    count(&mut values)
}
