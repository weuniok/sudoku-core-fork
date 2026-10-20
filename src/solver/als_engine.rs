//! ALS Engine: Almost Locked Set catalog + graph search.
//!
//! Replaces: find_xy_wing, find_xyz_wing, find_wxyz_wing, find_sue_de_coq,
//! find_als_xz, find_als_xy_wing, find_als_chain, find_death_blossom.
//!
//! Core: Enumerate ALS per sector, build RCC graph, search chains.
//! Classification: chain length + ALS sizes determine technique name.

use super::explain::{
    AlsProofDescriptor, ExplanationData, Finding, InferenceResult, ProofCertificate,
};
use super::fabric::{
    sector_cells, CandidateFabric, SECTOR_BOX_BASE, SECTOR_COL_BASE, SECTOR_ROW_BASE,
};
use super::types::Technique;
use crate::BitSet;

/// An Almost Locked Set: N cells with N+1 candidates in a single sector.
#[derive(Debug, Clone)]
struct Als {
    cells: Vec<usize>,  // linear cell indices
    candidates: BitSet, // union of candidates
    sector: usize,      // which sector it belongs to
}

/// Convert an `Als` into an `AlsProofDescriptor` for proof certificates.
fn als_to_descriptor(als: &Als) -> AlsProofDescriptor {
    AlsProofDescriptor {
        cells: als.cells.clone(),
        candidates: als.candidates.iter().collect(),
        sector: als.sector,
    }
}

/// Enumerate all ALS in the grid (sizes 1..=5 cells).
fn enumerate_als(fab: &CandidateFabric) -> Vec<Als> {
    enumerate_als_bounded(fab, &mut super::search_budget::SearchBudget::new(u32::MAX))
}

fn enumerate_als_bounded(
    fab: &CandidateFabric,
    budget: &mut super::search_budget::SearchBudget,
) -> Vec<Als> {
    let mut result = Vec::new();

    for sector in 0..27 {
        let sec_cells = sector_cells(sector);
        let empty: Vec<usize> = sec_cells
            .iter()
            .filter(|&&c| fab.values[c].is_none())
            .copied()
            .collect();

        // ALS of size 1: single cell with 2 candidates (bivalue cell)
        for &c in &empty {
            if fab.cell_cands[c].count() == 2 {
                // Check this isn't a duplicate from another sector
                let min_sector = fab.cell_sectors[c].iter().copied().min().unwrap();
                if sector == min_sector {
                    result.push(Als {
                        cells: vec![c],
                        candidates: fab.cell_cands[c],
                        sector,
                    });
                }
            }
        }

        // ALS of size 2..=5 using Gosper's hack for subset enumeration
        for n in 2..=empty.len().min(5) {
            let mask_limit = 1u32 << empty.len();
            // Use Gosper's hack to enumerate n-bit subsets of empty.len() bits
            if n > empty.len() {
                break;
            }
            let mut set = (1u32 << n) - 1;
            while set < mask_limit {
                if !budget.visit() {
                    return result;
                }
                // Extract the cells from this subset
                let mut cells = Vec::with_capacity(n);
                let mut union = BitSet::empty();
                for (bit, &empty_cell) in empty.iter().enumerate() {
                    if set & (1 << bit) != 0 {
                        cells.push(empty_cell);
                        union = union.union(&fab.cell_cands[empty_cell]);
                    }
                }

                if union.count() == (n + 1) as u32 {
                    cells.sort();
                    // Deduplicate: only add if this is the first sector we find these cells in
                    let is_dup = result.iter().any(|a: &Als| a.cells == cells);
                    if !is_dup {
                        result.push(Als {
                            cells,
                            candidates: union,
                            sector,
                        });
                    }
                }

                // Gosper's hack: next subset of same size
                if set == 0 {
                    break;
                }
                let c = set & (!set).wrapping_add(1); // lowest set bit
                let r = set + c;
                set = (((r ^ set) >> 2) / c) | r;
            }
        }
    }
    result
}

/// Check if value X is an RCC between two ALS: all X-cells in A see all X-cells in B.
fn is_rcc(fab: &CandidateFabric, als_a: &Als, als_b: &Als, x: u8) -> bool {
    let a_cells_x: Vec<usize> = als_a
        .cells
        .iter()
        .filter(|&&c| fab.cell_cands[c].contains(x))
        .copied()
        .collect();
    let b_cells_x: Vec<usize> = als_b
        .cells
        .iter()
        .filter(|&&c| fab.cell_cands[c].contains(x))
        .copied()
        .collect();

    if a_cells_x.is_empty() || b_cells_x.is_empty() {
        return false;
    }

    a_cells_x
        .iter()
        .all(|&a| b_cells_x.iter().all(|&b| fab.sees(a, b)))
}

/// Find RCC values between two non-overlapping ALS.
fn find_rccs(fab: &CandidateFabric, als_a: &Als, als_b: &Als) -> Vec<u8> {
    if als_a.cells.iter().any(|c| als_b.cells.contains(c)) {
        return Vec::new();
    }
    let common = als_a.candidates.intersection(&als_b.candidates);
    common
        .iter()
        .filter(|&x| is_rcc(fab, als_a, als_b, x))
        .collect()
}

// ==================== Wings (small ALS-XZ) ====================

/// XY-Wing: ALS-XZ with 1+1 cells (SE sub-classification, SE 4.2).
pub fn find_xy_wing(fab: &CandidateFabric) -> Option<Finding> {
    find_als_xz_filtered(fab, Some(Technique::XYWing))
}

/// XYZ-Wing: ALS-XZ with 1+2 cells (SE sub-classification, SE 4.4).
pub fn find_xyz_wing(fab: &CandidateFabric) -> Option<Finding> {
    find_als_xz_filtered(fab, Some(Technique::XYZWing))
}

/// WXYZ-Wing: ALS-XZ with total 4 cells (SE sub-classification, SE 4.6).
pub fn find_wxyz_wing(fab: &CandidateFabric) -> Option<Finding> {
    find_als_xz_filtered(fab, Some(Technique::WXYZWing))
}

// ==================== ALS-XZ ====================

/// ALS-XZ: two ALS connected by one RCC. Eliminate shared non-RCC values from common peers.
/// Returns only general ALS-XZ (not wings which are handled by dedicated functions).
pub fn find_als_xz(fab: &CandidateFabric) -> Option<Finding> {
    find_als_xz_filtered(fab, Some(Technique::AlsXz))
}

/// Internal: find ALS-XZ with optional technique filter.
fn find_als_xz_filtered(fab: &CandidateFabric, filter: Option<Technique>) -> Option<Finding> {
    find_als_xz_budget(
        fab,
        filter,
        &mut super::search_budget::SearchBudget::new(u32::MAX),
        false,
    )
}

pub(crate) fn find_wing_bounded(
    fab: &CandidateFabric,
    technique: Technique,
    limit: u32,
) -> (Option<Finding>, u32, bool) {
    let mut budget = super::search_budget::SearchBudget::new(limit);
    let finding = find_als_xz_budget(fab, Some(technique), &mut budget, true);
    (finding, budget.used, budget.exhausted)
}

fn find_als_xz_budget(
    fab: &CandidateFabric,
    filter: Option<Technique>,
    budget: &mut super::search_budget::SearchBudget,
    teaching_names: bool,
) -> Option<Finding> {
    let all_als = enumerate_als_bounded(fab, budget);
    if budget.exhausted {
        return None;
    }

    for i in 0..all_als.len() {
        if !super::search_budget::tick() {
            return None;
        }
        for j in (i + 1)..all_als.len() {
            if !super::search_budget::tick() {
                return None;
            }
            if !budget.visit() {
                return None;
            }
            let als_a = &all_als[i];
            let als_b = &all_als[j];

            if als_a.cells.iter().any(|c| als_b.cells.contains(c)) {
                continue;
            }

            let rccs = find_rccs(fab, als_a, als_b);
            if rccs.is_empty() {
                continue;
            }

            let common = als_a.candidates.intersection(&als_b.candidates);

            for &x in &rccs {
                if !super::search_budget::tick() {
                    return None;
                }
                for z in common.iter() {
                    if !super::search_budget::tick() {
                        return None;
                    }
                    if z == x {
                        continue;
                    }

                    let a_cells_z: Vec<usize> = als_a
                        .cells
                        .iter()
                        .filter(|&&c| fab.cell_cands[c].contains(z))
                        .copied()
                        .collect();
                    let b_cells_z: Vec<usize> = als_b
                        .cells
                        .iter()
                        .filter(|&&c| fab.cell_cands[c].contains(z))
                        .copied()
                        .collect();

                    if a_cells_z.is_empty() || b_cells_z.is_empty() {
                        continue;
                    }

                    // Eliminate z from cells seeing all z-cells in both ALS
                    for cell in 0..81 {
                        if !super::search_budget::tick() {
                            return None;
                        }
                        if fab.values[cell].is_some() || !fab.cell_cands[cell].contains(z) {
                            continue;
                        }
                        if als_a.cells.contains(&cell) || als_b.cells.contains(&cell) {
                            continue;
                        }
                        if a_cells_z.iter().all(|&a| fab.sees(cell, a))
                            && b_cells_z.iter().all(|&b| fab.sees(cell, b))
                        {
                            // The legacy rating classifier calls a 1+1 ALS pair
                            // XY-Wing. For teaching, require the conventional
                            // three bivalue cells with an XY pivot and XZ/YZ wings.
                            let technique = if teaching_names && filter == Some(Technique::XYWing) {
                                if !is_teaching_xy_wing(fab, als_a, als_b, z) {
                                    continue;
                                }
                                Technique::XYWing
                            } else if teaching_names && filter == Some(Technique::XYZWing) {
                                if !is_teaching_xyz_wing(fab, als_a, als_b, z) {
                                    continue;
                                }
                                Technique::XYZWing
                            } else {
                                classify_als_pair(als_a, als_b)
                            };
                            if let Some(f) = filter {
                                if technique != f {
                                    continue;
                                }
                            }
                            let mut involved = als_a.cells.clone();
                            involved.extend(&als_b.cells);
                            return Some(Finding {
                                technique,
                                inference: InferenceResult::Elimination {
                                    cell,
                                    values: vec![z],
                                },
                                involved_cells: involved,
                                explanation: ExplanationData::Als {
                                    variant: technique_name(technique).into(),
                                    chain_length: 2,
                                    shared_value: Some(z),
                                },
                                proof: Some(ProofCertificate::Als {
                                    als_chain: vec![
                                        als_to_descriptor(als_a),
                                        als_to_descriptor(als_b),
                                    ],
                                    rcc_values: vec![x],
                                    z_value: Some(z),
                                }),
                            });
                        }
                    }
                }
            }
        }
    }
    None
}

fn is_teaching_xyz_wing(fab: &CandidateFabric, a: &Als, b: &Als, z: u8) -> bool {
    let cells: Vec<usize> = a.cells.iter().chain(&b.cells).copied().collect();
    if cells.len() != 3 {
        return false;
    }
    let pivots: Vec<_> = cells
        .iter()
        .copied()
        .filter(|&c| fab.cell_cands[c].count() == 3)
        .collect();
    if pivots.len() != 1 {
        return false;
    }
    let pivot = pivots[0];
    let wings: Vec<_> = cells.into_iter().filter(|&c| c != pivot).collect();
    let left = fab.cell_cands[wings[0]];
    let right = fab.cell_cands[wings[1]];
    left.count() == 2
        && right.count() == 2
        && left != right
        && left.contains(z)
        && right.contains(z)
        && fab.cell_cands[pivot].contains(z)
        && left.union(&right) == fab.cell_cands[pivot]
        && fab.sees(pivot, wings[0])
        && fab.sees(pivot, wings[1])
}

fn is_teaching_xy_wing(fab: &CandidateFabric, a: &Als, b: &Als, z: u8) -> bool {
    let cells: Vec<usize> = a.cells.iter().chain(&b.cells).copied().collect();
    if cells.len() != 3 || cells.iter().any(|&cell| fab.cell_cands[cell].count() != 2) {
        return false;
    }
    let pivots: Vec<usize> = cells
        .iter()
        .copied()
        .filter(|&cell| !fab.cell_cands[cell].contains(z))
        .collect();
    let wings: Vec<usize> = cells
        .iter()
        .copied()
        .filter(|&cell| fab.cell_cands[cell].contains(z))
        .collect();
    if pivots.len() != 1 || wings.len() != 2 {
        return false;
    }
    let pivot = pivots[0];
    let left = fab.cell_cands[pivot].intersection(&fab.cell_cands[wings[0]]);
    let right = fab.cell_cands[pivot].intersection(&fab.cell_cands[wings[1]]);
    fab.sees(pivot, wings[0])
        && fab.sees(pivot, wings[1])
        && left.count() == 1
        && right.count() == 1
        && left != right
}

/// Classify an ALS pair by cell sizes into SE rating sub-classifications of ALS-XZ.
///
/// XY-Wing (1+1), XYZ-Wing (1+2), WXYZ-Wing (total 4), and general ALS-XZ
/// are all the same ALS degree-of-freedom pattern; the names are retained
/// for Sudoku Explainer rating compatibility.
#[allow(deprecated)]
fn classify_als_pair(a: &Als, b: &Als) -> Technique {
    let total = a.cells.len() + b.cells.len();
    match (a.cells.len(), b.cells.len()) {
        (1, 1) => Technique::XYWing,
        (1, 2) | (2, 1) => Technique::XYZWing,
        _ if total == 4 => Technique::WXYZWing,
        _ => Technique::AlsXz,
    }
}

fn technique_name(t: Technique) -> &'static str {
    match t {
        Technique::XYWing => "XY-Wing",
        Technique::XYZWing => "XYZ-Wing",
        Technique::WXYZWing => "WXYZ-Wing",
        Technique::AlsXz => "ALS-XZ",
        Technique::AlsXyWing => "ALS-XY-Wing",
        Technique::AlsChain => "ALS Chain",
        Technique::SueDeCoq => "Sue de Coq",
        Technique::DeathBlossom => "Death Blossom",
        _ => "ALS",
    }
}

// ==================== ALS-XY-Wing (3-ALS chain) ====================

pub fn find_als_xy_wing(fab: &CandidateFabric) -> Option<Finding> {
    find_als_xy_wing_with_order(fab, false)
}
pub(crate) fn find_als_xy_wing_teaching(fab: &CandidateFabric) -> Option<Finding> {
    find_als_xy_wing_with_order(fab, true)
}
fn find_als_xy_wing_with_order(fab: &CandidateFabric, stable: bool) -> Option<Finding> {
    let all_als = enumerate_als(fab);
    let small_als: Vec<&Als> = all_als
        .iter()
        .filter(|a| a.cells.len() <= 4)
        .take(60)
        .collect();

    // Precompute RCC map
    let mut rcc_map: std::collections::HashMap<(usize, usize), Vec<u8>> =
        std::collections::HashMap::new();
    for i in 0..small_als.len() {
        if !super::search_budget::tick() {
            return None;
        }
        for j in (i + 1)..small_als.len() {
            if !super::search_budget::tick() {
                return None;
            }
            let rccs = find_rccs(fab, small_als[i], small_als[j]);
            if !rccs.is_empty() {
                rcc_map.insert((i, j), rccs.clone());
                rcc_map.insert((j, i), rccs);
            }
        }
    }

    let mut entries: Vec<_> = rcc_map.iter().collect();
    if stable {
        entries.sort_by_key(|(key, _)| **key);
    }
    // Try chains A-B-C (length 3)
    for a_idx in 0..small_als.len() {
        if !super::search_budget::tick() {
            return None;
        }
        for (&(from, b_idx), rccs_ab) in entries.iter().copied() {
            if !super::search_budget::tick() {
                return None;
            }
            if from != a_idx {
                continue;
            }
            for (&(from2, c_idx), rccs_bc) in entries.iter().copied() {
                if !super::search_budget::tick() {
                    return None;
                }
                if from2 != b_idx || c_idx == a_idx {
                    continue;
                }

                let als_a = small_als[a_idx];
                let als_c = small_als[c_idx];

                if als_a.cells.iter().any(|c| als_c.cells.contains(c)) {
                    continue;
                }

                let common_ac = als_a.candidates.intersection(&als_c.candidates);

                for &x in rccs_ab {
                    if !super::search_budget::tick() {
                        return None;
                    }
                    for &y in rccs_bc {
                        if !super::search_budget::tick() {
                            return None;
                        }
                        if y == x {
                            continue;
                        }

                        for z in common_ac.iter() {
                            if !super::search_budget::tick() {
                                return None;
                            }
                            if z == x || z == y {
                                continue;
                            }

                            let a_cells_z: Vec<usize> = als_a
                                .cells
                                .iter()
                                .filter(|&&c| fab.cell_cands[c].contains(z))
                                .copied()
                                .collect();
                            let c_cells_z: Vec<usize> = als_c
                                .cells
                                .iter()
                                .filter(|&&c| fab.cell_cands[c].contains(z))
                                .copied()
                                .collect();

                            if a_cells_z.is_empty() || c_cells_z.is_empty() {
                                continue;
                            }

                            for cell in 0..81 {
                                if !super::search_budget::tick() {
                                    return None;
                                }
                                if fab.values[cell].is_some() || !fab.cell_cands[cell].contains(z) {
                                    continue;
                                }
                                if als_a.cells.contains(&cell) || als_c.cells.contains(&cell) {
                                    continue;
                                }
                                if a_cells_z.iter().all(|&a| fab.sees(cell, a))
                                    && c_cells_z.iter().all(|&c| fab.sees(cell, c))
                                {
                                    let mut involved = als_a.cells.clone();
                                    involved.extend(&small_als[b_idx].cells);
                                    involved.extend(&als_c.cells);
                                    return Some(Finding {
                                        technique: Technique::AlsXyWing,
                                        inference: InferenceResult::Elimination {
                                            cell,
                                            values: vec![z],
                                        },
                                        involved_cells: involved,
                                        explanation: ExplanationData::Als {
                                            variant: "ALS-XY-Wing".into(),
                                            chain_length: 3,
                                            shared_value: Some(z),
                                        },
                                        proof: Some(ProofCertificate::Als {
                                            als_chain: vec![
                                                als_to_descriptor(als_a),
                                                als_to_descriptor(small_als[b_idx]),
                                                als_to_descriptor(als_c),
                                            ],
                                            rcc_values: vec![x, y],
                                            z_value: Some(z),
                                        }),
                                    });
                                }
                            }
                        }
                    }
                }
            }
        }
    }
    None
}

// ==================== ALS Chain (4+ ALS) ====================

pub fn find_als_chain(fab: &CandidateFabric) -> Option<Finding> {
    find_als_chain_with_order(fab, false)
}
pub(crate) fn find_als_chain_teaching(fab: &CandidateFabric) -> Option<Finding> {
    find_als_chain_with_order(fab, true)
}
fn find_als_chain_with_order(fab: &CandidateFabric, stable: bool) -> Option<Finding> {
    let all_als = enumerate_als(fab);
    let small_als: Vec<&Als> = all_als
        .iter()
        .filter(|a| a.cells.len() <= 4)
        .take(50)
        .collect();

    let mut rcc_map: std::collections::HashMap<(usize, usize), Vec<u8>> =
        std::collections::HashMap::new();
    for i in 0..small_als.len() {
        if !super::search_budget::tick() {
            return None;
        }
        for j in (i + 1)..small_als.len() {
            if !super::search_budget::tick() {
                return None;
            }
            let rccs = find_rccs(fab, small_als[i], small_als[j]);
            if !rccs.is_empty() {
                rcc_map.insert((i, j), rccs.clone());
                rcc_map.insert((j, i), rccs);
            }
        }
    }

    // Try chains A-B-C-D (length 4)
    for a_idx in 0..small_als.len() {
        if !super::search_budget::tick() {
            return None;
        }
        let mut a_partners: Vec<(usize, &[u8])> = rcc_map
            .iter()
            .filter(|((from, _), _)| *from == a_idx)
            .map(|((_, to), rccs)| (*to, rccs.as_slice()))
            .collect();
        if stable {
            a_partners.sort_by_key(|(cell, _)| *cell);
        }

        for &(b_idx, rccs_ab) in &a_partners {
            if !super::search_budget::tick() {
                return None;
            }
            let mut b_partners: Vec<(usize, &[u8])> = rcc_map
                .iter()
                .filter(|((from, _), _)| *from == b_idx)
                .map(|((_, to), rccs)| (*to, rccs.as_slice()))
                .collect();
            if stable {
                b_partners.sort_by_key(|(cell, _)| *cell);
            }

            for &(c_idx, rccs_bc) in &b_partners {
                if !super::search_budget::tick() {
                    return None;
                }
                if c_idx == a_idx {
                    continue;
                }

                let mut c_partners: Vec<(usize, &[u8])> = rcc_map
                    .iter()
                    .filter(|((from, _), _)| *from == c_idx)
                    .map(|((_, to), rccs)| (*to, rccs.as_slice()))
                    .collect();
                if stable {
                    c_partners.sort_by_key(|(cell, _)| *cell);
                }

                for &(d_idx, rccs_cd) in &c_partners {
                    if !super::search_budget::tick() {
                        return None;
                    }
                    if d_idx == a_idx || d_idx == b_idx {
                        continue;
                    }

                    let als_a = small_als[a_idx];
                    let als_d = small_als[d_idx];

                    if als_a.cells.iter().any(|c| als_d.cells.contains(c)) {
                        continue;
                    }

                    let common_ad = als_a.candidates.intersection(&als_d.candidates);

                    for &x in rccs_ab {
                        if !super::search_budget::tick() {
                            return None;
                        }
                        for &y in rccs_bc {
                            if !super::search_budget::tick() {
                                return None;
                            }
                            if y == x {
                                continue;
                            }
                            for &w in rccs_cd {
                                if !super::search_budget::tick() {
                                    return None;
                                }
                                if w == y {
                                    continue;
                                }
                                for z in common_ad.iter() {
                                    if !super::search_budget::tick() {
                                        return None;
                                    }
                                    if z == x || z == w {
                                        continue;
                                    }

                                    let a_cells_z: Vec<usize> = als_a
                                        .cells
                                        .iter()
                                        .filter(|&&c| fab.cell_cands[c].contains(z))
                                        .copied()
                                        .collect();
                                    let d_cells_z: Vec<usize> = als_d
                                        .cells
                                        .iter()
                                        .filter(|&&c| fab.cell_cands[c].contains(z))
                                        .copied()
                                        .collect();

                                    if a_cells_z.is_empty() || d_cells_z.is_empty() {
                                        continue;
                                    }

                                    for cell in 0..81 {
                                        if !super::search_budget::tick() {
                                            return None;
                                        }
                                        if fab.values[cell].is_some()
                                            || !fab.cell_cands[cell].contains(z)
                                        {
                                            continue;
                                        }
                                        if als_a.cells.contains(&cell)
                                            || als_d.cells.contains(&cell)
                                        {
                                            continue;
                                        }
                                        if a_cells_z.iter().all(|&a| fab.sees(cell, a))
                                            && d_cells_z.iter().all(|&d| fab.sees(cell, d))
                                        {
                                            let mut involved = als_a.cells.clone();
                                            involved.extend(&small_als[b_idx].cells);
                                            involved.extend(&small_als[c_idx].cells);
                                            involved.extend(&als_d.cells);
                                            return Some(Finding {
                                                technique: Technique::AlsChain,
                                                inference: InferenceResult::Elimination {
                                                    cell,
                                                    values: vec![z],
                                                },
                                                involved_cells: involved,
                                                explanation: ExplanationData::Als {
                                                    variant: "ALS Chain".into(),
                                                    chain_length: 4,
                                                    shared_value: Some(z),
                                                },
                                                proof: Some(ProofCertificate::Als {
                                                    als_chain: vec![
                                                        als_to_descriptor(als_a),
                                                        als_to_descriptor(small_als[b_idx]),
                                                        als_to_descriptor(small_als[c_idx]),
                                                        als_to_descriptor(als_d),
                                                    ],
                                                    rcc_values: vec![x, y, w],
                                                    z_value: Some(z),
                                                }),
                                            });
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    }
    None
}

// ==================== Sue de Coq ====================

/// Sue de Coq (DDS — Distributed Disjoint Subsets): ALS degree-of-freedom
/// decomposition at box/line intersection.
pub fn find_sue_de_coq(fab: &CandidateFabric) -> Option<Finding> {
    // For each box-line intersection (3 cells where a box meets a row/col)
    for box_idx in 0..9 {
        if !super::search_budget::tick() {
            return None;
        }
        let box_cells = sector_cells(SECTOR_BOX_BASE + box_idx);

        for line_type in 0..2 {
            if !super::search_budget::tick() {
                return None;
            }
            for line_idx in 0..9 {
                if !super::search_budget::tick() {
                    return None;
                }
                let line_sector = if line_type == 0 {
                    SECTOR_ROW_BASE + line_idx
                } else {
                    SECTOR_COL_BASE + line_idx
                };
                let line_cells = sector_cells(line_sector);

                // Intersection cells
                let intersection: Vec<usize> = box_cells
                    .iter()
                    .filter(|c| line_cells.contains(c))
                    .copied()
                    .collect();

                // Empty intersection cells
                let empty_inter: Vec<usize> = intersection
                    .iter()
                    .filter(|&&c| fab.values[c].is_none())
                    .copied()
                    .collect();

                if empty_inter.len() < 2 {
                    continue;
                }

                let inter_cands = empty_inter
                    .iter()
                    .fold(BitSet::empty(), |acc, &c| acc.union(&fab.cell_cands[c]));

                if inter_cands.count() < 3 || inter_cands.count() > 5 {
                    continue;
                }

                // Rest of box (not in intersection)
                let rest_box: Vec<usize> = box_cells
                    .iter()
                    .filter(|c| !intersection.contains(c) && fab.values[**c].is_none())
                    .copied()
                    .collect();

                // Rest of line (not in intersection)
                let rest_line: Vec<usize> = line_cells
                    .iter()
                    .filter(|c| !intersection.contains(c) && fab.values[**c].is_none())
                    .copied()
                    .collect();

                // For SdC: candidates in intersection = A ∪ B where
                // A is an ALS from rest-of-line and B is an ALS from rest-of-box
                // Try to find ALS pairs that cover the intersection candidates
                let box_als = find_local_als(fab, &rest_box, SECTOR_BOX_BASE + box_idx);
                let line_als = find_local_als(fab, &rest_line, line_sector);

                for ba in &box_als {
                    if !super::search_budget::tick() {
                        return None;
                    }
                    for la in &line_als {
                        if !super::search_budget::tick() {
                            return None;
                        }
                        let combined = ba.candidates.union(&la.candidates);
                        if combined == inter_cands
                            && ba.candidates.intersection(&la.candidates).is_empty()
                        {
                            // Found SdC! Eliminate:
                            // - ba candidates from rest_box cells not in ba
                            // - la candidates from rest_line cells not in la
                            // Build intersection ALS descriptor (the cells in the
                            // box/line overlap with their combined candidates)
                            let inter_desc = AlsProofDescriptor {
                                cells: empty_inter.clone(),
                                candidates: inter_cands.iter().collect(),
                                sector: line_sector,
                            };
                            let sdc_proof = || {
                                Some(ProofCertificate::Als {
                                    als_chain: vec![
                                        inter_desc.clone(),
                                        als_to_descriptor(ba),
                                        als_to_descriptor(la),
                                    ],
                                    rcc_values: ba
                                        .candidates
                                        .intersection(&inter_cands)
                                        .iter()
                                        .chain(la.candidates.intersection(&inter_cands).iter())
                                        .collect(),
                                    z_value: None,
                                })
                            };
                            for &cell in &rest_box {
                                if !super::search_budget::tick() {
                                    return None;
                                }
                                if ba.cells.contains(&cell) {
                                    continue;
                                }
                                let to_remove: Vec<u8> = ba
                                    .candidates
                                    .iter()
                                    .filter(|&v| fab.cell_cands[cell].contains(v))
                                    .collect();
                                if !to_remove.is_empty() {
                                    let mut involved = empty_inter.clone();
                                    involved.extend(&ba.cells);
                                    involved.extend(&la.cells);
                                    return Some(Finding {
                                        technique: Technique::SueDeCoq,
                                        inference: InferenceResult::Elimination {
                                            cell,
                                            values: to_remove,
                                        },
                                        involved_cells: involved,
                                        explanation: ExplanationData::Als {
                                            variant: "Sue de Coq".into(),
                                            chain_length: 2,
                                            shared_value: None,
                                        },
                                        proof: sdc_proof(),
                                    });
                                }
                            }
                            for &cell in &rest_line {
                                if !super::search_budget::tick() {
                                    return None;
                                }
                                if la.cells.contains(&cell) {
                                    continue;
                                }
                                let to_remove: Vec<u8> = la
                                    .candidates
                                    .iter()
                                    .filter(|&v| fab.cell_cands[cell].contains(v))
                                    .collect();
                                if !to_remove.is_empty() {
                                    let mut involved = empty_inter.clone();
                                    involved.extend(&ba.cells);
                                    involved.extend(&la.cells);
                                    return Some(Finding {
                                        technique: Technique::SueDeCoq,
                                        inference: InferenceResult::Elimination {
                                            cell,
                                            values: to_remove,
                                        },
                                        involved_cells: involved,
                                        explanation: ExplanationData::Als {
                                            variant: "Sue de Coq".into(),
                                            chain_length: 2,
                                            shared_value: None,
                                        },
                                        proof: sdc_proof(),
                                    });
                                }
                            }
                        }
                    }
                }
            }
        }
    }
    None
}

/// Find ALS within a specific set of cells (not a full sector scan).
fn find_local_als(fab: &CandidateFabric, cells: &[usize], sector: usize) -> Vec<Als> {
    let mut result = Vec::new();
    for n in 1..=cells.len().min(4) {
        for combo in combinations_usize(cells, n) {
            let union = combo
                .iter()
                .fold(BitSet::empty(), |acc, &c| acc.union(&fab.cell_cands[c]));
            if union.count() == (n + 1) as u32 {
                result.push(Als {
                    cells: combo,
                    candidates: union,
                    sector,
                });
            }
        }
    }
    result
}

// ==================== Death Blossom ====================

/// Death Blossom: ALS degree-of-freedom method with star topology
/// (stem cell + petal ALS).
pub fn find_death_blossom(fab: &CandidateFabric) -> Option<Finding> {
    let all_als = enumerate_als(fab);

    for stem in 0..81 {
        if !super::search_budget::tick() {
            return None;
        }
        if fab.values[stem].is_some() {
            continue;
        }
        let stem_cands = fab.cell_cands[stem];
        if stem_cands.count() < 2 || stem_cands.count() > 4 {
            continue;
        }

        // For each candidate of the stem, find an ALS that:
        // 1. Contains that candidate
        // 2. Doesn't contain the stem cell
        // 3. Every occurrence of that candidate sees the stem (restricted link)
        let cand_vec: Vec<u8> = stem_cands.iter().collect();

        // Try to assign one petal ALS per stem candidate
        let mut petal_options: Vec<Vec<usize>> = Vec::new();
        let mut all_have_petals = true;

        for &d in &cand_vec {
            if !super::search_budget::tick() {
                return None;
            }
            let mut options = Vec::new();
            for (idx, als) in all_als.iter().enumerate() {
                if !super::search_budget::tick() {
                    return None;
                }
                if als.cells.contains(&stem) {
                    continue;
                }
                if !als.candidates.contains(d) {
                    continue;
                }
                // Choosing d at the stem must remove d from the entire petal.
                let d_cells: Vec<usize> = als
                    .cells
                    .iter()
                    .filter(|&&c| fab.cell_cands[c].contains(d))
                    .copied()
                    .collect();
                if !d_cells.is_empty() && d_cells.iter().all(|&c| fab.sees(c, stem)) {
                    options.push(idx);
                }
            }
            if options.is_empty() {
                all_have_petals = false;
                break;
            }
            petal_options.push(options);
        }

        if !all_have_petals || petal_options.len() < 2 {
            continue;
        }

        // Try combinations of petals (one per candidate)
        // For simplicity, try the first valid combination
        if let Some(finding) = try_death_blossom(fab, stem, &cand_vec, &petal_options, &all_als) {
            return Some(finding);
        }
    }
    None
}

fn try_death_blossom(
    fab: &CandidateFabric,
    stem: usize,
    cand_vec: &[u8],
    petal_options: &[Vec<usize>],
    all_als: &[Als],
) -> Option<Finding> {
    // Simple: try first valid petal for each candidate
    let mut petals: Vec<usize> = Vec::new();
    for opts in petal_options {
        if !super::search_budget::tick() {
            return None;
        }
        if let Some(&first) = opts.first() {
            petals.push(first);
        } else {
            return None;
        }
    }

    // Check no overlap between petals
    for i in 0..petals.len() {
        if !super::search_budget::tick() {
            return None;
        }
        for j in (i + 1)..petals.len() {
            if !super::search_budget::tick() {
                return None;
            }
            if all_als[petals[i]]
                .cells
                .iter()
                .any(|c| all_als[petals[j]].cells.contains(c))
            {
                return None;
            }
        }
    }

    // Find common elimination: value z that's in all petals (not the linking candidate)
    // and can be eliminated from cells seeing all z-cells in all petals
    let mut common_cands = BitSet::all_9();
    for &pi in &petals {
        if !super::search_budget::tick() {
            return None;
        }
        common_cands = common_cands.intersection(&all_als[pi].candidates);
    }

    // Remove the linking candidates
    for &d in cand_vec {
        if !super::search_budget::tick() {
            return None;
        }
        common_cands.remove(d);
    }

    for z in common_cands.iter() {
        if !super::search_budget::tick() {
            return None;
        }
        let z_cell_groups: Vec<Vec<usize>> = petals
            .iter()
            .map(|&pi| {
                all_als[pi]
                    .cells
                    .iter()
                    .filter(|&&c| fab.cell_cands[c].contains(z))
                    .copied()
                    .collect()
            })
            .collect();

        if z_cell_groups.iter().any(|g| g.is_empty()) {
            continue;
        }

        for cell in 0..81 {
            if !super::search_budget::tick() {
                return None;
            }
            if cell == stem || fab.values[cell].is_some() || !fab.cell_cands[cell].contains(z) {
                continue;
            }
            if petals.iter().any(|&pi| all_als[pi].cells.contains(&cell)) {
                continue;
            }

            let sees_all = z_cell_groups
                .iter()
                .all(|group| group.iter().all(|&zc| fab.sees(cell, zc)));

            if sees_all {
                let mut involved = vec![stem];
                for &pi in &petals {
                    if !super::search_budget::tick() {
                        return None;
                    }
                    involved.extend(&all_als[pi].cells);
                }
                // Stem as a size-1 ALS descriptor, plus each petal ALS
                let stem_desc = AlsProofDescriptor {
                    cells: vec![stem],
                    candidates: fab.cell_cands[stem].iter().collect(),
                    sector: fab.cell_sectors[stem].iter().copied().min().unwrap_or(0),
                };
                let mut als_chain = vec![stem_desc];
                for &pi in &petals {
                    if !super::search_budget::tick() {
                        return None;
                    }
                    als_chain.push(als_to_descriptor(&all_als[pi]));
                }
                return Some(Finding {
                    technique: Technique::DeathBlossom,
                    inference: InferenceResult::Elimination {
                        cell,
                        values: vec![z],
                    },
                    involved_cells: involved,
                    explanation: ExplanationData::Als {
                        variant: "Death Blossom".into(),
                        chain_length: petals.len(),
                        shared_value: Some(z),
                    },
                    proof: Some(ProofCertificate::Als {
                        als_chain,
                        rcc_values: cand_vec.to_vec(),
                        z_value: Some(z),
                    }),
                });
            }
        }
    }
    None
}

// ==================== Aligned Pair/Triplet Exclusion ====================
//
// Reframed as implicit ALS constraints: N mutually visible cells form a
// constrained set. Enumerate valid value assignments (respecting visibility),
// and if every assignment includes value z, then z is "locked" to the set —
// eliminate z from cells that see all members but aren't part of the set.

/// Aligned Pair Exclusion: two mutually visible cells whose valid value pairs
/// lock a candidate, eliminating it from common peers.
///
/// RETIRED by community consensus (StrmCkr / Players Forum): subsumed by
/// ALS chains. Retained for SE rating compatibility (SE 6.2).
#[deprecated(note = "Subsumed by ALS chains. Retained for SE rating compatibility (SE 6.2).")]
#[allow(deprecated)]
pub fn find_aligned_pair_exclusion(fab: &CandidateFabric) -> Option<Finding> {
    let empty: Vec<usize> = (0..81).filter(|&c| fab.values[c].is_none()).collect();

    for i in 0..empty.len() {
        if !super::search_budget::tick() {
            return None;
        }
        let p1 = empty[i];
        let c1 = fab.cell_cands[p1];
        if c1.count() < 2 || c1.count() > 5 {
            continue;
        }

        for j in (i + 1)..empty.len() {
            if !super::search_budget::tick() {
                return None;
            }
            let p2 = empty[j];
            if !fab.sees(p1, p2) {
                continue;
            }
            let c2 = fab.cell_cands[p2];
            if c2.count() < 2 || c2.count() > 5 {
                continue;
            }

            // Enumerate valid value pairs (mutual visibility forbids equal values)
            let mut valid_pairs: Vec<(u8, u8)> = Vec::new();
            for v1 in c1.iter() {
                if !super::search_budget::tick() {
                    return None;
                }
                for v2 in c2.iter() {
                    if !super::search_budget::tick() {
                        return None;
                    }
                    if v1 != v2 {
                        valid_pairs.push((v1, v2));
                    }
                }
            }

            if valid_pairs.is_empty() {
                continue;
            }

            // Check common peers for locked values
            for &pos in &empty {
                if !super::search_budget::tick() {
                    return None;
                }
                if pos == p1 || pos == p2 {
                    continue;
                }
                if !fab.sees(pos, p1) || !fab.sees(pos, p2) {
                    continue;
                }

                for val in fab.cell_cands[pos].iter() {
                    if !super::search_budget::tick() {
                        return None;
                    }
                    // If every valid assignment uses val in at least one cell,
                    // val is locked to the pair — eliminate from this peer.
                    let all_exclude = valid_pairs.iter().all(|&(v1, v2)| v1 == val || v2 == val);

                    if all_exclude {
                        return Some(Finding {
                            technique: Technique::AlignedPairExclusion,
                            inference: InferenceResult::Elimination {
                                cell: pos,
                                values: vec![val],
                            },
                            involved_cells: vec![p1, p2, pos],
                            explanation: ExplanationData::Als {
                                variant: "Aligned Pair Exclusion".into(),
                                chain_length: 2,
                                shared_value: Some(val),
                            },
                            proof: Some(ProofCertificate::Als {
                                als_chain: vec![
                                    AlsProofDescriptor {
                                        cells: vec![p1],
                                        candidates: c1.iter().collect(),
                                        sector: fab.cell_sectors[p1]
                                            .iter()
                                            .copied()
                                            .min()
                                            .unwrap_or(0),
                                    },
                                    AlsProofDescriptor {
                                        cells: vec![p2],
                                        candidates: c2.iter().collect(),
                                        sector: fab.cell_sectors[p2]
                                            .iter()
                                            .copied()
                                            .min()
                                            .unwrap_or(0),
                                    },
                                ],
                                rcc_values: vec![val],
                                z_value: Some(val),
                            }),
                        });
                    }
                }
            }
        }
    }
    None
}

/// Aligned Triplet Exclusion: three mutually visible cells whose valid value
/// triples lock a candidate, eliminating it from common peers.
///
/// RETIRED by community consensus (StrmCkr / Players Forum): subsumed by
/// ALS chains. Retained for SE rating compatibility (SE 7.5).
#[deprecated(note = "Subsumed by ALS chains. Retained for SE rating compatibility (SE 7.5).")]
#[allow(deprecated)]
pub fn find_aligned_triplet_exclusion(fab: &CandidateFabric) -> Option<Finding> {
    let empty: Vec<usize> = (0..81).filter(|&c| fab.values[c].is_none()).collect();

    for i in 0..empty.len() {
        if !super::search_budget::tick() {
            return None;
        }
        let p1 = empty[i];
        let c1 = fab.cell_cands[p1];
        if c1.count() < 2 || c1.count() > 4 {
            continue;
        }

        for j in (i + 1)..empty.len() {
            if !super::search_budget::tick() {
                return None;
            }
            let p2 = empty[j];
            if !fab.sees(p1, p2) {
                continue;
            }
            let c2 = fab.cell_cands[p2];
            if c2.count() < 2 || c2.count() > 4 {
                continue;
            }

            for k in (j + 1)..empty.len() {
                if !super::search_budget::tick() {
                    return None;
                }
                let p3 = empty[k];
                if !fab.sees(p1, p3) || !fab.sees(p2, p3) {
                    continue;
                }
                let c3 = fab.cell_cands[p3];
                if c3.count() < 2 || c3.count() > 4 {
                    continue;
                }

                // Enumerate valid value triples (all pairs mutually visible)
                let mut valid_triples: Vec<(u8, u8, u8)> = Vec::new();
                for v1 in c1.iter() {
                    if !super::search_budget::tick() {
                        return None;
                    }
                    for v2 in c2.iter() {
                        if !super::search_budget::tick() {
                            return None;
                        }
                        if v1 == v2 {
                            continue;
                        }
                        for v3 in c3.iter() {
                            if !super::search_budget::tick() {
                                return None;
                            }
                            if v1 != v3 && v2 != v3 {
                                valid_triples.push((v1, v2, v3));
                            }
                        }
                    }
                }

                if valid_triples.is_empty() {
                    continue;
                }

                // Check common peers for locked values
                for &pos in &empty {
                    if !super::search_budget::tick() {
                        return None;
                    }
                    if pos == p1 || pos == p2 || pos == p3 {
                        continue;
                    }
                    if !fab.sees(pos, p1) || !fab.sees(pos, p2) || !fab.sees(pos, p3) {
                        continue;
                    }

                    for val in fab.cell_cands[pos].iter() {
                        if !super::search_budget::tick() {
                            return None;
                        }
                        let all_exclude = valid_triples
                            .iter()
                            .all(|&(v1, v2, v3)| v1 == val || v2 == val || v3 == val);

                        if all_exclude {
                            return Some(Finding {
                                technique: Technique::AlignedTripletExclusion,
                                inference: InferenceResult::Elimination {
                                    cell: pos,
                                    values: vec![val],
                                },
                                involved_cells: vec![p1, p2, p3, pos],
                                explanation: ExplanationData::Als {
                                    variant: "Aligned Triplet Exclusion".into(),
                                    chain_length: 3,
                                    shared_value: Some(val),
                                },
                                proof: Some(ProofCertificate::Als {
                                    als_chain: vec![
                                        AlsProofDescriptor {
                                            cells: vec![p1],
                                            candidates: c1.iter().collect(),
                                            sector: fab.cell_sectors[p1]
                                                .iter()
                                                .copied()
                                                .min()
                                                .unwrap_or(0),
                                        },
                                        AlsProofDescriptor {
                                            cells: vec![p2],
                                            candidates: c2.iter().collect(),
                                            sector: fab.cell_sectors[p2]
                                                .iter()
                                                .copied()
                                                .min()
                                                .unwrap_or(0),
                                        },
                                        AlsProofDescriptor {
                                            cells: vec![p3],
                                            candidates: c3.iter().collect(),
                                            sector: fab.cell_sectors[p3]
                                                .iter()
                                                .copied()
                                                .min()
                                                .unwrap_or(0),
                                        },
                                    ],
                                    rcc_values: vec![val],
                                    z_value: Some(val),
                                }),
                            });
                        }
                    }
                }
            }
        }
    }
    None
}

// ==================== Combination utility ====================

fn combinations_usize(items: &[usize], k: usize) -> Vec<Vec<usize>> {
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

#[cfg(test)]
#[path = "als_engine_tests.rs"]
mod tests;
