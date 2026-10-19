//! Basic techniques: singles and locked subsets.
//!
//! These are direct pattern-matching techniques that don't require
//! the abstract graph/catalog engines. Intersection techniques (pointing
//! pairs, box-line reductions) are now in fish_engine as size-1 fish.

use super::explain::{ExplanationData, Finding, InferenceResult, ProofCertificate};
use super::fabric::{sector_cells, CandidateFabric};
use super::types::Technique;
use crate::BitSet;

/// Name a sector for human-readable explanations.
fn sector_name(sector: usize) -> String {
    if sector < 9 {
        format!("row {}", sector + 1)
    } else if sector < 18 {
        format!("column {}", sector - 9 + 1)
    } else {
        format!("box {}", sector - 18 + 1)
    }
}

// ==================== Naked Single ====================

pub fn find_naked_single(fab: &CandidateFabric) -> Option<Finding> {
    for idx in 0..81 {
        if fab.values[idx].is_some() {
            continue;
        }
        if let Some(value) = fab.cell_cands[idx].single_value() {
            return Some(Finding {
                technique: Technique::NakedSingle,
                inference: InferenceResult::Placement { cell: idx, value },
                involved_cells: vec![idx],
                explanation: ExplanationData::NakedSingle { cell: idx, value },
                proof: Some(ProofCertificate::Basic {
                    kind: "NakedSingle",
                }),
            });
        }
    }
    None
}

// ==================== Hidden Single ====================

pub fn find_hidden_single(fab: &CandidateFabric) -> Option<Finding> {
    for sector in 0..27 {
        for digit in 1..=9u8 {
            let count = fab.sector_cand_count(sector, digit);
            if count == 1 {
                let cells = fab.sector_cells_with_candidate(sector, digit);
                if cells.len() == 1 {
                    let cell = cells[0];
                    return Some(Finding {
                        technique: Technique::HiddenSingle,
                        inference: InferenceResult::Placement { cell, value: digit },
                        involved_cells: vec![cell],
                        explanation: ExplanationData::HiddenSingle {
                            sector,
                            cell,
                            value: digit,
                            sector_name: sector_name(sector),
                        },
                        proof: Some(ProofCertificate::Basic {
                            kind: "HiddenSingle",
                        }),
                    });
                }
            }
        }
    }
    None
}

// ==================== Naked Subset (parameterized) ====================

/// Find a naked subset of given size (2=pair, 3=triple, 4=quad).
/// N cells whose combined candidates have exactly N values.
pub fn find_naked_subset(fab: &CandidateFabric, size: usize) -> Option<Finding> {
    let technique = match size {
        2 => Technique::NakedPair,
        3 => Technique::NakedTriple,
        4 => Technique::NakedQuad,
        _ => return None,
    };

    for sector in 0..27 {
        let sec_cells = sector_cells(sector);
        // Collect empty cells in this sector with appropriate candidate counts
        let empty: Vec<usize> = sec_cells
            .iter()
            .filter(|&&c| {
                fab.values[c].is_none()
                    && fab.cell_cands[c].count() >= 2
                    && fab.cell_cands[c].count() <= size as u32
            })
            .copied()
            .collect();

        if empty.len() < size {
            continue;
        }

        // Enumerate combinations of `size` cells
        for combo in combinations_idx(&empty, size) {
            let union = combo
                .iter()
                .fold(BitSet::empty(), |acc, &c| acc.union(&fab.cell_cands[c]));

            if union.count() == size as u32 {
                let values: Vec<u8> = union.iter().collect();
                // Check for eliminations in other cells of this sector
                for &other in &sec_cells {
                    if fab.values[other].is_some() || combo.contains(&other) {
                        continue;
                    }
                    let to_remove: Vec<u8> = values
                        .iter()
                        .filter(|&&v| fab.cell_cands[other].contains(v))
                        .copied()
                        .collect();
                    if !to_remove.is_empty() {
                        let mut involved = combo.clone();
                        involved.push(other);
                        let proof_kind = match size {
                            2 => "NakedPair",
                            3 => "NakedTriple",
                            _ => "NakedQuad",
                        };
                        return Some(Finding {
                            technique,
                            inference: InferenceResult::Elimination {
                                cell: other,
                                values: to_remove,
                            },
                            involved_cells: involved,
                            explanation: ExplanationData::LockedSet {
                                sector,
                                kind: "Naked",
                                size,
                                cells: combo,
                                values: values.clone(),
                                sector_name: sector_name(sector),
                            },
                            proof: Some(ProofCertificate::Basic { kind: proof_kind }),
                        });
                    }
                }
            }
        }
    }
    None
}

// ==================== Hidden Subset (parameterized) ====================

/// Find a hidden subset of given size (2=pair, 3=triple, 4=quad).
/// N values that appear in only N cells within a sector.
pub fn find_hidden_subset(fab: &CandidateFabric, size: usize) -> Option<Finding> {
    let technique = match size {
        2 => Technique::HiddenPair,
        3 => Technique::HiddenTriple,
        4 => Technique::HiddenQuad,
        _ => return None,
    };

    for sector in 0..27 {
        // Find which digits have 2..=size candidate positions in this sector
        let eligible_digits: Vec<u8> = (1..=9u8)
            .filter(|&d| {
                let cnt = fab.sector_cand_count(sector, d);
                cnt >= 2 && cnt <= size as u8
            })
            .collect();

        if eligible_digits.len() < size {
            continue;
        }

        // Enumerate combinations of `size` digits
        for digit_combo in combinations_u8(&eligible_digits, size) {
            // Find union of cells containing these digits
            let mut cell_set = 0u16; // bitmask of cells within sector
            for &d in &digit_combo {
                cell_set |= fab.sector_digit_cells[sector][(d - 1) as usize];
            }

            if cell_set.count_ones() == size as u32 {
                // Found hidden subset. Eliminate other candidates from these cells.
                let sec_cells = sector_cells(sector);
                let subset_cells: Vec<usize> = (0..9)
                    .filter(|&i| cell_set & (1 << i) != 0)
                    .map(|i| sec_cells[i])
                    .collect();

                let digit_set = BitSet::from_slice(&digit_combo);

                for &cell in &subset_cells {
                    let to_remove: Vec<u8> = fab.cell_cands[cell]
                        .iter()
                        .filter(|&v| !digit_set.contains(v))
                        .collect();
                    if !to_remove.is_empty() {
                        let proof_kind = match size {
                            2 => "HiddenPair",
                            3 => "HiddenTriple",
                            _ => "HiddenQuad",
                        };
                        return Some(Finding {
                            technique,
                            inference: InferenceResult::Elimination {
                                cell,
                                values: to_remove,
                            },
                            involved_cells: subset_cells.clone(),
                            explanation: ExplanationData::LockedSet {
                                sector,
                                kind: "Hidden",
                                size,
                                cells: subset_cells.clone(),
                                values: digit_combo.clone(),
                                sector_name: sector_name(sector),
                            },
                            proof: Some(ProofCertificate::Basic { kind: proof_kind }),
                        });
                    }
                }
            }
        }
    }
    None
}

// ==================== Combination utilities ====================

/// Generate all combinations of `k` items from a slice of usize.
fn combinations_idx(items: &[usize], k: usize) -> Vec<Vec<usize>> {
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

/// Generate all combinations of `k` items from a slice of u8.
fn combinations_u8(items: &[u8], k: usize) -> Vec<Vec<u8>> {
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
#[path = "basic_tests.rs"]
mod tests;
