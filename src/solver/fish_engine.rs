//! Unified Fish Engine: ONE `search_fish()` parameterized by SectorConstraint.
//!
//! Replaces: find_x_wing, find_swordfish, find_jellyfish, find_finned_fish_generic,
//! find_franken_fish, find_siamese_fish, find_mutant_fish.
//!
//! Core idea: For each digit, enumerate base-sector combos of size n, find cover-sector
//! combos of size n. Fins = base_cells \ cover_cells. Classification by
//! (size, has_fins, sector_types) -> Technique variant.

use super::explain::{ExplanationData, Finding, InferenceResult, ProofCertificate};
use super::fabric::{
    sector_cells, CandidateFabric, SECTOR_BOX_BASE, SECTOR_COL_BASE, SECTOR_ROW_BASE,
};
use super::types::Technique;

/// Controls which sector types are allowed in base/cover sets.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum SectorConstraint {
    /// Rows as bases, cols as covers (or vice versa)
    Basic,
    /// Basic + boxes allowed in cover sets (Franken)
    Franken,
    /// All three sector types in both base and cover (Mutant)
    Mutant,
}

fn sector_name(sector: usize) -> String {
    if sector < 9 {
        format!("r{}", sector + 1)
    } else if sector < 18 {
        format!("c{}", sector - 9 + 1)
    } else {
        format!("b{}", sector - 18 + 1)
    }
}

fn sector_type(sector: usize) -> u8 {
    if sector < 9 {
        0
    }
    // row
    else if sector < 18 {
        1
    }
    // col
    else {
        2
    } // box
}

/// Get a bitmask of cells (linear indices) that are in a given sector and have a candidate.
fn sector_candidate_mask(fab: &CandidateFabric, sector: usize, digit: u8) -> u128 {
    let cells = sector_cells(sector);
    let mut mask = 0u128;
    for &c in &cells {
        if fab.values[c].is_none() && fab.cell_cands[c].contains(digit) {
            mask |= 1u128 << c;
        }
    }
    mask
}

/// Visit independent sector combinations in lexicographic order. Overlapping
/// partial groups cannot become independent by adding sectors, so reject them
/// before enumerating the remaining choices. Reuse one prefix allocation.
fn find_independent_combination(
    pool: &[usize],
    masks: &[u128; 27],
    size: usize,
    mut visit: impl FnMut(&[usize], u128) -> Option<Finding>,
) -> Option<Finding> {
    fn search(
        pool: &[usize],
        masks: &[u128; 27],
        start: usize,
        remaining: usize,
        prefix: &mut Vec<usize>,
        cells: u128,
        visit: &mut impl FnMut(&[usize], u128) -> Option<Finding>,
    ) -> Option<Finding> {
        if !super::search_budget::tick() {
            return None;
        }
        if remaining == 0 {
            return visit(prefix, cells);
        }
        for index in start..=pool.len() - remaining {
            if !super::search_budget::tick() {
                return None;
            }
            let sector = pool[index];
            let candidates = masks[sector];
            // One placement in an overlap could satisfy two groups, invalidating
            // the rank argument. This is the same independence check for both
            // base sectors and cover sectors.
            if cells & candidates != 0 {
                continue;
            }
            prefix.push(sector);
            let finding = search(
                pool,
                masks,
                index + 1,
                remaining - 1,
                prefix,
                cells | candidates,
                visit,
            );
            prefix.pop();
            if finding.is_some() {
                return finding;
            }
        }
        None
    }
    if size == 0 || size > pool.len() {
        return None;
    }
    search(
        pool,
        masks,
        0,
        size,
        &mut Vec::with_capacity(size),
        0,
        &mut visit,
    )
}

/// Return the common box only when the nonempty fin set lies wholly inside it.
fn fin_box(fins: u128, masks: &[u128; 27]) -> Option<usize> {
    if fins == 0 {
        return None;
    }
    let cell = fins.trailing_zeros() as usize;
    let box_idx = (cell / 27) * 3 + (cell % 9) / 3;
    (fins & !masks[SECTOR_BOX_BASE + box_idx] == 0).then_some(box_idx)
}

/// Search for fish patterns of a given size, sector constraint, and digit.
fn search_fish_for_digit(
    fab: &CandidateFabric,
    digit: u8,
    size: usize,
    constraint: SectorConstraint,
) -> Option<Finding> {
    search_fish_for_digit_filtered(fab, digit, size, constraint, None)
}

pub(crate) fn find_teaching_fish(fab: &CandidateFabric, id: &str) -> Option<Finding> {
    let (technique, size, constraint) = match id {
        "x_wing" => (Technique::XWing, 2, SectorConstraint::Basic),
        "finned_x_wing" => (Technique::FinnedXWing, 2, SectorConstraint::Basic),
        "swordfish" => (Technique::Swordfish, 3, SectorConstraint::Basic),
        "finned_swordfish" => (Technique::FinnedSwordfish, 3, SectorConstraint::Basic),
        "jellyfish" => (Technique::Jellyfish, 4, SectorConstraint::Basic),
        "finned_jellyfish" => (Technique::FinnedJellyfish, 4, SectorConstraint::Basic),
        "franken_fish" => (Technique::FrankenFish, 0, SectorConstraint::Franken),
        "mutant_fish" => (Technique::MutantFish, 0, SectorConstraint::Mutant),
        _ => return None,
    };
    let mut digits: Vec<u8> = (1..=9).collect();
    // Search the most constrained digit first. The digit itself breaks ties,
    // independent of map seeds; sparse fish need not wait behind dense digits.
    digits.sort_by_key(|&d| {
        (
            (0..81).filter(|&c| fab.cell_cands[c].contains(d)).count(),
            d,
        )
    });
    for digit in digits {
        for n in 2..=4 {
            if !super::search_budget::tick() {
                return None;
            }
            if size != 0 && n != size {
                continue;
            }
            if let Some(f) =
                search_fish_for_digit_filtered(fab, digit, n, constraint, Some(technique))
            {
                return Some(f);
            }
        }
    }
    None
}

fn search_fish_for_digit_filtered(
    fab: &CandidateFabric,
    digit: u8,
    size: usize,
    constraint: SectorConstraint,
    wanted: Option<Technique>,
) -> Option<Finding> {
    let masks = std::array::from_fn(|sector| sector_candidate_mask(fab, sector, digit));
    // Determine which sectors can be bases and covers
    let (base_sectors, cover_sectors) = match constraint {
        SectorConstraint::Basic => {
            // Row bases -> col covers, then col bases -> row covers
            let rows: Vec<usize> = (SECTOR_ROW_BASE..SECTOR_ROW_BASE + 9).collect();
            let cols: Vec<usize> = (SECTOR_COL_BASE..SECTOR_COL_BASE + 9).collect();
            // Try both orientations
            return search_fish_oriented(
                fab, &masks, digit, size, &rows, &cols, constraint, wanted,
            )
            .or_else(|| {
                search_fish_oriented(fab, &masks, digit, size, &cols, &rows, constraint, wanted)
            });
        }
        SectorConstraint::Franken => {
            // Lines as bases, lines+boxes as covers
            let lines: Vec<usize> = (0..18).collect();
            let all: Vec<usize> = (0..27).collect();
            return search_fish_oriented(
                fab, &masks, digit, size, &lines, &all, constraint, wanted,
            );
        }
        SectorConstraint::Mutant => {
            let all: Vec<usize> = (0..27).collect();
            (all.clone(), all)
        }
    };

    search_fish_oriented(
        fab,
        &masks,
        digit,
        size,
        &base_sectors,
        &cover_sectors,
        constraint,
        wanted,
    )
}

fn search_fish_oriented(
    fab: &CandidateFabric,
    masks: &[u128; 27],
    digit: u8,
    size: usize,
    base_pool: &[usize],
    cover_pool: &[usize],
    constraint: SectorConstraint,
    wanted: Option<Technique>,
) -> Option<Finding> {
    let eligible_bases: Vec<usize> = base_pool
        .iter()
        .filter(|&&s| fab.sector_cand_count(s, digit) >= 2)
        .copied()
        .collect();

    find_independent_combination(&eligible_bases, masks, size, |base_combo, base_cells| {
        if base_cells.count_ones() < size as u32 {
            return None;
        }
        let eligible_covers: Vec<usize> = cover_pool
            .iter()
            .filter(|&&s| !base_combo.contains(&s) && masks[s] & base_cells != 0)
            .copied()
            .collect();

        find_independent_combination(&eligible_covers, masks, size, |cover_combo, cover_cells| {
            if !validate_fish_types(base_combo, cover_combo, constraint) {
                return None;
            }
            let fins = base_cells & !cover_cells;
            if wanted.is_some_and(|t| t != classify_fish(size, fins != 0, constraint)) {
                return None;
            }
            let eliminations = cover_cells & !base_cells;
            if fins == 0 {
                make_fish_finding(
                    fab,
                    digit,
                    size,
                    base_combo,
                    cover_combo,
                    &[],
                    eliminations,
                    constraint,
                )
            } else {
                let box_idx = fin_box(fins, masks)?;
                let restricted_elims = eliminations & masks[SECTOR_BOX_BASE + box_idx];
                if restricted_elims == 0 {
                    return None;
                }
                // Materialize evidence only once a finned pattern can eliminate.
                let fin_cells: Vec<usize> = (0..81).filter(|&i| fins & (1u128 << i) != 0).collect();
                make_fish_finding(
                    fab,
                    digit,
                    size,
                    base_combo,
                    cover_combo,
                    &fin_cells,
                    restricted_elims,
                    constraint,
                )
            }
        })
    })
}

fn validate_fish_types(bases: &[usize], covers: &[usize], constraint: SectorConstraint) -> bool {
    match constraint {
        SectorConstraint::Basic => {
            // All bases same type, all covers different type
            let bt = sector_type(bases[0]);
            let ct = sector_type(covers[0]);
            bt != ct
                && bt != 2 && ct != 2  // No boxes in basic
                && bases.iter().all(|&s| sector_type(s) == bt)
                && covers.iter().all(|&s| sector_type(s) == ct)
        }
        SectorConstraint::Franken => {
            // Bases are lines (row or col), covers can include boxes
            // At least one base and one cover must be different types
            let has_box_cover = covers.iter().any(|&s| sector_type(s) == 2);
            let bases_are_lines = bases.iter().all(|&s| sector_type(s) != 2);
            bases_are_lines && has_box_cover
        }
        SectorConstraint::Mutant => {
            // At least 3 different sector types across all bases+covers
            let types = bases
                .iter()
                .chain(covers.iter())
                .fold(0u8, |types, &sector| types | (1 << sector_type(sector)));
            types.count_ones() >= 3 || (types.count_ones() >= 2 && types & (1 << 2) != 0)
        }
    }
}

fn classify_fish(size: usize, has_fins: bool, constraint: SectorConstraint) -> Technique {
    match (constraint, size, has_fins) {
        (SectorConstraint::Basic, 2, false) => Technique::XWing,
        (SectorConstraint::Basic, 2, true) => Technique::FinnedXWing,
        (SectorConstraint::Basic, 3, false) => Technique::Swordfish,
        (SectorConstraint::Basic, 3, true) => Technique::FinnedSwordfish,
        (SectorConstraint::Basic, 4, false) => Technique::Jellyfish,
        (SectorConstraint::Basic, 4, true) => Technique::FinnedJellyfish,
        (SectorConstraint::Franken, _, _) => Technique::FrankenFish,
        (SectorConstraint::Mutant, _, _) => Technique::MutantFish,
        _ => Technique::Jellyfish, // fallback
    }
}

#[allow(clippy::too_many_arguments)]
fn make_fish_finding(
    fab: &CandidateFabric,
    digit: u8,
    size: usize,
    bases: &[usize],
    covers: &[usize],
    fins: &[usize],
    elim_mask: u128,
    constraint: SectorConstraint,
) -> Option<Finding> {
    if elim_mask == 0 {
        return None;
    }

    // Find the first elimination cell
    for cell in 0..81 {
        if !super::search_budget::tick() {
            return None;
        }
        if elim_mask & (1u128 << cell) != 0
            && fab.values[cell].is_none()
            && fab.cell_cands[cell].contains(digit)
        {
            // Don't eliminate from cells that are in base sectors
            let in_base = bases.iter().any(|&s| {
                let sc = sector_cells(s);
                sc.contains(&cell)
            });
            if in_base {
                continue;
            }

            let technique = classify_fish(size, !fins.is_empty(), constraint);
            let variant: String = match constraint {
                SectorConstraint::Basic => {
                    if fins.is_empty() {
                        "Basic".into()
                    } else {
                        "Finned".into()
                    }
                }
                SectorConstraint::Franken => "Franken".into(),
                SectorConstraint::Mutant => "Mutant".into(),
            };

            let base_names: Vec<String> = bases.iter().map(|&s| sector_name(s)).collect();
            let cover_names: Vec<String> = covers.iter().map(|&s| sector_name(s)).collect();

            // Collect involved cells (all candidates for this digit in base sectors)
            let mut involved = Vec::new();
            for &bs in bases {
                if !super::search_budget::tick() {
                    return None;
                }
                for &c in &sector_cells(bs) {
                    if !super::search_budget::tick() {
                        return None;
                    }
                    if fab.values[c].is_none()
                        && fab.cell_cands[c].contains(digit)
                        && !involved.contains(&c)
                    {
                        involved.push(c);
                    }
                }
            }

            return Some(Finding {
                technique,
                inference: InferenceResult::Elimination {
                    cell,
                    values: vec![digit],
                },
                involved_cells: involved,
                explanation: ExplanationData::Fish {
                    size,
                    digit,
                    base_sectors: base_names,
                    cover_sectors: cover_names,
                    fins: fins.to_vec(),
                    variant,
                },
                proof: Some(ProofCertificate::Fish {
                    digit,
                    base_sectors: bases.to_vec(),
                    cover_sectors: covers.to_vec(),
                    fins: fins.to_vec(),
                }),
            });
        }
    }
    None
}

// ==================== Siamese Fish ====================

struct SectorCombination {
    sectors: Vec<usize>,
    cells: u128,
    sector_bits: u32,
}

fn masked_combinations(pool: &[usize], size: usize, masks: &[u128; 27]) -> Vec<SectorCombination> {
    combinations(pool, size)
        .into_iter()
        .map(|sectors| {
            let cells = sectors
                .iter()
                .fold(0, |cells, &sector| cells | masks[sector]);
            let sector_bits = sectors.iter().fold(0, |bits, &sector| bits | (1 << sector));
            SectorCombination {
                sectors,
                cells,
                sector_bits,
            }
        })
        .collect()
}

/// Siamese Fish: two overlapping fish patterns that share fins.
pub fn find_siamese_fish(fab: &CandidateFabric) -> Option<Finding> {
    // Siamese fish are detected as finned fish where two separate base combos
    // produce fin cells in the same box. We search for finned fish with an
    // additional constraint that there's a second valid fish pattern sharing the fin.
    for digit in 1..=9u8 {
        if !super::search_budget::tick() {
            return None;
        }
        let masks = std::array::from_fn(|sector| sector_candidate_mask(fab, sector, digit));
        for size in 2..=4usize {
            if !super::search_budget::tick() {
                return None;
            }
            // Get all row-based fish patterns with fins
            let rows: Vec<usize> = (SECTOR_ROW_BASE..SECTOR_ROW_BASE + 9)
                .filter(|&s| fab.sector_cand_count(s, digit) >= 2)
                .collect();
            let cols: Vec<usize> = (SECTOR_COL_BASE..SECTOR_COL_BASE + 9).collect();

            if rows.len() < size + 1 {
                continue;
            }

            // Find pairs of base combos that produce fins in the same box
            let all_combos = masked_combinations(&rows, size, &masks);
            let col_combos = masked_combinations(&cols, size, &masks);
            for i in 0..all_combos.len() {
                if !super::search_budget::tick() {
                    return None;
                }
                for j in (i + 1)..all_combos.len() {
                    if !super::search_budget::tick() {
                        return None;
                    }
                    let combo_a = &all_combos[i];
                    let combo_b = &all_combos[j];

                    // Check if these produce valid finned fish with shared fin box
                    if let Some(finding) =
                        check_siamese_pair(fab, &masks, digit, size, combo_a, combo_b, &col_combos)
                    {
                        return Some(finding);
                    }
                }
            }

            // Column-based
            let eligible_cols: Vec<usize> = (SECTOR_COL_BASE..SECTOR_COL_BASE + 9)
                .filter(|&s| fab.sector_cand_count(s, digit) >= 2)
                .collect();
            let row_covers: Vec<usize> = (SECTOR_ROW_BASE..SECTOR_ROW_BASE + 9).collect();

            if eligible_cols.len() < size + 1 {
                continue;
            }

            let all_col_combos = masked_combinations(&eligible_cols, size, &masks);
            let row_combos = masked_combinations(&row_covers, size, &masks);
            for i in 0..all_col_combos.len() {
                if !super::search_budget::tick() {
                    return None;
                }
                for j in (i + 1)..all_col_combos.len() {
                    if !super::search_budget::tick() {
                        return None;
                    }
                    if let Some(finding) = check_siamese_pair(
                        fab,
                        &masks,
                        digit,
                        size,
                        &all_col_combos[i],
                        &all_col_combos[j],
                        &row_combos,
                    ) {
                        return Some(finding);
                    }
                }
            }
        }
    }
    None
}

fn check_siamese_pair(
    fab: &CandidateFabric,
    masks: &[u128; 27],
    digit: u8,
    size: usize,
    base_a: &SectorCombination,
    base_b: &SectorCombination,
    cover_combos: &[SectorCombination],
) -> Option<Finding> {
    // Both combos must share at least one base sector (overlapping).
    let shared = (base_a.sector_bits & base_b.sector_bits).count_ones() as usize;
    if shared == 0 || shared == size {
        return None;
    }
    let combo_a = &base_a.sectors;
    let combo_b = &base_b.sectors;
    for cover in cover_combos {
        if !super::search_budget::tick() {
            return None;
        }
        let fins_a = base_a.cells & !cover.cells;
        let fins_b = base_b.cells & !cover.cells;
        if fins_a == 0 || fins_b == 0 {
            continue;
        }
        let Some(box_idx) = fin_box(fins_a | fins_b, masks) else {
            continue;
        };
        let combined =
            cover.cells & !base_a.cells & !base_b.cells & masks[SECTOR_BOX_BASE + box_idx];
        if combined == 0 {
            continue;
        }
        let cover_combo = &cover.sectors;
        let fin_cells_a: Vec<usize> = (0..81).filter(|&i| fins_a & (1u128 << i) != 0).collect();
        let fin_cells_b: Vec<usize> = (0..81).filter(|&i| fins_b & (1u128 << i) != 0).collect();

        for cell in 0..81 {
            if !super::search_budget::tick() {
                return None;
            }
            if combined & (1u128 << cell) != 0
                && fab.values[cell].is_none()
                && fab.cell_cands[cell].contains(digit)
            {
                let mut involved = Vec::new();
                for &bs in combo_a.iter().chain(combo_b.iter()) {
                    if !super::search_budget::tick() {
                        return None;
                    }
                    for &c in &sector_cells(bs) {
                        if !super::search_budget::tick() {
                            return None;
                        }
                        if fab.values[c].is_none()
                            && fab.cell_cands[c].contains(digit)
                            && !involved.contains(&c)
                        {
                            involved.push(c);
                        }
                    }
                }
                return Some(Finding {
                    technique: Technique::SiameseFish,
                    inference: InferenceResult::Elimination {
                        cell,
                        values: vec![digit],
                    },
                    involved_cells: involved,
                    explanation: ExplanationData::Fish {
                        size,
                        digit,
                        base_sectors: combo_a
                            .iter()
                            .chain(combo_b.iter())
                            .map(|&s| sector_name(s))
                            .collect(),
                        cover_sectors: cover_combo.iter().map(|&s| sector_name(s)).collect(),
                        fins: fin_cells_a
                            .iter()
                            .chain(fin_cells_b.iter())
                            .copied()
                            .collect(),
                        variant: "Siamese".into(),
                    },
                    proof: Some(ProofCertificate::Fish {
                        digit,
                        base_sectors: combo_a.iter().chain(combo_b.iter()).copied().collect(),
                        cover_sectors: cover_combo.clone(),
                        fins: fin_cells_a
                            .iter()
                            .chain(fin_cells_b.iter())
                            .copied()
                            .collect(),
                    }),
                });
            }
        }
    }
    None
}

// ==================== Size-1 fish (intersections) ====================

/// Long-form sector name for intersection explanations.
fn sector_name_long(sector: usize) -> String {
    if sector < 9 {
        format!("row {}", sector + 1)
    } else if sector < 18 {
        format!("column {}", sector - 9 + 1)
    } else {
        format!("box {}", sector - 18 + 1)
    }
}

/// Size-1 fish: pointing pair (box → line intersection).
///
/// When all candidates for a digit within a box are confined to a single
/// row or column, that digit can be eliminated from the rest of that row/col.
pub fn find_pointing_pair(fab: &CandidateFabric) -> Option<Finding> {
    for digit in 1..=9u8 {
        if !super::search_budget::tick() {
            return None;
        }
        for box_idx in 0..9 {
            if !super::search_budget::tick() {
                return None;
            }
            let base = SECTOR_BOX_BASE + box_idx;
            let base_mask = sector_candidate_mask(fab, base, digit);
            let base_count = base_mask.count_ones();
            if !(2..=3).contains(&base_count) {
                continue;
            }

            // Try each row and column as the cover
            for cover in 0..18 {
                if !super::search_budget::tick() {
                    return None;
                }
                let cover_mask = sector_candidate_mask(fab, cover, digit);
                // All base cells must lie within the cover sector
                if base_mask & cover_mask != base_mask {
                    continue;
                }
                // Eliminations: candidates in cover sector outside the base
                let elim_mask = cover_mask & !base_mask;
                if elim_mask == 0 {
                    continue;
                }
                for cell in 0..81usize {
                    if !super::search_budget::tick() {
                        return None;
                    }
                    if elim_mask & (1u128 << cell) != 0 {
                        let involved: Vec<usize> =
                            (0..81).filter(|&c| base_mask & (1u128 << c) != 0).collect();
                        return Some(Finding {
                            technique: Technique::PointingPair,
                            inference: InferenceResult::Elimination {
                                cell,
                                values: vec![digit],
                            },
                            involved_cells: involved,
                            explanation: ExplanationData::Intersection {
                                kind: "Pointing Pair",
                                digit,
                                from_sector: sector_name_long(base),
                                to_sector: sector_name_long(cover),
                            },
                            proof: Some(ProofCertificate::Fish {
                                digit,
                                base_sectors: vec![base],
                                cover_sectors: vec![cover],
                                fins: vec![],
                            }),
                        });
                    }
                }
            }
        }
    }
    None
}

/// Size-1 fish: box-line reduction (line → box intersection).
///
/// When all candidates for a digit within a row or column are confined to
/// a single box, that digit can be eliminated from the rest of that box.
pub fn find_box_line_reduction(fab: &CandidateFabric) -> Option<Finding> {
    for digit in 1..=9u8 {
        if !super::search_budget::tick() {
            return None;
        }
        for line in 0..18usize {
            if !super::search_budget::tick() {
                return None;
            }
            let base_mask = sector_candidate_mask(fab, line, digit);
            let base_count = base_mask.count_ones();
            if !(2..=3).contains(&base_count) {
                continue;
            }

            for box_idx in 0..9 {
                if !super::search_budget::tick() {
                    return None;
                }
                let cover = SECTOR_BOX_BASE + box_idx;
                let cover_mask = sector_candidate_mask(fab, cover, digit);
                // All base cells must lie within the cover sector
                if base_mask & cover_mask != base_mask {
                    continue;
                }
                // Eliminations: candidates in cover sector outside the base
                let elim_mask = cover_mask & !base_mask;
                if elim_mask == 0 {
                    continue;
                }
                for cell in 0..81usize {
                    if !super::search_budget::tick() {
                        return None;
                    }
                    if elim_mask & (1u128 << cell) != 0 {
                        let involved: Vec<usize> =
                            (0..81).filter(|&c| base_mask & (1u128 << c) != 0).collect();
                        return Some(Finding {
                            technique: Technique::BoxLineReduction,
                            inference: InferenceResult::Elimination {
                                cell,
                                values: vec![digit],
                            },
                            involved_cells: involved,
                            explanation: ExplanationData::Intersection {
                                kind: "Box/Line Reduction",
                                digit,
                                from_sector: sector_name_long(line),
                                to_sector: sector_name_long(cover),
                            },
                            proof: Some(ProofCertificate::Fish {
                                digit,
                                base_sectors: vec![line],
                                cover_sectors: vec![cover],
                                fins: vec![],
                            }),
                        });
                    }
                }
            }
        }
    }
    None
}

// ==================== Public API (size 2+) ====================

/// Search for basic fish (X-Wing, Swordfish, Jellyfish) and finned variants.
pub fn find_basic_fish(fab: &CandidateFabric, size: usize) -> Option<Finding> {
    for digit in 1..=9u8 {
        if !super::search_budget::tick() {
            return None;
        }
        if let Some(f) = search_fish_for_digit(fab, digit, size, SectorConstraint::Basic) {
            return Some(f);
        }
    }
    None
}

/// Search for finned fish of given size.
pub fn find_finned_fish(fab: &CandidateFabric, size: usize) -> Option<Finding> {
    // The search_fish_for_digit already handles finned variants
    // We need to filter for finned-only results
    for digit in 1..=9u8 {
        if !super::search_budget::tick() {
            return None;
        }
        if let Some(f) = search_fish_for_digit(fab, digit, size, SectorConstraint::Basic) {
            // Check if it's actually finned (the technique classification tells us)
            match f.technique {
                Technique::FinnedXWing
                | Technique::FinnedSwordfish
                | Technique::FinnedJellyfish => {
                    return Some(f);
                }
                _ => {}
            }
        }
    }
    None
}

/// Search for Franken fish (mixed line+box sectors).
pub fn find_franken_fish(fab: &CandidateFabric) -> Option<Finding> {
    for digit in 1..=9u8 {
        if !super::search_budget::tick() {
            return None;
        }
        for size in 2..=4 {
            if !super::search_budget::tick() {
                return None;
            }
            if let Some(f) = search_fish_for_digit(fab, digit, size, SectorConstraint::Franken) {
                return Some(f);
            }
        }
    }
    None
}

/// Search for Mutant fish (all three sector types).
pub fn find_mutant_fish(fab: &CandidateFabric) -> Option<Finding> {
    for digit in 1..=9u8 {
        if !super::search_budget::tick() {
            return None;
        }
        for size in 2..=4 {
            if !super::search_budget::tick() {
                return None;
            }
            if let Some(f) = search_fish_for_digit(fab, digit, size, SectorConstraint::Mutant) {
                return Some(f);
            }
        }
    }
    None
}

// ==================== Combination utility ====================

fn combinations(items: &[usize], k: usize) -> Vec<Vec<usize>> {
    let mut result = Vec::new();
    if k == 0 || k > items.len() {
        return result;
    }
    let mut indices: Vec<usize> = (0..k).collect();
    loop {
        result.push(indices.iter().map(|&i| items[i]).collect());
        let mut i = k;
        loop {
            if i == 0 {
                return result;
            }
            i -= 1;
            indices[i] += 1;
            if indices[i] <= items.len() - k + i {
                break;
            }
        }
        for j in (i + 1)..k {
            indices[j] = indices[j - 1] + 1;
        }
    }
}
