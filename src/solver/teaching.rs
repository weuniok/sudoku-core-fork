//! Versioned, read-only mobile teaching API. Technique priority belongs to the
//! caller; this dispatcher never invokes the globally preferred hint or guesses.
use super::explain::{
    ExplanationData, Finding, InferenceResult, LinkType, Polarity, ProofCertificate,
};
use super::fabric::{sector_cells, CandidateFabric};
use super::{aic_engine, als_engine, basic, fish_engine, uniqueness, Technique};
use crate::{Grid, Position};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

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

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TeachingCandidate {
    pub cell: u8,
    pub digit: u8,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TeachingHouse {
    pub kind: String,
    pub index: u8,
    pub role: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TeachingNode {
    pub id: u16,
    pub kind: String,
    pub members: Vec<TeachingCandidate>,
    pub polarity: String,
    pub meaning: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TeachingLink {
    pub from: u16,
    pub to: u16,
    pub kind: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TeachingGroup {
    pub cells: Vec<u8>,
    pub digits: Vec<u8>,
    pub house: u8,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TeachingEvidence {
    pub kind: String,
    pub nodes: Vec<TeachingNode>,
    pub links: Vec<TeachingLink>,
    pub groups: Vec<TeachingGroup>,
    pub rcc_digits: Vec<u8>,
    pub fins: Vec<TeachingCandidate>,
    pub group_roles: Vec<String>,
    pub relations: Vec<TeachingRelation>,
    pub uniqueness: Option<TeachingUniqueness>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TeachingRelation {
    pub from: u16,
    pub to: u16,
    pub digit: u8,
    pub kind: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TeachingUniqueness {
    pub rule: String,
    pub floor_cells: Vec<u8>,
    pub roof_cells: Vec<u8>,
    pub digits: Vec<u8>,
    pub auxiliary_cells: Vec<u8>,
    pub conjugates: Vec<TeachingConjugate>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TeachingConjugate {
    pub house: u8,
    pub digit: u8,
    pub cells: Vec<u8>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TeachingProof {
    #[serde(flatten)]
    pub evidence: TeachingEvidence,
    pub branches: Vec<TeachingBranch>,
}
impl std::ops::Deref for TeachingProof {
    type Target = TeachingEvidence;
    fn deref(&self) -> &Self::Target {
        &self.evidence
    }
}
impl std::ops::DerefMut for TeachingProof {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.evidence
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TeachingDeduction {
    pub technique_id: String,
    pub source_cells: Vec<u8>,
    pub candidates: Vec<TeachingCandidate>,
    pub placements: Vec<TeachingCandidate>,
    pub eliminations: Vec<TeachingCandidate>,
    pub houses: Vec<TeachingHouse>,
    pub proof: TeachingEvidence,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TeachingBranch {
    pub assumption: TeachingCandidate,
    pub steps: Vec<TeachingDeduction>,
    pub contradiction: Option<TeachingContradiction>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TeachingContradiction {
    pub kind: String,
    pub cells: Vec<u8>,
    pub house: Option<u8>,
    pub digit: Option<u8>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TeachingHint {
    pub schema_version: u32,
    pub technique_id: String,
    pub variant: String,
    pub position_key: String,
    pub key: String,
    pub source_cells: Vec<u8>,
    pub candidates: Vec<TeachingCandidate>,
    pub placements: Vec<TeachingCandidate>,
    pub eliminations: Vec<TeachingCandidate>,
    pub houses: Vec<TeachingHouse>,
    pub proof: TeachingProof,
    pub explanation: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TeachingSearchResult {
    pub status: String,
    pub hints: Vec<TeachingHint>,
    pub work_used: u32,
}

fn result(status: &str, work_used: u32) -> TeachingSearchResult {
    TeachingSearchResult {
        status: status.into(),
        hints: vec![],
        work_used,
    }
}

fn candidate(cell: usize, digit: u8) -> TeachingCandidate {
    TeachingCandidate {
        cell: cell as u8,
        digit,
    }
}

fn house(sector: usize, role: &str) -> TeachingHouse {
    TeachingHouse {
        kind: if sector < 9 {
            "row"
        } else if sector < 18 {
            "column"
        } else {
            "box"
        }
        .into(),
        index: (sector % 9) as u8,
        role: role.into(),
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
    let mut hint = adapt(&finding, &fab, technique, position_key)?;
    hint.proof.branches = branches;
    if !hint.proof.branches.is_empty() {
        hint.key = format!(
            "{}:{}:{:x}",
            hint.position_key,
            technique,
            Sha256::digest(format!("{}:{:?}", hint.key, hint.proof.branches).as_bytes())
        );
        for branch in &hint.proof.branches {
            hint.candidates.push(branch.assumption.clone());
        }
        hint.candidates.sort_by_key(|c| (c.cell, c.digit));
        hint.candidates.dedup();
    }
    Ok(TeachingSearchResult {
        status: "found".into(),
        work_used: work,
        hints: vec![hint],
    })
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

pub(super) fn adapt(
    finding: &Finding,
    fab: &CandidateFabric,
    technique: &str,
    position_key: String,
) -> Result<TeachingHint, String> {
    let (placements, eliminations) = match &finding.inference {
        InferenceResult::Placement { cell, value } => (vec![candidate(*cell, *value)], vec![]),
        InferenceResult::Elimination { cell, values } => (
            vec![],
            values.iter().map(|&v| candidate(*cell, v)).collect(),
        ),
    };
    let mut sources = finding.involved_cells.clone();
    let mut houses = vec![];
    let mut candidates = vec![];
    let mut variant = finding.technique.to_string();
    let mut proof = TeachingProof {
        branches: vec![],
        evidence: TeachingEvidence {
            kind: "basic".into(),
            nodes: vec![],
            links: vec![],
            groups: vec![],
            rcc_digits: vec![],
            fins: vec![],
            group_roles: vec![],
            relations: vec![],
            uniqueness: None,
        },
    };
    match &finding.explanation {
        ExplanationData::HiddenSingle {
            cell,
            value,
            sector,
            ..
        } => {
            houses.push(house(*sector, "source"));
            candidates.push(candidate(*cell, *value));
        }
        ExplanationData::NakedSingle { cell, value } => candidates.push(candidate(*cell, *value)),
        ExplanationData::LockedSet {
            cells,
            values,
            sector,
            ..
        } => {
            sources = cells.clone();
            houses.push(house(*sector, "source"));
            for &cell in cells {
                for &digit in values {
                    if fab.cell_cands[cell].contains(digit) {
                        candidates.push(candidate(cell, digit));
                    }
                }
            }
        }
        _ => {}
    }
    match &finding.proof {
        Some(ProofCertificate::Basic { .. }) => {}
        Some(ProofCertificate::Fish {
            digit,
            base_sectors,
            cover_sectors,
            fins,
        }) => {
            proof.fins = fins.iter().map(|&cell| candidate(cell, *digit)).collect();
            proof.kind = if technique == "locked_candidates" {
                "intersection"
            } else {
                "fish"
            }
            .into();
            houses.extend(base_sectors.iter().map(|&s| house(s, "base")));
            houses.extend(cover_sectors.iter().map(|&s| house(s, "cover")));
            candidates.extend(
                sources
                    .iter()
                    .filter(|&&c| fab.cell_cands[c].contains(*digit))
                    .map(|&c| candidate(c, *digit)),
            );
        }
        Some(ProofCertificate::Als {
            als_chain,
            rcc_values,
            ..
        }) => {
            proof.kind = "als".into();
            proof.rcc_digits = rcc_values.clone();
            for group in als_chain {
                proof.groups.push(TeachingGroup {
                    cells: group.cells.iter().map(|&c| c as u8).collect(),
                    digits: group.candidates.clone(),
                    house: group.sector as u8,
                });
                for &cell in &group.cells {
                    candidates.extend(fab.cell_cands[cell].iter().map(|d| candidate(cell, d)));
                }
                houses.push(house(group.sector, "source"));
            }
            proof.group_roles = (0..als_chain.len())
                .map(|i| {
                    match (technique, i) {
                        ("sue_de_coq", 0) => "intersection",
                        ("death_blossom", 0) => "stem",
                        _ => "als",
                    }
                    .into()
                })
                .collect();
            if technique == "death_blossom" {
                proof.relations = rcc_values
                    .iter()
                    .enumerate()
                    .map(|(i, &digit)| TeachingRelation {
                        from: 0,
                        to: i as u16 + 1,
                        digit,
                        kind: "stem".into(),
                    })
                    .collect();
            } else if technique == "sue_de_coq" {
                for (i, group) in als_chain.iter().enumerate().skip(1) {
                    for &digit in &group.candidates {
                        proof.relations.push(TeachingRelation {
                            from: 0,
                            to: i as u16,
                            digit,
                            kind: "partition".into(),
                        });
                    }
                }
            } else {
                proof.relations = rcc_values
                    .iter()
                    .enumerate()
                    .map(|(i, &digit)| TeachingRelation {
                        from: i as u16,
                        to: i as u16 + 1,
                        digit,
                        kind: "rcc".into(),
                    })
                    .collect();
            }
        }
        Some(ProofCertificate::Aic { chain, link_types }) => {
            if chain.len() < 2 || link_types.len() + 1 != chain.len() {
                return Err("Incomplete chain proof".into());
            }
            proof.kind = "chain".into();
            for (id, &(cell, digit, polarity)) in chain.iter().enumerate() {
                let member = candidate(cell, digit);
                candidates.push(member.clone());
                proof.nodes.push(TeachingNode {
                    id: id as u16,
                    kind: "candidate".into(),
                    members: vec![member],
                    polarity: if polarity == Polarity::On {
                        "on"
                    } else {
                        "off"
                    }
                    .into(),
                    meaning: "candidate".into(),
                });
            }
            proof.links = link_types
                .iter()
                .enumerate()
                .map(|(i, link)| TeachingLink {
                    from: i as u16,
                    to: i as u16 + 1,
                    kind: if *link == LinkType::Strong {
                        "strong"
                    } else {
                        "weak"
                    }
                    .into(),
                })
                .collect();
            if let ExplanationData::Chain { variant: name, .. } = &finding.explanation {
                variant = name.clone();
            }
        }
        Some(ProofCertificate::Uniqueness {
            pattern,
            floor_cells,
            roof_cells,
        }) => {
            proof.kind = "uniqueness".into();
            let rule = match pattern.as_str() {
                "UR Type 1" => "ur_type_1",
                "UR Type 2" => "ur_type_2",
                "UR Type 3" => "ur_type_3",
                "UR Type 4" => "ur_type_4",
                "UR Type 5" => "ur_type_5",
                "UR Type 6" => "ur_type_6",
                "Hidden Rectangle" => "hidden_rectangle",
                "Avoidable Rectangle" => "avoidable_rectangle",
                "Extended UR" => "extended_unique_rectangle",
                _ if technique == "bug" => "bug",
                _ => return Err("Unsupported uniqueness certificate".into()),
            };
            let mut digits = Vec::new();
            for &cell in &sources {
                if let Some(value) = fab.values[cell] {
                    digits.push(value);
                }
                candidates.extend(fab.cell_cands[cell].iter().map(|d| candidate(cell, d)));
                if floor_cells.contains(&cell) {
                    digits.extend(fab.cell_cands[cell].iter());
                }
            }
            digits.sort_unstable();
            digits.dedup();
            let mut conjugates = vec![];
            for sector in 0..27 {
                if !sources.iter().any(|c| sector_cells(sector).contains(c)) {
                    continue;
                }
                houses.push(house(sector, "source"));
                for digit in 1..=9 {
                    let cells: Vec<u8> = sector_cells(sector)
                        .iter()
                        .copied()
                        .filter(|&c| fab.cell_cands[c].contains(digit))
                        .map(|c| c as u8)
                        .collect();
                    if cells.len() == 2 {
                        conjugates.push(TeachingConjugate {
                            house: sector as u8,
                            digit,
                            cells,
                        });
                    }
                }
            }
            proof.uniqueness = Some(TeachingUniqueness {
                rule: rule.into(),
                floor_cells: floor_cells.iter().map(|&c| c as u8).collect(),
                roof_cells: roof_cells.iter().map(|&c| c as u8).collect(),
                digits,
                auxiliary_cells: sources
                    .iter()
                    .filter(|c| !floor_cells.contains(c) && !roof_cells.contains(c))
                    .map(|&c| c as u8)
                    .collect(),
                conjugates,
            });
        }
        Some(ProofCertificate::Forcing { source, .. }) => {
            proof.kind = "forcing".into();
            if let super::explain::ForcingSource::Region { sector, digit } = source {
                houses.push(house(*sector, "source"));
                candidates.extend(
                    sources
                        .iter()
                        .filter(|&&c| fab.cell_cands[c].contains(*digit))
                        .map(|&c| candidate(c, *digit)),
                );
                if technique == "kraken_fish" && sources.len() >= 5 {
                    let corners = &finding.involved_cells[..4];
                    for &row in &[corners[0] / 9, corners[2] / 9] {
                        houses.push(house(row, "base"));
                    }
                    for &col in &[corners[0] % 9, corners[1] % 9] {
                        houses.push(house(col + 9, "cover"));
                    }
                    proof.fins = finding.involved_cells[4..finding.involved_cells.len() - 1]
                        .iter()
                        .map(|&c| candidate(c, *digit))
                        .collect();
                }
            }
        }
        _ => return Err("Unsupported teaching proof".into()),
    }
    sources.sort_unstable();
    sources.dedup();
    candidates.sort_by_key(|c| (c.cell, c.digit));
    candidates.dedup();
    // Preserve v1 identities for the original eight techniques: protocol growth
    // must not count an already-requested deduction again after upgrading.
    let original = [
        "naked_single",
        "hidden_single",
        "locked_candidates",
        "naked_pair",
        "x_wing",
        "xy_wing",
        "wxyz_wing",
        "aic",
    ]
    .contains(&technique);
    let proof_key = if original {
        format!("TeachingProof {{ kind: {:?}, nodes: {:?}, links: {:?}, groups: {:?}, rcc_digits: {:?} }}", proof.kind, proof.nodes, proof.links, proof.groups, proof.rcc_digits)
    } else {
        format!("{proof:?}")
    };
    let pattern = format!("{sources:?}:{candidates:?}:{placements:?}:{eliminations:?}:{proof_key}");
    let key = format!(
        "{position_key}:{technique}:{:x}",
        Sha256::digest(pattern.as_bytes())
    );
    Ok(TeachingHint {
        schema_version: 2,
        technique_id: technique.into(),
        variant,
        position_key,
        key,
        source_cells: sources.iter().map(|&c| c as u8).collect(),
        candidates,
        placements,
        eliminations,
        houses,
        proof,
        explanation: finding.to_hint().explanation,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ring() -> Vec<u16> {
        let mut masks = vec![0; 81];
        for (cell, digits) in [
            (0, vec![1, 2]),
            (1, vec![1, 3]),
            (2, vec![3, 4]),
            (3, vec![2, 4]),
        ] {
            masks[cell] = 511 ^ digits.iter().fold(0, |m, d| m | (1 << (d - 1)));
        }
        masks
    }

    #[test]
    fn aic_and_wxyz_both_available_and_chain_is_stable_and_structured() {
        let puzzle = "0".repeat(81);
        let exclusions = ring();
        let before = exclusions.clone();
        let aic = search_teaching_hint(&puzzle, &puzzle, &exclusions, "aic", 200_000).unwrap();
        assert_eq!(aic.status, "found");
        let hint = &aic.hints[0];
        assert_eq!(hint.proof.kind, "chain");
        assert_eq!(hint.proof.links.len() + 1, hint.proof.nodes.len());
        assert!(hint.proof.links.iter().any(|l| l.kind == "strong"));
        assert!(hint.proof.links.iter().any(|l| l.kind == "weak"));
        assert_eq!(
            search_teaching_hint(&puzzle, &puzzle, &exclusions, "wxyz_wing", 200_000)
                .unwrap()
                .status,
            "found"
        );
        let wire = serde_json::to_value(hint).unwrap();
        for _ in 0..20 {
            let next = search_teaching_hint(&puzzle, &puzzle, &exclusions, "aic", 200_000).unwrap();
            assert_eq!(serde_json::to_value(&next.hints[0]).unwrap(), wire);
        }
        let roundtrip: TeachingHint = serde_json::from_value(wire).unwrap();
        assert_eq!(roundtrip.proof.nodes.len(), hint.proof.nodes.len());
        assert_eq!(exclusions, before);
    }

    #[test]
    fn direct_search_never_guesses_and_reports_invalid_or_incomplete_positions() {
        let blank = "0".repeat(81);
        assert_eq!(
            search_teaching_hint(&blank, &blank, &[0; 81], "naked_single", 200_000)
                .unwrap()
                .status,
            "not_found"
        );
        assert_eq!(
            search_teaching_hint(&blank, &blank, &ring(), "aic", 1)
                .unwrap()
                .status,
            "budget_exhausted"
        );
        assert_eq!(
            search_teaching_hint(&blank, &blank, &ring(), "wxyz_wing", 1)
                .unwrap()
                .status,
            "budget_exhausted"
        );
        assert_eq!(
            search_teaching_hint(&blank, &blank, &[0; 81], "backtracking", 1)
                .unwrap()
                .status,
            "unsupported"
        );
        let mut dead = vec![0; 81];
        dead[0] = 511;
        assert_eq!(
            search_teaching_hint(&blank, &blank, &dead, "aic", 1)
                .unwrap()
                .status,
            "contradictory"
        );
        let duplicate = format!("11{}", "0".repeat(79));
        assert_eq!(
            search_teaching_hint(&blank, &duplicate, &[0; 81], "aic", 1)
                .unwrap()
                .status,
            "contradictory"
        );
    }

    #[test]
    fn singles_have_numerical_house_metadata_and_preserve_clues() {
        let solved =
            "123456789456789123789123456234567891567891234891234567345678912678912345912345678";
        let puzzle = format!("0{}", &solved[1..]);
        let hint = search_teaching_hint(&puzzle, &puzzle, &[0; 81], "hidden_single", 100)
            .unwrap()
            .hints
            .remove(0);
        assert_eq!(hint.placements, vec![candidate(0, 1)]);
        assert_eq!(hint.houses[0].kind, "row");
        assert_eq!(hint.houses[0].index, 0);
        assert!(search_teaching_hint(solved, &puzzle, &[0; 81], "hidden_single", 100).is_err());
    }

    #[test]
    fn teaching_xy_wing_has_three_bivalue_cells_and_never_labels_a_pair_as_a_wing() {
        let blank = "0".repeat(81);
        let mut exclusions = vec![0u16; 81];
        for (cell, mask) in [(0, 3), (4, 5), (36, 6)] {
            exclusions[cell] = 511 ^ mask;
        }
        let hint = search_teaching_hint(&blank, &blank, &exclusions, "xy_wing", 200_000)
            .unwrap()
            .hints
            .remove(0);
        assert_eq!(hint.source_cells, vec![0, 4, 36]);
        assert_eq!(hint.eliminations, vec![candidate(40, 3)]);
        assert_eq!(hint.proof.kind, "als");

        exclusions.fill(0);
        exclusions[0] = 511 ^ 3;
        exclusions[1] = 511 ^ 3;
        assert_eq!(
            search_teaching_hint(&blank, &blank, &exclusions, "xy_wing", 200_000)
                .unwrap()
                .status,
            "not_found"
        );
        assert_eq!(
            search_teaching_hint(&blank, &blank, &exclusions, "naked_pair", 200_000)
                .unwrap()
                .status,
            "found"
        );
    }
}
