//! Uniqueness-based techniques.
//!
//! These rely on the uniqueness assumption (puzzle has exactly one solution).
//! Replaces: find_unique_rectangle, find_avoidable_rectangle,
//! find_hidden_rectangle, find_extended_unique_rectangle, find_bug.
//!
//! Note: Empty Rectangle was moved to aic_engine.rs — it is a single-digit
//! AIC pattern, not a uniqueness technique.

use super::explain::{ExplanationData, Finding, InferenceResult, ProofCertificate};
use super::fabric::{idx_to_pos, sector_cells, CandidateFabric};
use super::types::Technique;
use crate::BitSet;

/// Helper: check if two cells share a row, column, or box.
fn sees(a: usize, b: usize) -> bool {
    if a == b {
        return false;
    }
    let (ar, ac) = (a / 9, a % 9);
    let (br, bc) = (b / 9, b % 9);
    ar == br || ac == bc || (ar / 3 == br / 3 && ac / 3 == bc / 3)
}

fn pos_to_idx(row: usize, col: usize) -> usize {
    row * 9 + col
}

// ==================== Avoidable Rectangle ====================

/// Avoidable Rectangle: Like UR but involves given (clue) cells.
pub fn find_avoidable_rectangle(fab: &CandidateFabric) -> Option<Finding> {
    for r1 in 0..9 {
        if !super::search_budget::tick() {
            return None;
        }
        for r2 in (r1 + 1)..9 {
            if !super::search_budget::tick() {
                return None;
            }
            for c1 in 0..9 {
                if !super::search_budget::tick() {
                    return None;
                }
                for c2 in (c1 + 1)..9 {
                    if !super::search_budget::tick() {
                        return None;
                    }
                    let corners = [
                        pos_to_idx(r1, c1),
                        pos_to_idx(r1, c2),
                        pos_to_idx(r2, c1),
                        pos_to_idx(r2, c2),
                    ];

                    // Need corners in exactly 2 boxes
                    let boxes: std::collections::HashSet<usize> =
                        corners.iter().map(|&c| idx_to_pos(c).box_index()).collect();
                    if boxes.len() != 2 {
                        continue;
                    }

                    // ALL 4 corners must be non-given.  Given cells are fixed
                    // by the puzzle, so swapping digits in the rectangle would
                    // violate the given constraints — the pattern is never
                    // deadly when any corner is a given.
                    if corners.iter().any(|&c| fab.is_given[c]) {
                        continue;
                    }

                    let mut solved_count = 0;
                    let mut empty_corners = Vec::new();
                    let mut digits = std::collections::HashSet::new();

                    for &corner in &corners {
                        if !super::search_budget::tick() {
                            return None;
                        }
                        if fab.values[corner].is_some() {
                            solved_count += 1;
                            if let Some(v) = fab.values[corner] {
                                digits.insert(v);
                            }
                        } else {
                            empty_corners.push(corner);
                        }
                    }

                    // Need exactly 2 distinct digits, 3 solved corners, 1 empty
                    if digits.len() != 2 || solved_count != 3 || empty_corners.len() != 1 {
                        continue;
                    }

                    let empty = empty_corners[0];
                    let cands = fab.cell_cands[empty];

                    // For a deadly swap pattern, diagonal partners must have the
                    // same value.  Identify which digit would complete the rectangle:
                    //   corners[0]-corners[3] are diagonal partners
                    //   corners[1]-corners[2] are diagonal partners
                    // The digit to eliminate from the empty corner is the value of
                    // its diagonal partner.
                    let diag_partner = if empty == corners[0] {
                        corners[3]
                    } else if empty == corners[1] {
                        corners[2]
                    } else if empty == corners[2] {
                        corners[1]
                    } else {
                        corners[0]
                    };
                    let deadly_digit = fab.values[diag_partner];

                    if let Some(d) = deadly_digit {
                        if cands.contains(d) {
                            let ar_floor: Vec<usize> =
                                corners.iter().filter(|&&c| c != empty).copied().collect();
                            return Some(Finding {
                                technique: Technique::AvoidableRectangle,
                                inference: InferenceResult::Elimination {
                                    cell: empty,
                                    values: vec![d],
                                },
                                involved_cells: corners.to_vec(),
                                explanation: ExplanationData::Uniqueness {
                                    variant: "Avoidable Rectangle".into(),
                                },
                                proof: Some(ProofCertificate::Uniqueness {
                                    pattern: "Avoidable Rectangle".into(),
                                    floor_cells: ar_floor,
                                    roof_cells: vec![empty],
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

// ==================== Unique Rectangle (Types 1-6) ====================

/// Find Unique Rectangle patterns.
pub fn find_unique_rectangle(fab: &CandidateFabric) -> Option<Finding> {
    // Collect bivalue cells
    let bivalues: Vec<(usize, u8, u8)> = (0..81)
        .filter_map(|idx| {
            if fab.values[idx].is_some() {
                return None;
            }
            let cands = fab.cell_cands[idx];
            if cands.count() == 2 {
                let vals: Vec<u8> = cands.iter().collect();
                Some((idx, vals[0], vals[1]))
            } else {
                None
            }
        })
        .collect();

    for i in 0..bivalues.len() {
        if !super::search_budget::tick() {
            return None;
        }
        let (c1, a, b) = bivalues[i];

        for &(c2, c, d) in &bivalues[(i + 1)..] {
            if !super::search_budget::tick() {
                return None;
            }
            if !((a == c && b == d) || (a == d && b == c)) {
                continue;
            }

            let (r1, col1) = (c1 / 9, c1 % 9);
            let (r2, col2) = (c2 / 9, c2 % 9);

            if r1 != r2 && col1 != col2 {
                continue;
            }

            if r1 == r2 {
                for other_row in 0..9 {
                    if !super::search_budget::tick() {
                        return None;
                    }
                    if other_row == r1 {
                        continue;
                    }
                    let corner3 = pos_to_idx(other_row, col1);
                    let corner4 = pos_to_idx(other_row, col2);

                    let boxes: std::collections::HashSet<usize> = [c1, c2, corner3, corner4]
                        .iter()
                        .map(|&c| idx_to_pos(c).box_index())
                        .collect();
                    if boxes.len() != 2 {
                        continue;
                    }

                    if let Some(f) = try_ur_hint(fab, c1, c2, corner3, corner4, a, b) {
                        return Some(f);
                    }
                }
            } else {
                for other_col in 0..9 {
                    if !super::search_budget::tick() {
                        return None;
                    }
                    if other_col == col1 {
                        continue;
                    }
                    let corner3 = pos_to_idx(r1, other_col);
                    let corner4 = pos_to_idx(r2, other_col);

                    let boxes: std::collections::HashSet<usize> = [c1, c2, corner3, corner4]
                        .iter()
                        .map(|&c| idx_to_pos(c).box_index())
                        .collect();
                    if boxes.len() != 2 {
                        continue;
                    }

                    if let Some(f) = try_ur_hint(fab, c1, c2, corner3, corner4, a, b) {
                        return Some(f);
                    }
                }
            }
        }
    }
    None
}

/// Try UR Types 1-6 on a specific rectangle.
fn try_ur_hint(
    fab: &CandidateFabric,
    pos1: usize,
    pos2: usize,
    corner3: usize,
    corner4: usize,
    a: u8,
    b: u8,
) -> Option<Finding> {
    if fab.values[corner3].is_some() || fab.values[corner4].is_some() {
        return None;
    }
    let cand3 = fab.cell_cands[corner3];
    let cand4 = fab.cell_cands[corner4];

    if !cand3.contains(a) || !cand3.contains(b) || !cand4.contains(a) || !cand4.contains(b) {
        return None;
    }

    let corners = vec![pos1, pos2, corner3, corner4];

    // Type 1: Three bivalue corners, fourth has extras
    if cand3.count() == 2 && cand4.count() > 2 {
        return Some(Finding {
            technique: Technique::UniqueRectangle,
            inference: InferenceResult::Elimination {
                cell: corner4,
                values: vec![a, b],
            },
            involved_cells: corners,
            explanation: ExplanationData::Uniqueness {
                variant: "Unique Rectangle Type 1".into(),
            },
            proof: Some(ProofCertificate::Uniqueness {
                pattern: "UR Type 1".into(),
                floor_cells: vec![pos1, pos2, corner3],
                roof_cells: vec![corner4],
            }),
        });
    }
    if cand4.count() == 2 && cand3.count() > 2 {
        return Some(Finding {
            technique: Technique::UniqueRectangle,
            inference: InferenceResult::Elimination {
                cell: corner3,
                values: vec![a, b],
            },
            involved_cells: corners,
            explanation: ExplanationData::Uniqueness {
                variant: "Unique Rectangle Type 1".into(),
            },
            proof: Some(ProofCertificate::Uniqueness {
                pattern: "UR Type 1".into(),
                floor_cells: vec![pos1, pos2, corner4],
                roof_cells: vec![corner3],
            }),
        });
    }

    // Type 2: Both non-bivalue corners have same single extra candidate
    if cand3.count() == 3 && cand4.count() == 3 {
        let extra3: Vec<u8> = cand3.iter().filter(|&v| v != a && v != b).collect();
        let extra4: Vec<u8> = cand4.iter().filter(|&v| v != a && v != b).collect();

        if extra3.len() == 1 && extra4.len() == 1 && extra3[0] == extra4[0] {
            let extra = extra3[0];
            for idx in 0..81 {
                if !super::search_budget::tick() {
                    return None;
                }
                if fab.values[idx].is_some() || idx == corner3 || idx == corner4 {
                    continue;
                }
                if !fab.cell_cands[idx].contains(extra) {
                    continue;
                }
                if sees(idx, corner3) && sees(idx, corner4) {
                    let mut involved = corners.clone();
                    involved.push(idx);
                    return Some(Finding {
                        technique: Technique::UniqueRectangle,
                        inference: InferenceResult::Elimination {
                            cell: idx,
                            values: vec![extra],
                        },
                        involved_cells: involved,
                        explanation: ExplanationData::Uniqueness {
                            variant: "Unique Rectangle Type 2".into(),
                        },
                        proof: Some(ProofCertificate::Uniqueness {
                            pattern: "UR Type 2".into(),
                            floor_cells: vec![pos1, pos2],
                            roof_cells: vec![corner3, corner4],
                        }),
                    });
                }
            }
        }
    }

    // Type 3: Non-bivalue corners' extras form naked subset with peers
    if cand3.count() > 2 || cand4.count() > 2 {
        let ur_pair = BitSet::from_slice(&[a, b]);
        let extra3 = cand3.difference(&ur_pair);
        let extra4 = cand4.difference(&ur_pair);
        let combined_extras = extra3.union(&extra4);

        if combined_extras.count() >= 1 && combined_extras.count() <= 4 {
            let subset_size = combined_extras.count() as usize;
            let (r3, c3_col) = (corner3 / 9, corner3 % 9);
            let (r4, c4_col) = (corner4 / 9, corner4 % 9);

            // Check shared units between corner3 and corner4
            let mut units: Vec<usize> = Vec::new();
            if r3 == r4 {
                units.push(r3); // row sector
            }
            if c3_col == c4_col {
                units.push(9 + c3_col); // col sector
            }
            if idx_to_pos(corner3).box_index() == idx_to_pos(corner4).box_index() {
                units.push(18 + idx_to_pos(corner3).box_index()); // box sector
            }

            for &unit_sector in &units {
                if !super::search_budget::tick() {
                    return None;
                }
                let sec_cells = sector_cells(unit_sector);
                let other_cells: Vec<usize> = sec_cells
                    .iter()
                    .filter(|&&c| {
                        c != corner3
                            && c != corner4
                            && c != pos1
                            && c != pos2
                            && fab.values[c].is_none()
                    })
                    .copied()
                    .collect();

                if subset_size >= 2 && other_cells.len() >= subset_size - 1 {
                    // Enumerate combos of (subset_size - 1) other cells
                    for combo in combinations(&other_cells, subset_size - 1) {
                        if !super::search_budget::tick() {
                            return None;
                        }
                        let mut subset_cands = combined_extras;
                        let mut valid = true;
                        for &sc in &combo {
                            if !super::search_budget::tick() {
                                return None;
                            }
                            let sc_cands = fab.cell_cands[sc];
                            if !sc_cands.difference(&combined_extras).is_empty()
                                && sc_cands.intersection(&combined_extras).is_empty()
                            {
                                valid = false;
                                break;
                            }
                            subset_cands = subset_cands.union(&sc_cands);
                        }
                        if !valid || subset_cands.count() as usize != subset_size {
                            continue;
                        }
                        let all_subset = combo
                            .iter()
                            .all(|&sc| fab.cell_cands[sc].difference(&subset_cands).is_empty());
                        if !all_subset {
                            continue;
                        }

                        for &cell in &other_cells {
                            if !super::search_budget::tick() {
                                return None;
                            }
                            if combo.contains(&cell) {
                                continue;
                            }
                            let overlap = fab.cell_cands[cell].intersection(&subset_cands);
                            if !overlap.is_empty() {
                                let mut involved = corners.clone();
                                involved.extend(combo.iter());
                                return Some(Finding {
                                    technique: Technique::UniqueRectangle,
                                    inference: InferenceResult::Elimination {
                                        cell,
                                        values: overlap.iter().collect(),
                                    },
                                    involved_cells: involved,
                                    explanation: ExplanationData::Uniqueness {
                                        variant: "Unique Rectangle Type 3".into(),
                                    },
                                    proof: Some(ProofCertificate::Uniqueness {
                                        pattern: "UR Type 3".into(),
                                        floor_cells: vec![pos1, pos2],
                                        roof_cells: vec![corner3, corner4],
                                    }),
                                });
                            }
                        }
                    }
                }
            }
        }
    }

    // Type 4: Strong link on one UR digit forces the other out
    for &digit in &[a, b] {
        if !super::search_budget::tick() {
            return None;
        }
        let other = if digit == a { b } else { a };
        let (r3, c3_col) = (corner3 / 9, corner3 % 9);
        let (r4, c4_col) = (corner4 / 9, corner4 % 9);

        let check_strong_in_sector = |sector: usize| -> bool {
            let di = (digit - 1) as usize;
            fab.sector_digit_cells[sector][di].count_ones() == 2
        };

        let strong = if r3 == r4 {
            check_strong_in_sector(r3)
        } else if c3_col == c4_col {
            check_strong_in_sector(9 + c3_col)
        } else if idx_to_pos(corner3).box_index() == idx_to_pos(corner4).box_index() {
            check_strong_in_sector(18 + idx_to_pos(corner3).box_index())
        } else {
            false
        };

        if strong {
            if cand3.contains(other) && cand3.count() > 2 {
                return Some(Finding {
                    technique: Technique::UniqueRectangle,
                    inference: InferenceResult::Elimination {
                        cell: corner3,
                        values: vec![other],
                    },
                    involved_cells: corners,
                    explanation: ExplanationData::Uniqueness {
                        variant: "Unique Rectangle Type 4".into(),
                    },
                    proof: Some(ProofCertificate::Uniqueness {
                        pattern: "UR Type 4".into(),
                        floor_cells: vec![pos1, pos2],
                        roof_cells: vec![corner3, corner4],
                    }),
                });
            }
            if cand4.contains(other) && cand4.count() > 2 {
                return Some(Finding {
                    technique: Technique::UniqueRectangle,
                    inference: InferenceResult::Elimination {
                        cell: corner4,
                        values: vec![other],
                    },
                    involved_cells: corners,
                    explanation: ExplanationData::Uniqueness {
                        variant: "Unique Rectangle Type 4".into(),
                    },
                    proof: Some(ProofCertificate::Uniqueness {
                        pattern: "UR Type 4".into(),
                        floor_cells: vec![pos1, pos2],
                        roof_cells: vec![corner3, corner4],
                    }),
                });
            }
        }
    }

    // Type 5: Diagonal non-bivalue corners with same extra
    {
        let (r3, c3_col) = (corner3 / 9, corner3 % 9);
        let (r4, c4_col) = (corner4 / 9, corner4 % 9);

        if cand3.count() == 3 && cand4.count() == 3 && r3 != r4 && c3_col != c4_col {
            let extra3: Vec<u8> = cand3.iter().filter(|&v| v != a && v != b).collect();
            let extra4: Vec<u8> = cand4.iter().filter(|&v| v != a && v != b).collect();

            if extra3.len() == 1 && extra4.len() == 1 && extra3[0] == extra4[0] {
                let extra = extra3[0];
                for idx in 0..81 {
                    if !super::search_budget::tick() {
                        return None;
                    }
                    if fab.values[idx].is_some()
                        || idx == corner3
                        || idx == corner4
                        || idx == pos1
                        || idx == pos2
                    {
                        continue;
                    }
                    if !fab.cell_cands[idx].contains(extra) {
                        continue;
                    }
                    if sees(idx, corner3) && sees(idx, corner4) {
                        let mut involved = corners.clone();
                        involved.push(idx);
                        return Some(Finding {
                            technique: Technique::UniqueRectangle,
                            inference: InferenceResult::Elimination {
                                cell: idx,
                                values: vec![extra],
                            },
                            involved_cells: involved,
                            explanation: ExplanationData::Uniqueness {
                                variant: "Unique Rectangle Type 5".into(),
                            },
                            proof: Some(ProofCertificate::Uniqueness {
                                pattern: "UR Type 5".into(),
                                floor_cells: vec![pos1, pos2],
                                roof_cells: vec![corner3, corner4],
                            }),
                        });
                    }
                }
            }
        }
    }

    // Type 6: Diagonal strong links force digit into diagonal
    {
        let (r3, c3_col) = (corner3 / 9, corner3 % 9);
        let (r4, c4_col) = (corner4 / 9, corner4 % 9);

        if cand3.count() > 2 && cand4.count() > 2 && r3 != r4 && c3_col != c4_col {
            for &digit in &[a, b] {
                if !super::search_budget::tick() {
                    return None;
                }
                let other = if digit == a { b } else { a };
                let di = (digit - 1) as usize;

                let strong_row3 = fab.sector_digit_cells[r3][di].count_ones() == 2;
                let strong_row4 = fab.sector_digit_cells[r4][di].count_ones() == 2;
                let strong_col3 = fab.sector_digit_cells[9 + c3_col][di].count_ones() == 2;
                let strong_col4 = fab.sector_digit_cells[9 + c4_col][di].count_ones() == 2;

                if (strong_row3 || strong_col3) && (strong_row4 || strong_col4) {
                    if cand3.contains(other) && cand3.count() > 2 {
                        return Some(Finding {
                            technique: Technique::UniqueRectangle,
                            inference: InferenceResult::Elimination {
                                cell: corner3,
                                values: vec![other],
                            },
                            involved_cells: corners,
                            explanation: ExplanationData::Uniqueness {
                                variant: "Unique Rectangle Type 6".into(),
                            },
                            proof: Some(ProofCertificate::Uniqueness {
                                pattern: "UR Type 6".into(),
                                floor_cells: vec![pos1, pos2],
                                roof_cells: vec![corner3, corner4],
                            }),
                        });
                    }
                    if cand4.contains(other) && cand4.count() > 2 {
                        return Some(Finding {
                            technique: Technique::UniqueRectangle,
                            inference: InferenceResult::Elimination {
                                cell: corner4,
                                values: vec![other],
                            },
                            involved_cells: corners,
                            explanation: ExplanationData::Uniqueness {
                                variant: "Unique Rectangle Type 6".into(),
                            },
                            proof: Some(ProofCertificate::Uniqueness {
                                pattern: "UR Type 6".into(),
                                floor_cells: vec![pos1, pos2],
                                roof_cells: vec![corner3, corner4],
                            }),
                        });
                    }
                }
            }
        }
    }

    None
}

// ==================== Hidden Rectangle ====================

pub fn find_hidden_rectangle(fab: &CandidateFabric) -> Option<Finding> {
    for r1 in 0..9 {
        if !super::search_budget::tick() {
            return None;
        }
        for r2 in (r1 + 1)..9 {
            if !super::search_budget::tick() {
                return None;
            }
            for c1 in 0..9 {
                if !super::search_budget::tick() {
                    return None;
                }
                for c2 in (c1 + 1)..9 {
                    if !super::search_budget::tick() {
                        return None;
                    }
                    let corners = [
                        pos_to_idx(r1, c1),
                        pos_to_idx(r1, c2),
                        pos_to_idx(r2, c1),
                        pos_to_idx(r2, c2),
                    ];

                    if corners.iter().any(|&c| fab.values[c].is_some()) {
                        continue;
                    }

                    let boxes: std::collections::HashSet<usize> =
                        corners.iter().map(|&c| idx_to_pos(c).box_index()).collect();
                    if boxes.len() != 2 {
                        continue;
                    }

                    // Find common candidates
                    let mut common = fab.cell_cands[corners[0]];
                    for &c in &corners[1..] {
                        if !super::search_budget::tick() {
                            return None;
                        }
                        common = common.intersection(&fab.cell_cands[c]);
                    }
                    if common.count() < 2 {
                        continue;
                    }

                    let common_vec: Vec<u8> = common.iter().collect();
                    for di in 0..common_vec.len() {
                        if !super::search_budget::tick() {
                            return None;
                        }
                        for dj in (di + 1)..common_vec.len() {
                            if !super::search_budget::tick() {
                                return None;
                            }
                            let a = common_vec[di];
                            let b = common_vec[dj];

                            let pair = BitSet::from_slice(&[a, b]);
                            for (index, &corner) in corners.iter().enumerate() {
                                if !super::search_budget::tick() {
                                    return None;
                                }
                                let diagonal = corners[3 - index];
                                // If the eliminated digit occupied this corner,
                                // the two conjugate links would force the other
                                // digit in both adjacent corners. The opposite
                                // bivalue corner would then complete a deadly
                                // rectangle. One link alone proves nothing.
                                if fab.cell_cands[diagonal] != pair
                                    || fab.cell_cands[corner].count() <= 2
                                {
                                    continue;
                                }
                                for &digit in &[a, b] {
                                    if !super::search_budget::tick() {
                                        return None;
                                    }
                                    let other = if digit == a { b } else { a };
                                    let di = (digit - 1) as usize;
                                    let row = corner / 9;
                                    let col = corner % 9;
                                    if fab.sector_digit_cells[row][di].count_ones() != 2
                                        || fab.sector_digit_cells[9 + col][di].count_ones() != 2
                                    {
                                        continue;
                                    }
                                    let floor: Vec<usize> = corners
                                        .iter()
                                        .filter(|&&cell| cell != corner)
                                        .copied()
                                        .collect();
                                    return Some(Finding {
                                        technique: Technique::HiddenRectangle,
                                        inference: InferenceResult::Elimination {
                                            cell: corner,
                                            values: vec![other],
                                        },
                                        involved_cells: corners.to_vec(),
                                        explanation: ExplanationData::Uniqueness {
                                            variant: "Hidden Rectangle".into(),
                                        },
                                        proof: Some(ProofCertificate::Uniqueness {
                                            pattern: "Hidden Rectangle".into(),
                                            floor_cells: floor,
                                            roof_cells: vec![corner],
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

// ==================== Extended Unique Rectangle ====================

pub fn find_extended_unique_rectangle(fab: &CandidateFabric) -> Option<Finding> {
    let try_ext_ur = |corners: &[usize]| -> Option<Finding> {
        if corners.iter().any(|&c| fab.values[c].is_some()) {
            return None;
        }
        let boxes: std::collections::HashSet<usize> =
            corners.iter().map(|&c| idx_to_pos(c).box_index()).collect();
        if boxes.len() < 2 {
            return None;
        }

        let mut common = fab.cell_cands[corners[0]];
        for &c in &corners[1..] {
            if !super::search_budget::tick() {
                return None;
            }
            common = common.intersection(&fab.cell_cands[c]);
        }
        if common.count() < 2 {
            return None;
        }

        let common_vec: Vec<u8> = common.iter().collect();
        for di in 0..common_vec.len() {
            if !super::search_budget::tick() {
                return None;
            }
            for dj in (di + 1)..common_vec.len() {
                if !super::search_budget::tick() {
                    return None;
                }
                let a = common_vec[di];
                let b = common_vec[dj];
                let ur_pair = BitSet::from_slice(&[a, b]);

                let bivalue_count = corners
                    .iter()
                    .filter(|&&c| fab.cell_cands[c].count() == 2 && fab.cell_cands[c] == ur_pair)
                    .count();

                if bivalue_count < 4 {
                    continue;
                }

                for &corner in corners {
                    if !super::search_budget::tick() {
                        return None;
                    }
                    if fab.cell_cands[corner].count() > 2 {
                        let mut elim = Vec::new();
                        if fab.cell_cands[corner].contains(a) {
                            elim.push(a);
                        }
                        if fab.cell_cands[corner].contains(b) {
                            elim.push(b);
                        }
                        if !elim.is_empty() {
                            let eur_floor: Vec<usize> = corners
                                .iter()
                                .filter(|&&c| {
                                    fab.cell_cands[c].count() == 2 && fab.cell_cands[c] == ur_pair
                                })
                                .copied()
                                .collect();
                            let eur_roof: Vec<usize> = corners
                                .iter()
                                .filter(|&&c| fab.cell_cands[c].count() > 2)
                                .copied()
                                .collect();
                            return Some(Finding {
                                technique: Technique::ExtendedUniqueRectangle,
                                inference: InferenceResult::Elimination {
                                    cell: corner,
                                    values: elim,
                                },
                                involved_cells: corners.to_vec(),
                                explanation: ExplanationData::Uniqueness {
                                    variant: "Extended Unique Rectangle".into(),
                                },
                                proof: Some(ProofCertificate::Uniqueness {
                                    pattern: "Extended UR".into(),
                                    floor_cells: eur_floor,
                                    roof_cells: eur_roof,
                                }),
                            });
                        }
                    }
                }
            }
        }
        None
    };

    // 2 rows x 3 cols
    for r1 in 0..9 {
        if !super::search_budget::tick() {
            return None;
        }
        for r2 in (r1 + 1)..9 {
            if !super::search_budget::tick() {
                return None;
            }
            for c1 in 0..9 {
                if !super::search_budget::tick() {
                    return None;
                }
                for c2 in (c1 + 1)..9 {
                    if !super::search_budget::tick() {
                        return None;
                    }
                    for c3 in (c2 + 1)..9 {
                        if !super::search_budget::tick() {
                            return None;
                        }
                        let corners = [
                            pos_to_idx(r1, c1),
                            pos_to_idx(r1, c2),
                            pos_to_idx(r1, c3),
                            pos_to_idx(r2, c1),
                            pos_to_idx(r2, c2),
                            pos_to_idx(r2, c3),
                        ];
                        if let Some(f) = try_ext_ur(&corners) {
                            return Some(f);
                        }
                    }
                }
            }
        }
    }

    // 3 rows x 2 cols
    for r1 in 0..9 {
        if !super::search_budget::tick() {
            return None;
        }
        for r2 in (r1 + 1)..9 {
            if !super::search_budget::tick() {
                return None;
            }
            for r3 in (r2 + 1)..9 {
                if !super::search_budget::tick() {
                    return None;
                }
                for c1 in 0..9 {
                    if !super::search_budget::tick() {
                        return None;
                    }
                    for c2 in (c1 + 1)..9 {
                        if !super::search_budget::tick() {
                            return None;
                        }
                        let corners = [
                            pos_to_idx(r1, c1),
                            pos_to_idx(r1, c2),
                            pos_to_idx(r2, c1),
                            pos_to_idx(r2, c2),
                            pos_to_idx(r3, c1),
                            pos_to_idx(r3, c2),
                        ];
                        if let Some(f) = try_ext_ur(&corners) {
                            return Some(f);
                        }
                    }
                }
            }
        }
    }

    None
}

// ==================== BUG (Bivalue Universal Grave) ====================

/// Removing the proposed extras must leave an actual BUG pattern. Bivalue
/// cells alone are insufficient: every still-missing digit must occur twice
/// in each row, column and box. Otherwise odd counts can select a wrong value.
fn is_bug_remainder(fab: &CandidateFabric, extras: &[(usize, Vec<u8>)]) -> bool {
    let mut candidates = fab.cell_cands;
    for (cell, values) in extras {
        for &value in values {
            candidates[*cell].remove(value);
        }
    }
    for (cell, values) in candidates.iter().enumerate() {
        if fab.values[cell].is_none() && values.count() != 2 {
            return false;
        }
    }
    for sector in 0..27 {
        let cells = sector_cells(sector);
        for digit in 1..=9 {
            let placed = cells.iter().any(|&cell| fab.values[cell] == Some(digit));
            let count = cells
                .iter()
                .filter(|&&cell| fab.values[cell].is_none() && candidates[cell].contains(digit))
                .count();
            if count != if placed { 0 } else { 2 } {
                return false;
            }
        }
    }
    true
}

pub fn find_bug(fab: &CandidateFabric) -> Option<Finding> {
    let empty: Vec<usize> = (0..81).filter(|&c| fab.values[c].is_none()).collect();
    if empty.is_empty() {
        return None;
    }

    let mut non_bivalue: Vec<usize> = Vec::new();
    for &idx in &empty {
        if !super::search_budget::tick() {
            return None;
        }
        let count = fab.cell_cands[idx].count();
        if count < 2 {
            return None;
        }
        if count > 2 {
            non_bivalue.push(idx);
        }
    }

    if non_bivalue.is_empty() {
        return None;
    }

    let total_extra: u32 = non_bivalue
        .iter()
        .map(|&idx| fab.cell_cands[idx].count() - 2)
        .sum();
    if total_extra > 6 {
        return None;
    }

    // BUG+1
    if non_bivalue.len() == 1 && total_extra == 1 {
        let tri = non_bivalue[0];
        let cands = fab.cell_cands[tri];
        let (row, col) = (tri / 9, tri % 9);
        let box_idx = idx_to_pos(tri).box_index();

        for val in cands.iter() {
            if !super::search_budget::tick() {
                return None;
            }
            let row_count = fab.sector_digit_cells[row][(val - 1) as usize].count_ones();
            let col_count = fab.sector_digit_cells[9 + col][(val - 1) as usize].count_ones();
            let box_count = fab.sector_digit_cells[18 + box_idx][(val - 1) as usize].count_ones();

            if (row_count % 2 == 1 || col_count % 2 == 1 || box_count % 2 == 1)
                && is_bug_remainder(fab, &[(tri, vec![val])])
            {
                let bug_floor: Vec<usize> = empty.iter().filter(|&&c| c != tri).copied().collect();
                return Some(Finding {
                    technique: Technique::BivalueUniversalGrave,
                    inference: InferenceResult::Placement {
                        cell: tri,
                        value: val,
                    },
                    involved_cells: vec![tri],
                    explanation: ExplanationData::Uniqueness {
                        variant: "BUG+1".into(),
                    },
                    proof: Some(ProofCertificate::Uniqueness {
                        pattern: "BUG+1".into(),
                        floor_cells: bug_floor,
                        roof_cells: vec![tri],
                    }),
                });
            }
        }
        return None;
    }

    // BUG+n: identify extra candidates, look for eliminations
    let mut cell_extras: Vec<(usize, Vec<u8>)> = Vec::new();
    for &idx in &non_bivalue {
        if !super::search_budget::tick() {
            return None;
        }
        let cands = fab.cell_cands[idx];
        let (row, col) = (idx / 9, idx % 9);
        let box_idx = idx_to_pos(idx).box_index();
        let mut extras = Vec::new();

        for val in cands.iter() {
            if !super::search_budget::tick() {
                return None;
            }
            let row_count = fab.sector_digit_cells[row][(val - 1) as usize].count_ones();
            let col_count = fab.sector_digit_cells[9 + col][(val - 1) as usize].count_ones();
            let box_count = fab.sector_digit_cells[18 + box_idx][(val - 1) as usize].count_ones();

            if row_count % 2 == 1 || col_count % 2 == 1 || box_count % 2 == 1 {
                extras.push(val);
            }
        }
        cell_extras.push((idx, extras));
    }

    if !is_bug_remainder(fab, &cell_extras) {
        return None;
    }

    for digit in 1..=9u8 {
        if !super::search_budget::tick() {
            return None;
        }
        // At least one extra must be true. Removing this digit from a common
        // peer is justified only when it conflicts with every possible extra,
        // including extras in other cells or of other digits.
        if cell_extras
            .iter()
            .any(|(_, extras)| extras.iter().any(|&value| value != digit))
        {
            continue;
        }
        let cells_with_digit: Vec<usize> = cell_extras
            .iter()
            .filter(|(_, exts)| exts.contains(&digit))
            .map(|(idx, _)| *idx)
            .collect();

        if cells_with_digit.len() < 2 {
            continue;
        }

        // Check if all share a row
        if cells_with_digit
            .iter()
            .all(|&c| c / 9 == cells_with_digit[0] / 9)
        {
            let row = cells_with_digit[0] / 9;
            for col in 0..9 {
                if !super::search_budget::tick() {
                    return None;
                }
                let idx = pos_to_idx(row, col);
                if !cells_with_digit.contains(&idx)
                    && fab.values[idx].is_none()
                    && fab.cell_cands[idx].contains(digit)
                {
                    let bug_floor: Vec<usize> = empty
                        .iter()
                        .filter(|&&c| fab.cell_cands[c].count() == 2)
                        .copied()
                        .collect();
                    return Some(Finding {
                        technique: Technique::BivalueUniversalGrave,
                        inference: InferenceResult::Elimination {
                            cell: idx,
                            values: vec![digit],
                        },
                        involved_cells: non_bivalue.clone(),
                        explanation: ExplanationData::Uniqueness {
                            variant: format!("BUG+{}", total_extra),
                        },
                        proof: Some(ProofCertificate::Uniqueness {
                            pattern: format!("BUG+{}", total_extra),
                            floor_cells: bug_floor,
                            roof_cells: non_bivalue.clone(),
                        }),
                    });
                }
            }
        }

        // Check if all share a column
        if cells_with_digit
            .iter()
            .all(|&c| c % 9 == cells_with_digit[0] % 9)
        {
            let col = cells_with_digit[0] % 9;
            for row in 0..9 {
                if !super::search_budget::tick() {
                    return None;
                }
                let idx = pos_to_idx(row, col);
                if !cells_with_digit.contains(&idx)
                    && fab.values[idx].is_none()
                    && fab.cell_cands[idx].contains(digit)
                {
                    let bug_floor: Vec<usize> = empty
                        .iter()
                        .filter(|&&c| fab.cell_cands[c].count() == 2)
                        .copied()
                        .collect();
                    return Some(Finding {
                        technique: Technique::BivalueUniversalGrave,
                        inference: InferenceResult::Elimination {
                            cell: idx,
                            values: vec![digit],
                        },
                        involved_cells: non_bivalue.clone(),
                        explanation: ExplanationData::Uniqueness {
                            variant: format!("BUG+{}", total_extra),
                        },
                        proof: Some(ProofCertificate::Uniqueness {
                            pattern: format!("BUG+{}", total_extra),
                            floor_cells: bug_floor,
                            roof_cells: non_bivalue.clone(),
                        }),
                    });
                }
            }
        }

        // Check if all share a box
        if cells_with_digit
            .iter()
            .all(|&c| idx_to_pos(c).box_index() == idx_to_pos(cells_with_digit[0]).box_index())
        {
            let box_idx = idx_to_pos(cells_with_digit[0]).box_index();
            let box_sector = 18 + box_idx;
            for &cell in &sector_cells(box_sector) {
                if !super::search_budget::tick() {
                    return None;
                }
                if !cells_with_digit.contains(&cell)
                    && fab.values[cell].is_none()
                    && fab.cell_cands[cell].contains(digit)
                {
                    let bug_floor: Vec<usize> = empty
                        .iter()
                        .filter(|&&c| fab.cell_cands[c].count() == 2)
                        .copied()
                        .collect();
                    return Some(Finding {
                        technique: Technique::BivalueUniversalGrave,
                        inference: InferenceResult::Elimination {
                            cell,
                            values: vec![digit],
                        },
                        involved_cells: non_bivalue.clone(),
                        explanation: ExplanationData::Uniqueness {
                            variant: format!("BUG+{}", total_extra),
                        },
                        proof: Some(ProofCertificate::Uniqueness {
                            pattern: format!("BUG+{}", total_extra),
                            floor_cells: bug_floor,
                            roof_cells: non_bivalue.clone(),
                        }),
                    });
                }
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

#[cfg(test)]
#[path = "uniqueness_tests.rs"]
mod tests;
