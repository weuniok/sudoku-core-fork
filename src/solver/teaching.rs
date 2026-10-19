//! Versioned, read-only mobile teaching API. Technique priority belongs to the
//! caller; this dispatcher never invokes the globally preferred hint or guesses.
use super::explain::{
    ExplanationData, Finding, InferenceResult, LinkType, Polarity, ProofCertificate,
};
use super::fabric::{sector_cells, CandidateFabric};
use super::{aic_engine, als_engine, basic, fish_engine, Technique};
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
pub struct TeachingProof {
    pub kind: String,
    pub nodes: Vec<TeachingNode>,
    pub links: Vec<TeachingLink>,
    pub groups: Vec<TeachingGroup>,
    pub rcc_digits: Vec<u8>,
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
    if puzzle.len() != 81
        || values.len() != 81
        || !puzzle.bytes().all(|b| b.is_ascii_digit())
        || !values.bytes().all(|b| b.is_ascii_digit())
        || exclusions.len() != 81
        || exclusions.iter().any(|&m| m > 511)
        || work_limit == 0
        || work_limit > 2_000_000
        || !TEACHING_TECHNIQUES.contains(&technique)
    {
        return Err("Invalid hint request".into());
    }
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
    let (finding, work, exhausted) = match technique {
        "naked_single" => (basic::find_naked_single(&fab), 0, false),
        "hidden_single" => (basic::find_hidden_single(&fab), 0, false),
        "locked_candidates" => (
            fish_engine::find_pointing_pair(&fab)
                .or_else(|| fish_engine::find_box_line_reduction(&fab)),
            0,
            false,
        ),
        "naked_pair" => (basic::find_naked_subset(&fab, 2), 0, false),
        "x_wing" => (fish_engine::find_basic_fish(&fab, 2), 0, false),
        "xy_wing" => als_engine::find_wing_bounded(&fab, Technique::XYWing, work_limit),
        "wxyz_wing" => als_engine::find_wing_bounded(&fab, Technique::WXYZWing, work_limit),
        "aic" => aic_engine::find_aic_bounded(&fab, work_limit),
        _ => unreachable!(),
    };
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
    Ok(TeachingSearchResult {
        status: "found".into(),
        work_used: work,
        hints: vec![adapt(&finding, &fab, technique, position_key)?],
    })
}

fn adapt(
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
        kind: "basic".into(),
        nodes: vec![],
        links: vec![],
        groups: vec![],
        rcc_digits: vec![],
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
            ..
        }) => {
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
        _ => return Err("Unsupported teaching proof".into()),
    }
    sources.sort_unstable();
    sources.dedup();
    candidates.sort_by_key(|c| (c.cell, c.digit));
    candidates.dedup();
    let pattern = format!("{sources:?}:{candidates:?}:{placements:?}:{eliminations:?}:{proof:?}");
    let key = format!(
        "{position_key}:{technique}:{:x}",
        Sha256::digest(pattern.as_bytes())
    );
    Ok(TeachingHint {
        schema_version: 1,
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
            search_teaching_hint(&blank, &blank, &[0; 81], "naked_single", 1)
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
        assert!(search_teaching_hint(&blank, &blank, &[0; 81], "backtracking", 1).is_err());
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
