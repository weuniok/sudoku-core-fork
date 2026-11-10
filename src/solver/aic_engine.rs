//! AIC Engine: Bipartite polarity inference graph.
//!
//! Replaces: find_x_chain, find_aic, find_three_d_medusa, find_w_wing,
//! find_nishio_forcing_chain, find_kraken_fish, find_cell_forcing_chain,
//! find_region_forcing_chain, find_dynamic_forcing_chain.
//!
//! Core: Build ON/OFF node graph with directed edges. Chain search finds
//! paths with elimination checks at endpoints. Medusa = strong-link coloring.
//! Forcing chains = multi-source propagation.

use std::collections::{HashMap, HashSet, VecDeque};

use super::explain::{
    ExplanationData, Finding, ForcingSource, InferenceResult, LinkType, Polarity, ProofCertificate,
};
use super::fabric::{sector_cells, CandidateFabric};
use super::types::Technique;
use crate::{Grid, Position};

/// Node in the inference graph: a (cell_index, digit) pair.
type Node = (usize, u8);

/// Strong link map (shared by X-Chain, 3D Medusa, AIC).
///
/// Weak inferences (NAND relationships) are derived on-the-fly from the
/// CandidateFabric rather than pre-computed, since they are trivially
/// enumerable from sector/cell constraints and only one call site needs them.
pub(crate) struct LinkGraph {
    strong: HashMap<Node, Vec<Node>>,
}

/// Build the link graph from the CandidateFabric.
///
/// Only strong links are pre-computed. Weak inferences are derived
/// on-the-fly via `weak_inferences()`.
pub(crate) fn build_link_graph(fab: &CandidateFabric) -> LinkGraph {
    let mut strong: HashMap<Node, Vec<Node>> = HashMap::new();

    // Conjugate pairs: exactly 2 cells for a value in a sector → strong link
    for sector in 0..27 {
        for digit in 1..=9u8 {
            let di = (digit - 1) as usize;
            let mask = fab.sector_digit_cells[sector][di];
            if mask.count_ones() == 2 {
                let sec_cells = sector_cells(sector);
                let cells: Vec<usize> = (0..9)
                    .filter(|&i| mask & (1 << i) != 0)
                    .map(|i| sec_cells[i])
                    .collect();
                let a = (cells[0], digit);
                let b = (cells[1], digit);
                strong.entry(a).or_default().push(b);
                strong.entry(b).or_default().push(a);
            }
        }
    }

    // Bivalue cells: strong link between the two candidates
    for idx in 0..81 {
        if fab.values[idx].is_some() {
            continue;
        }
        let cands = fab.cell_cands[idx];
        if cands.count() == 2 {
            let vals: Vec<u8> = cands.iter().collect();
            let a = (idx, vals[0]);
            let b = (idx, vals[1]);
            strong.entry(a).or_default().push(b);
            strong.entry(b).or_default().push(a);
        }
    }

    // Deduplicate
    for list in strong.values_mut() {
        list.sort_unstable();
        list.dedup();
    }

    LinkGraph { strong }
}

/// Derive weak inferences for a node on-the-fly from the CandidateFabric.
///
/// A weak inference (NAND) exists between two nodes when at most one can
/// be true: same digit in same sector, or different digits in same cell.
fn weak_inferences(fab: &CandidateFabric, node: Node) -> Vec<Node> {
    let (cell, digit) = node;
    let mut result = Vec::new();

    // Same digit, same sector (at most one cell holds this digit per sector)
    for &sector in &fab.cell_sectors[cell] {
        let di = (digit - 1) as usize;
        let mask = fab.sector_digit_cells[sector][di];
        let sec_cells = sector_cells(sector);
        for (i, &c) in sec_cells.iter().enumerate() {
            if mask & (1 << i) != 0 && c != cell {
                result.push((c, digit));
            }
        }
    }

    // Same cell, different digit (a cell holds at most one value)
    for d in fab.cell_cands[cell].iter() {
        if d != digit {
            result.push((cell, d));
        }
    }

    result.sort_unstable();
    result.dedup();
    result
}

// ==================== Empty Rectangle ====================
//
// Empty Rectangle is a single-digit pattern (X-chain sub-class), NOT a
// uniqueness technique.  The box's L/T-shaped candidate distribution acts
// as an ERI (Empty Rectangle Intersection) strong link connecting a row
// and a column within the box.  Combined with a conjugate pair in a
// crossing line, it forms a 2-strong-link single-digit chain.
//
// Community classification (Sudopedia): "Single Digit Patterns"
// StrmCkr taxonomy: (row or col) + box => type 2 to type 5
// sudokuwiki.org has retired the standalone name in favour of X-chain.

/// Empty Rectangle: single-digit pattern using an ERI pivot in a box
/// combined with a conjugate pair in a crossing line.
pub fn find_empty_rectangle(fab: &CandidateFabric) -> Option<Finding> {
    use super::fabric::{SECTOR_BOX_BASE, SECTOR_COL_BASE};

    for digit in 1..=9u8 {
        if !super::search_budget::tick() {
            return None;
        }
        let di = (digit - 1) as usize;
        for box_idx in 0..9 {
            if !super::search_budget::tick() {
                return None;
            }
            let box_sector = SECTOR_BOX_BASE + box_idx;
            let digit_cells: Vec<usize> = sector_cells(box_sector)
                .into_iter()
                .filter(|&cell| fab.values[cell].is_none() && fab.cell_cands[cell].contains(digit))
                .collect();
            let box_row = box_idx / 3 * 3;
            let box_col = box_idx % 3 * 3;

            for er_row in box_row..box_row + 3 {
                if !super::search_budget::tick() {
                    return None;
                }
                for er_col in box_col..box_col + 3 {
                    if !super::search_budget::tick() {
                        return None;
                    }
                    // The box digit must lie on one of the two arms. Both arms
                    // must contain a candidate outside their intersection.
                    if !digit_cells
                        .iter()
                        .all(|&cell| cell / 9 == er_row || cell % 9 == er_col)
                        || !digit_cells
                            .iter()
                            .any(|&cell| cell / 9 == er_row && cell % 9 != er_col)
                        || !digit_cells
                            .iter()
                            .any(|&cell| cell % 9 == er_col && cell / 9 != er_row)
                    {
                        continue;
                    }

                    // Try the row arm, then its transposed column-arm pattern.
                    for transposed in [false, true] {
                        if !super::search_budget::tick() {
                            return None;
                        }
                        let arm = if transposed { er_col } else { er_row };
                        let cross_arm = if transposed { er_row } else { er_col };
                        let outside_box = if transposed { box_row } else { box_col };
                        let arm_box = if transposed { box_col } else { box_row };
                        for crossing in 0..9 {
                            if !super::search_budget::tick() {
                                return None;
                            }
                            if (outside_box..outside_box + 3).contains(&crossing) {
                                continue;
                            }
                            let sector = if transposed {
                                crossing
                            } else {
                                SECTOR_COL_BASE + crossing
                            };
                            let mask = fab.sector_digit_cells[sector][di];
                            if mask.count_ones() != 2 || mask & (1 << arm) == 0 {
                                continue;
                            }
                            let other = (mask & !(1 << arm)).trailing_zeros() as usize;
                            if (arm_box..arm_box + 3).contains(&other) {
                                continue;
                            }
                            let target = if transposed {
                                cross_arm * 9 + other
                            } else {
                                other * 9 + cross_arm
                            };
                            if fab.values[target].is_some()
                                || !fab.cell_cands[target].contains(digit)
                            {
                                continue;
                            }

                            // If the target held this digit, it would remove the
                            // box's opposite arm and the far conjugate endpoint.
                            // The near endpoint and the remaining box arm would
                            // then both require the digit in the same line.
                            let mut involved = digit_cells.clone();
                            involved.extend([arm, other].map(|coordinate| {
                                if transposed {
                                    crossing * 9 + coordinate
                                } else {
                                    coordinate * 9 + crossing
                                }
                            }));
                            let fins = digit_cells
                                .iter()
                                .copied()
                                .filter(|&cell| {
                                    if transposed {
                                        cell % 9 != arm
                                    } else {
                                        cell / 9 != arm
                                    }
                                })
                                .collect();
                            let cover_sectors = if transposed {
                                vec![SECTOR_COL_BASE + arm, SECTOR_COL_BASE + other]
                            } else {
                                vec![arm, other]
                            };
                            return Some(Finding {
                                technique: Technique::EmptyRectangle,
                                inference: InferenceResult::Elimination {
                                    cell: target,
                                    values: vec![digit],
                                },
                                involved_cells: involved,
                                explanation: ExplanationData::Chain {
                                    variant: "Empty Rectangle".into(),
                                    chain_length: 3,
                                    values: vec![digit],
                                },
                                // The ERI is a grouped inference: represent its
                                // complete box arm as fins of a two-base fish,
                                // rather than falsely asserting a strong link
                                // between two individual candidates in that arm.
                                proof: Some(ProofCertificate::Fish {
                                    digit,
                                    base_sectors: vec![box_sector, sector],
                                    cover_sectors,
                                    fins,
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

// ==================== W-Wing ====================

/// W-Wing: Two bivalue cells with same candidates {x,y}, connected by a
/// strong link on one value. Eliminates the other value from common peers.
pub fn find_w_wing(fab: &CandidateFabric) -> Option<Finding> {
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
        for j in (i + 1)..bivalues.len() {
            if !super::search_budget::tick() {
                return None;
            }
            let (c1, a1, b1) = bivalues[i];
            let (c2, a2, b2) = bivalues[j];

            if !((a1 == a2 && b1 == b2) || (a1 == b2 && b1 == a2)) {
                continue;
            }

            let x = a1;
            let y = b1;

            // Check for strong link on x or y in some sector
            for &link_val in &[x, y] {
                if !super::search_budget::tick() {
                    return None;
                }
                let other_val = if link_val == x { y } else { x };

                // Check all sectors for a conjugate pair on link_val
                for sector in 0..27 {
                    if !super::search_budget::tick() {
                        return None;
                    }
                    let di = (link_val - 1) as usize;
                    let mask = fab.sector_digit_cells[sector][di];
                    if mask.count_ones() != 2 {
                        continue;
                    }

                    let sec_cells = sector_cells(sector);
                    let link_cells: Vec<usize> = (0..9)
                        .filter(|&i| mask & (1 << i) != 0)
                        .map(|i| sec_cells[i])
                        .collect();

                    let l1 = link_cells[0];
                    let l2 = link_cells[1];

                    // Bivalue cells must not be part of the conjugate pair itself;
                    // if c1 == l1 the proof breaks (c1=b and l1≠b are contradictory).
                    if c1 == l1 || c1 == l2 || c2 == l1 || c2 == l2 {
                        continue;
                    }

                    // c1 must see one link cell, c2 must see the other (or vice versa)
                    let ok = (fab.sees(c1, l1) && fab.sees(c2, l2))
                        || (fab.sees(c1, l2) && fab.sees(c2, l1));
                    if !ok {
                        continue;
                    }

                    // Eliminate other_val from cells seeing both c1 and c2
                    for idx in 0..81 {
                        if !super::search_budget::tick() {
                            return None;
                        }
                        if fab.values[idx].is_some() || idx == c1 || idx == c2 {
                            continue;
                        }
                        if !fab.cell_cands[idx].contains(other_val) {
                            continue;
                        }
                        if fab.sees(idx, c1) && fab.sees(idx, c2) {
                            return Some(Finding {
                                technique: Technique::WWing,
                                inference: InferenceResult::Elimination {
                                    cell: idx,
                                    values: vec![other_val],
                                },
                                involved_cells: vec![c1, c2, l1, l2],
                                explanation: ExplanationData::Chain {
                                    variant: "W-Wing".into(),
                                    chain_length: 4,
                                    values: vec![x, y],
                                },
                                proof: Some(ProofCertificate::Aic {
                                    chain: vec![
                                        (c1, other_val, Polarity::On),
                                        (c1, link_val, Polarity::Off),
                                        (l1, link_val, Polarity::On),
                                        (l2, link_val, Polarity::Off),
                                        (c2, link_val, Polarity::Off),
                                        (c2, other_val, Polarity::On),
                                    ],
                                    link_types: vec![
                                        LinkType::Strong,
                                        LinkType::WeakInference,
                                        LinkType::Strong,
                                        LinkType::WeakInference,
                                        LinkType::Strong,
                                    ],
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

// ==================== X-Chain ====================

/// X-Chain: Single-digit alternating inference chain (AIC subclass).
pub fn find_x_chain(fab: &CandidateFabric, graph: &LinkGraph) -> Option<Finding> {
    find_aic_with_filter(fab, graph, true)
}

// ==================== AIC ====================

/// AIC: Multi-digit alternating inference chain.
pub fn find_aic(fab: &CandidateFabric, graph: &LinkGraph) -> Option<Finding> {
    find_aic_with_filter(fab, graph, false)
}

/// Classify a 3-strong-link AIC chain using StrmCkr's wing taxonomy.
///
/// Strong link types: V = bivalue cell (same cell, different digits),
/// L = bilocal conjugate pair (same digit, different cells).
/// Returns the community wing name for display.
fn classify_wing(chain: &[Node]) -> Option<&'static str> {
    // 3 strong links = 6 nodes (strong-weak-strong-weak-strong = 5 links, 6 nodes)
    if chain.len() != 6 {
        return None;
    }
    // Strong links are at positions (0,1), (2,3), (4,5)
    let link_type = |i: usize| -> char {
        let (c1, _) = chain[i];
        let (c2, _) = chain[i + 1];
        if c1 == c2 {
            'V'
        } else {
            'L'
        }
    };
    let s0 = link_type(0);
    let s1 = link_type(2);
    let s2 = link_type(4);
    match (s0, s1, s2) {
        ('V', 'V', 'V') => Some("XY-Wing (VVV)"),
        ('V', 'L', 'V') => Some("W-Wing (VLV)"),
        ('L', 'V', 'L') => Some("S-Wing (LVL)"),
        ('V', 'L', 'L') => Some("M-Wing (VLL)"),
        ('L', 'L', 'V') => Some("M-Wing (LLV)"),
        ('L', 'L', 'L') => Some("L-Wing (LLL)"),
        ('V', 'V', 'L') => Some("H-Wing (VVL)"),
        ('L', 'V', 'V') => Some("H-Wing (LVV)"),
        _ => None,
    }
}

/// Core AIC search: BFS alternating strong/weak inferences.
fn find_aic_with_filter(
    fab: &CandidateFabric,
    graph: &LinkGraph,
    single_value_only: bool,
) -> Option<Finding> {
    find_aic_with_budget(
        fab,
        graph,
        single_value_only,
        &mut super::search_budget::SearchBudget::new(u32::MAX),
    )
}

pub(crate) fn find_aic_bounded(fab: &CandidateFabric, limit: u32) -> (Option<Finding>, u32, bool) {
    let graph = build_link_graph(fab);
    let mut budget = super::search_budget::SearchBudget::new(limit);
    let finding = find_aic_with_budget(fab, &graph, false, &mut budget);
    (finding, budget.used, budget.exhausted)
}

fn find_aic_with_budget(
    fab: &CandidateFabric,
    graph: &LinkGraph,
    single_value_only: bool,
    budget: &mut super::search_budget::SearchBudget,
) -> Option<Finding> {
    const MAX_LENGTH: usize = 12;

    let mut all_nodes: Vec<Node> = graph.strong.keys().copied().collect();
    all_nodes.sort_unstable();

    for &start in &all_nodes {
        if !super::search_budget::tick() {
            return None;
        }
        let mut queue: VecDeque<(Node, bool, Vec<Node>)> = VecDeque::new();
        let mut visited: HashSet<(Node, bool)> = HashSet::new();

        // Start by following strong links from start
        if let Some(neighbors) = graph.strong.get(&start) {
            for &next in neighbors {
                if !super::search_budget::tick() {
                    return None;
                }
                if single_value_only && next.1 != start.1 {
                    continue;
                }
                queue.push_back((next, true, vec![start, next]));
            }
        }

        while let Some((current, arrived_strong, chain)) = queue.pop_front() {
            if !budget.visit() {
                return None;
            }
            if chain.len() > MAX_LENGTH {
                continue;
            }

            let key = (current, arrived_strong);
            if visited.contains(&key) {
                continue;
            }
            visited.insert(key);

            if arrived_strong {
                // Both endpoints are alternatives only when the chain starts
                // and ends with a strong link: if start is OFF, the alternating
                // implication forces the final endpoint ON. A weak final link
                // cannot justify removing candidates that see both endpoints.
                if current != start && chain.len() >= 4 {
                    // Type 1: same value at different positions
                    if current.1 == start.1 && current.0 != start.0 {
                        let val = start.1;
                        for idx in 0..81 {
                            if !super::search_budget::tick() {
                                return None;
                            }
                            if fab.values[idx].is_some() || idx == start.0 || idx == current.0 {
                                continue;
                            }
                            if !fab.cell_cands[idx].contains(val) {
                                continue;
                            }
                            if fab.sees(idx, start.0) && fab.sees(idx, current.0) {
                                let tech = if single_value_only {
                                    Technique::XChain
                                } else {
                                    Technique::AIC
                                };
                                let mut involved: Vec<usize> = chain.iter().map(|n| n.0).collect();
                                involved.push(current.0);
                                involved.sort_unstable();
                                involved.dedup();
                                let full_chain = &chain;
                                let aic_chain: Vec<(usize, u8, Polarity)> = full_chain
                                    .iter()
                                    .enumerate()
                                    .map(|(i, &(c, d))| {
                                        let pol = if i % 2 == 0 {
                                            Polarity::Off
                                        } else {
                                            Polarity::On
                                        };
                                        (c, d, pol)
                                    })
                                    .collect();
                                let aic_links: Vec<LinkType> = (0..full_chain.len() - 1)
                                    .map(|i| {
                                        if i % 2 == 0 {
                                            LinkType::Strong
                                        } else {
                                            LinkType::WeakInference
                                        }
                                    })
                                    .collect();
                                // Classify named wings for 3-strong-link chains
                                let variant = if let Some(wing_name) = classify_wing(full_chain) {
                                    wing_name.to_string()
                                } else if single_value_only {
                                    "X-Chain".into()
                                } else {
                                    "AIC".into()
                                };
                                return Some(Finding {
                                    technique: tech,
                                    inference: InferenceResult::Elimination {
                                        cell: idx,
                                        values: vec![val],
                                    },
                                    involved_cells: involved,
                                    explanation: ExplanationData::Chain {
                                        variant,
                                        chain_length: chain.len() - 1,
                                        values: vec![val],
                                    },
                                    proof: Some(ProofCertificate::Aic {
                                        chain: aic_chain,
                                        link_types: aic_links,
                                    }),
                                });
                            }
                        }
                    }

                    // Type 2: same cell, different values → eliminate other candidates
                    if !single_value_only && current.0 == start.0 && current.1 != start.1 {
                        let cands = fab.cell_cands[start.0];
                        let to_remove: Vec<u8> = cands
                            .iter()
                            .filter(|&v| v != start.1 && v != current.1)
                            .collect();
                        if !to_remove.is_empty() {
                            let involved: Vec<usize> = chain.iter().map(|n| n.0).collect();
                            let full_chain = &chain;
                            let aic_chain: Vec<(usize, u8, Polarity)> = full_chain
                                .iter()
                                .enumerate()
                                .map(|(i, &(c, d))| {
                                    let pol = if i % 2 == 0 {
                                        Polarity::Off
                                    } else {
                                        Polarity::On
                                    };
                                    (c, d, pol)
                                })
                                .collect();
                            let aic_links: Vec<LinkType> = (0..full_chain.len() - 1)
                                .map(|i| {
                                    if i % 2 == 0 {
                                        LinkType::Strong
                                    } else {
                                        LinkType::WeakInference
                                    }
                                })
                                .collect();
                            let variant = classify_wing(full_chain)
                                .map(|s| s.to_string())
                                .unwrap_or_else(|| "AIC".into());
                            return Some(Finding {
                                technique: Technique::AIC,
                                inference: InferenceResult::Elimination {
                                    cell: start.0,
                                    values: to_remove.clone(),
                                },
                                involved_cells: involved,
                                explanation: ExplanationData::Chain {
                                    variant,
                                    chain_length: chain.len() - 1,
                                    values: to_remove,
                                },
                                proof: Some(ProofCertificate::Aic {
                                    chain: aic_chain,
                                    link_types: aic_links,
                                }),
                            });
                        }
                    }
                }

                // Follow weak inferences (derived on-the-fly from fabric)
                let neighbors = weak_inferences(fab, current);
                for next in neighbors {
                    if !super::search_budget::tick() {
                        return None;
                    }
                    if chain.contains(&next) && next != start {
                        continue;
                    }
                    if single_value_only && next.1 != start.1 {
                        continue;
                    }

                    if next != start && !visited.contains(&(next, false)) {
                        let mut new_chain = chain.clone();
                        new_chain.push(next);
                        queue.push_back((next, false, new_chain));
                    }
                }
            } else {
                // Follow strong links
                if let Some(neighbors) = graph.strong.get(&current) {
                    for &next in neighbors {
                        if !super::search_budget::tick() {
                            return None;
                        }
                        if chain.contains(&next) && next != start {
                            continue;
                        }
                        if single_value_only && next.1 != start.1 {
                            continue;
                        }
                        if !visited.contains(&(next, true)) {
                            let mut new_chain = chain.clone();
                            new_chain.push(next);
                            queue.push_back((next, true, new_chain));
                        }
                    }
                }
            }
        }
    }
    None
}

// ==================== 3D Medusa ====================

/// 3D Medusa: AIC subclass using 2-coloring on the strong-link subgraph.
///
/// RETIRED by community consensus (StrmCkr / Players Forum): strong-link-only
/// AIC coloring, fully subsumed by general AIC. Retained for SE rating
/// compatibility (SE 5.0).
///
/// Medusa is equivalent to finding contradictions/eliminations via coloring
/// connected components of the AIC polarity graph's strong links:
/// - Contradiction: same color has two digits in one cell, or same digit+color in one unit
/// - Elimination: uncolored candidate sees both colors of its digit
#[deprecated(note = "Subsumed by general AIC. Retained for SE rating compatibility (SE 5.0).")]
#[allow(deprecated)]
pub fn find_medusa(fab: &CandidateFabric, graph: &LinkGraph) -> Option<Finding> {
    let mut globally_visited: HashSet<Node> = HashSet::new();

    for idx in 0..81 {
        if !super::search_budget::tick() {
            return None;
        }
        if fab.values[idx].is_some() {
            continue;
        }
        for start_digit in fab.cell_cands[idx].iter() {
            if !super::search_budget::tick() {
                return None;
            }
            let start = (idx, start_digit);
            if globally_visited.contains(&start) {
                continue;
            }

            // BFS coloring: color 0 and color 1
            let mut color: HashMap<Node, u8> = HashMap::new();
            let mut queue: VecDeque<(Node, u8)> = VecDeque::new();

            color.insert(start, 0);
            queue.push_back((start, 0));

            while let Some((node, c)) = queue.pop_front() {
                globally_visited.insert(node);
                let opp = 1 - c;

                // Follow strong links only for coloring
                if let Some(neighbors) = graph.strong.get(&node) {
                    for &next in neighbors {
                        if !super::search_budget::tick() {
                            return None;
                        }
                        if let std::collections::hash_map::Entry::Vacant(e) = color.entry(next) {
                            e.insert(opp);
                            queue.push_back((next, opp));
                        }
                    }
                }
            }

            if color.len() < 4 {
                continue;
            }

            // Check for contradictions in each color
            for check_color in 0..=1u8 {
                if !super::search_budget::tick() {
                    return None;
                }
                let colored: Vec<Node> = color
                    .iter()
                    .filter(|(_, &c)| c == check_color)
                    .map(|(&k, _)| k)
                    .collect();

                let mut contradiction = false;
                'outer: for i in 0..colored.len() {
                    for j in (i + 1)..colored.len() {
                        if !super::search_budget::tick() {
                            return None;
                        }
                        let (c1, d1) = colored[i];
                        let (c2, d2) = colored[j];
                        // Rule 1: Same digit, same unit
                        if d1 == d2 && fab.sees(c1, c2) {
                            contradiction = true;
                            break 'outer;
                        }
                        // Rule 2: Same cell, different digits
                        if c1 == c2 {
                            contradiction = true;
                            break 'outer;
                        }
                    }
                }

                if contradiction {
                    // Eliminate all candidates of this color
                    for &(cell, digit) in &colored {
                        if !super::search_budget::tick() {
                            return None;
                        }
                        if fab.values[cell].is_none() && fab.cell_cands[cell].contains(digit) {
                            let involved: Vec<usize> = colored.iter().map(|&(c, _)| c).collect();
                            let medusa_chain: Vec<(usize, u8, Polarity)> = color
                                .iter()
                                .map(|(&(c, d), &clr)| {
                                    let pol = if clr == 0 {
                                        Polarity::On
                                    } else {
                                        Polarity::Off
                                    };
                                    (c, d, pol)
                                })
                                .collect();
                            let medusa_links: Vec<LinkType> =
                                vec![LinkType::Strong; medusa_chain.len().saturating_sub(1)];
                            return Some(Finding {
                                technique: Technique::ThreeDMedusa,
                                inference: InferenceResult::Elimination {
                                    cell,
                                    values: vec![digit],
                                },
                                involved_cells: involved,
                                explanation: ExplanationData::Chain {
                                    variant: "3D Medusa".into(),
                                    chain_length: color.len(),
                                    values: vec![digit],
                                },
                                proof: Some(ProofCertificate::Aic {
                                    chain: medusa_chain,
                                    link_types: medusa_links,
                                }),
                            });
                        }
                    }
                }
            }

            // Rule 5: Uncolored candidate sees both colors of same digit
            for idx2 in 0..81 {
                if !super::search_budget::tick() {
                    return None;
                }
                if fab.values[idx2].is_some() {
                    continue;
                }
                for digit in fab.cell_cands[idx2].iter() {
                    if !super::search_budget::tick() {
                        return None;
                    }
                    if color.contains_key(&(idx2, digit)) {
                        continue;
                    }

                    let sees_color_0 = color
                        .iter()
                        .any(|(&(c, d), &clr)| clr == 0 && d == digit && fab.sees(idx2, c));
                    let sees_color_1 = color
                        .iter()
                        .any(|(&(c, d), &clr)| clr == 1 && d == digit && fab.sees(idx2, c));

                    if sees_color_0 && sees_color_1 {
                        let involved: Vec<usize> = color.keys().map(|&(c, _)| c).collect();
                        let medusa_chain: Vec<(usize, u8, Polarity)> = color
                            .iter()
                            .map(|(&(c, d), &clr)| {
                                let pol = if clr == 0 {
                                    Polarity::On
                                } else {
                                    Polarity::Off
                                };
                                (c, d, pol)
                            })
                            .collect();
                        let medusa_links: Vec<LinkType> =
                            vec![LinkType::Strong; medusa_chain.len().saturating_sub(1)];
                        return Some(Finding {
                            technique: Technique::ThreeDMedusa,
                            inference: InferenceResult::Elimination {
                                cell: idx2,
                                values: vec![digit],
                            },
                            involved_cells: involved,
                            explanation: ExplanationData::Chain {
                                variant: "3D Medusa".into(),
                                chain_length: color.len(),
                                values: vec![digit],
                            },
                            proof: Some(ProofCertificate::Aic {
                                chain: medusa_chain,
                                link_types: medusa_links,
                            }),
                        });
                    }
                }
            }
        }
    }
    None
}

// ==================== Forcing Chains ====================
// These use Grid-level propagation rather than CandidateFabric.

/// Nishio Forcing Chain: if assuming a candidate leads to contradiction, eliminate it.
pub fn find_nishio_fc(
    grid: &Grid,
    propagate: &dyn Fn(&Grid, Position, u8) -> (Grid, bool),
) -> Option<Finding> {
    let mut cells: Vec<Position> = grid.empty_positions();
    cells.sort_by_key(|&p| grid.get_candidates(p).count());

    for &pos in &cells {
        if !super::search_budget::tick() {
            return None;
        }
        let cands = grid.get_candidates(pos);
        if cands.count() < 2 || cands.count() > 4 {
            continue;
        }
        for val in cands.iter() {
            if !super::search_budget::tick() {
                return None;
            }
            let (_, contradiction) = propagate(grid, pos, val);
            if contradiction {
                let cell = pos.row * 9 + pos.col;
                return Some(Finding {
                    technique: Technique::NishioForcingChain,
                    inference: InferenceResult::Elimination {
                        cell,
                        values: vec![val],
                    },
                    involved_cells: vec![cell],
                    explanation: ExplanationData::ForcingChain {
                        variant: "Nishio Forcing Chain".into(),
                        source_cell: cell,
                    },
                    proof: Some(ProofCertificate::Forcing {
                        source: ForcingSource::Nishio { cell, digit: val },
                        branches: 1,
                    }),
                });
            }
        }
    }
    None
}

/// Kraken Fish: finned fish verified via forcing chain propagation.
pub fn find_kraken_fish(
    grid: &Grid,
    propagate: &dyn Fn(&Grid, Position, u8) -> (Grid, bool),
) -> Option<Finding> {
    for digit in 1..=9u8 {
        if !super::search_budget::tick() {
            return None;
        }
        for r1 in 0..9 {
            if !super::search_budget::tick() {
                return None;
            }
            for r2 in (r1 + 1)..9 {
                if !super::search_budget::tick() {
                    return None;
                }
                let row1_cols: Vec<usize> = (0..9)
                    .filter(|&c| {
                        let p = Position::new(r1, c);
                        grid.cell(p).is_empty() && grid.get_candidates(p).contains(digit)
                    })
                    .collect();
                let row2_cols: Vec<usize> = (0..9)
                    .filter(|&c| {
                        let p = Position::new(r2, c);
                        grid.cell(p).is_empty() && grid.get_candidates(p).contains(digit)
                    })
                    .collect();

                let common_cols: Vec<usize> = row1_cols
                    .iter()
                    .filter(|c| row2_cols.contains(c))
                    .copied()
                    .collect();

                if common_cols.len() != 2 {
                    continue;
                }

                let fins: Vec<Position> = row1_cols
                    .iter()
                    .filter(|c| !common_cols.contains(c))
                    .map(|&c| Position::new(r1, c))
                    .chain(
                        row2_cols
                            .iter()
                            .filter(|c| !common_cols.contains(c))
                            .map(|&c| Position::new(r2, c)),
                    )
                    .collect();

                if fins.is_empty() || fins.len() > 2 {
                    continue;
                }

                let targets: Vec<Position> = common_cols
                    .iter()
                    .flat_map(|&c| {
                        (0..9)
                            .filter(move |&r| r != r1 && r != r2)
                            .map(move |r| Position::new(r, c))
                    })
                    .filter(|&p| grid.cell(p).is_empty() && grid.get_candidates(p).contains(digit))
                    .collect();

                for &target in &targets {
                    if !super::search_budget::tick() {
                        return None;
                    }
                    let mut all_fins_eliminate = true;
                    for &fin in &fins {
                        if !super::search_budget::tick() {
                            return None;
                        }
                        let (result, contradiction) = propagate(grid, fin, digit);
                        if contradiction {
                            continue;
                        }
                        if result.get(target).is_some() {
                            if result.get(target) == Some(digit) {
                                all_fins_eliminate = false;
                                break;
                            }
                        } else if result.get_candidates(target).contains(digit) {
                            all_fins_eliminate = false;
                            break;
                        }
                    }

                    if all_fins_eliminate {
                        let target_cell = target.row * 9 + target.col;
                        let mut involved = vec![
                            r1 * 9 + common_cols[0],
                            r1 * 9 + common_cols[1],
                            r2 * 9 + common_cols[0],
                            r2 * 9 + common_cols[1],
                        ];
                        for fin in &fins {
                            if !super::search_budget::tick() {
                                return None;
                            }
                            involved.push(fin.row * 9 + fin.col);
                        }
                        involved.push(target_cell);
                        return Some(Finding {
                            technique: Technique::KrakenFish,
                            inference: InferenceResult::Elimination {
                                cell: target_cell,
                                values: vec![digit],
                            },
                            involved_cells: involved,
                            explanation: ExplanationData::ForcingChain {
                                variant: "Kraken Fish".into(),
                                source_cell: target_cell,
                            },
                            proof: Some(ProofCertificate::Forcing {
                                source: ForcingSource::Region { sector: r1, digit },
                                branches: fins.len(),
                            }),
                        });
                    }
                }
            }
        }
    }
    None
}

/// Cell Forcing Chain: all candidates of a cell propagate to the same conclusion.
pub fn find_cell_fc(
    grid: &Grid,
    propagate: &dyn Fn(&Grid, Position, u8) -> (Grid, bool),
) -> Option<Finding> {
    let mut cells: Vec<Position> = grid.empty_positions();
    cells.sort_by_key(|&p| grid.get_candidates(p).count());

    for &pos in &cells {
        if !super::search_budget::tick() {
            return None;
        }
        let cands = grid.get_candidates(pos);
        if cands.count() < 2 || cands.count() > 4 {
            continue;
        }

        let mut branches = Vec::new();
        let mut any_contradiction = false;

        for val in cands.iter() {
            if !super::search_budget::tick() {
                return None;
            }
            let (result, contradiction) = propagate(grid, pos, val);
            if contradiction {
                any_contradiction = true;
                break;
            }
            branches.push(result);
        }

        if any_contradiction || branches.len() < 2 {
            continue;
        }

        let source_cell = pos.row * 9 + pos.col;
        let num_branches = branches.len();

        if let Some(f) = find_common_placement(
            grid,
            pos,
            &branches,
            Technique::CellForcingChain,
            ProofCertificate::Forcing {
                source: ForcingSource::Cell(source_cell),
                branches: num_branches,
            },
        ) {
            return Some(f);
        }
        if let Some(f) = find_common_elimination(
            grid,
            pos,
            &branches,
            Technique::CellForcingChain,
            ProofCertificate::Forcing {
                source: ForcingSource::Cell(source_cell),
                branches: num_branches,
            },
        ) {
            return Some(f);
        }
    }
    None
}

/// Region Forcing Chain: all positions of a digit in a sector propagate to the same conclusion.
pub fn find_region_fc(
    grid: &Grid,
    propagate: &dyn Fn(&Grid, Position, u8) -> (Grid, bool),
) -> Option<Finding> {
    for unit in 0..27 {
        if !super::search_budget::tick() {
            return None;
        }
        let positions: Vec<Position> = if unit < 9 {
            (0..9).map(|c| Position::new(unit, c)).collect()
        } else if unit < 18 {
            (0..9).map(|r| Position::new(r, unit - 9)).collect()
        } else {
            let box_idx = unit - 18;
            let br = (box_idx / 3) * 3;
            let bc = (box_idx % 3) * 3;
            (0..9)
                .map(|i| Position::new(br + i / 3, bc + i % 3))
                .collect()
        };

        for digit in 1..=9u8 {
            if !super::search_budget::tick() {
                return None;
            }
            let digit_cells: Vec<Position> = positions
                .iter()
                .filter(|&&p| grid.cell(p).is_empty() && grid.get_candidates(p).contains(digit))
                .copied()
                .collect();

            if digit_cells.len() < 2 || digit_cells.len() > 4 {
                continue;
            }

            let mut branches = Vec::new();
            let mut any_contradiction = false;

            for &pos in &digit_cells {
                if !super::search_budget::tick() {
                    return None;
                }
                let (result, contradiction) = propagate(grid, pos, digit);
                if contradiction {
                    any_contradiction = true;
                    break;
                }
                branches.push(result);
            }

            if any_contradiction || branches.len() < 2 {
                continue;
            }

            let source = digit_cells[0];
            let num_branches = branches.len();

            if let Some(f) = find_common_placement(
                grid,
                source,
                &branches,
                Technique::RegionForcingChain,
                ProofCertificate::Forcing {
                    source: ForcingSource::Region {
                        sector: unit,
                        digit,
                    },
                    branches: num_branches,
                },
            ) {
                return Some(f);
            }
            if let Some(f) = find_common_elimination(
                grid,
                source,
                &branches,
                Technique::RegionForcingChain,
                ProofCertificate::Forcing {
                    source: ForcingSource::Region {
                        sector: unit,
                        digit,
                    },
                    branches: num_branches,
                },
            ) {
                return Some(f);
            }
        }
    }
    None
}

/// Dynamic Forcing Chain: like Cell FC but propagation uses the full technique set.
pub fn find_dynamic_fc(
    grid: &Grid,
    propagate_full: &dyn Fn(&Grid, Position, u8) -> (Grid, bool),
) -> Option<Finding> {
    let mut cells: Vec<Position> = grid.empty_positions();
    cells.sort_by_key(|&p| grid.get_candidates(p).count());

    for &pos in &cells {
        if !super::search_budget::tick() {
            return None;
        }
        let cands = grid.get_candidates(pos);
        if cands.count() < 2 || cands.count() > 3 {
            continue;
        }

        let mut branches = Vec::new();

        for val in cands.iter() {
            if !super::search_budget::tick() {
                return None;
            }
            let (result, contradiction) = propagate_full(grid, pos, val);
            if contradiction {
                let cell = pos.row * 9 + pos.col;
                return Some(Finding {
                    technique: Technique::DynamicForcingChain,
                    inference: InferenceResult::Elimination {
                        cell,
                        values: vec![val],
                    },
                    involved_cells: vec![cell],
                    explanation: ExplanationData::ForcingChain {
                        variant: "Dynamic Forcing Chain".into(),
                        source_cell: cell,
                    },
                    proof: Some(ProofCertificate::Forcing {
                        source: ForcingSource::Nishio { cell, digit: val },
                        branches: 1,
                    }),
                });
            }
            branches.push(result);
        }

        if branches.len() < 2 {
            continue;
        }

        let dfc_cell = pos.row * 9 + pos.col;
        let dfc_branches = branches.len();

        if let Some(f) = find_common_placement(
            grid,
            pos,
            &branches,
            Technique::DynamicForcingChain,
            ProofCertificate::Forcing {
                source: ForcingSource::Cell(dfc_cell),
                branches: dfc_branches,
            },
        ) {
            return Some(f);
        }
        if let Some(f) = find_common_elimination(
            grid,
            pos,
            &branches,
            Technique::DynamicForcingChain,
            ProofCertificate::Forcing {
                source: ForcingSource::Cell(dfc_cell),
                branches: dfc_branches,
            },
        ) {
            return Some(f);
        }
    }
    None
}

// ==================== Forcing Chain Helpers ====================

/// Find a common placement across all propagation branches.
fn find_common_placement(
    grid: &Grid,
    source: Position,
    branches: &[Grid],
    technique: Technique,
    proof: ProofCertificate,
) -> Option<Finding> {
    for target in grid.empty_positions() {
        if !super::search_budget::tick() {
            return None;
        }
        if target == source || grid.get(target).is_some() {
            continue;
        }
        let mut common_val: Option<u8> = None;
        let mut all_agree = true;
        for branch in branches {
            if !super::search_budget::tick() {
                return None;
            }
            if let Some(v) = branch.get(target) {
                match common_val {
                    None => common_val = Some(v),
                    Some(cv) if cv != v => {
                        all_agree = false;
                        break;
                    }
                    _ => {}
                }
            } else {
                all_agree = false;
                break;
            }
        }
        if all_agree {
            if let Some(val) = common_val {
                let source_cell = source.row * 9 + source.col;
                let target_cell = target.row * 9 + target.col;
                return Some(Finding {
                    technique,
                    inference: InferenceResult::Placement {
                        cell: target_cell,
                        value: val,
                    },
                    involved_cells: vec![source_cell, target_cell],
                    explanation: ExplanationData::ForcingChain {
                        variant: match technique {
                            Technique::CellForcingChain => "Cell Forcing Chain".into(),
                            Technique::RegionForcingChain => "Region Forcing Chain".into(),
                            Technique::DynamicForcingChain => "Dynamic Forcing Chain".into(),
                            _ => "Forcing Chain".into(),
                        },
                        source_cell,
                    },
                    proof: Some(proof),
                });
            }
        }
    }
    None
}

/// Find a common elimination across all propagation branches.
fn find_common_elimination(
    grid: &Grid,
    source: Position,
    branches: &[Grid],
    technique: Technique,
    proof: ProofCertificate,
) -> Option<Finding> {
    for target in grid.empty_positions() {
        if !super::search_budget::tick() {
            return None;
        }
        if target == source {
            continue;
        }
        let orig_cands = grid.get_candidates(target);
        if orig_cands.count() < 2 {
            continue;
        }
        for val in orig_cands.iter() {
            if !super::search_budget::tick() {
                return None;
            }
            let mut all_eliminate = true;
            for branch in branches {
                if !super::search_budget::tick() {
                    return None;
                }
                if let Some(placed) = branch.get(target) {
                    if placed == val {
                        all_eliminate = false;
                        break;
                    }
                } else if branch.get_candidates(target).contains(val) {
                    all_eliminate = false;
                    break;
                }
            }
            if all_eliminate {
                let source_cell = source.row * 9 + source.col;
                let target_cell = target.row * 9 + target.col;
                return Some(Finding {
                    technique,
                    inference: InferenceResult::Elimination {
                        cell: target_cell,
                        values: vec![val],
                    },
                    involved_cells: vec![source_cell, target_cell],
                    explanation: ExplanationData::ForcingChain {
                        variant: match technique {
                            Technique::CellForcingChain => "Cell Forcing Chain".into(),
                            Technique::RegionForcingChain => "Region Forcing Chain".into(),
                            Technique::DynamicForcingChain => "Dynamic Forcing Chain".into(),
                            _ => "Forcing Chain".into(),
                        },
                        source_cell,
                    },
                    proof: Some(proof),
                });
            }
        }
    }
    None
}
