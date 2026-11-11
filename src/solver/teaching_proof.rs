//! Convert solver findings into versioned teaching evidence.

use super::explain::{
    ExplanationData, Finding, InferenceResult, LinkType, Polarity, ProofCertificate,
};
use super::fabric::{sector_cells, CandidateFabric};
use super::teaching_contract::{
    TeachingCandidate, TeachingConjugate, TeachingEvidence, TeachingGroup, TeachingHint,
    TeachingHouse, TeachingLink, TeachingNode, TeachingProof, TeachingRelation, TeachingUniqueness,
};
use sha2::{Digest, Sha256};

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
    let mut evidence = TeachingEvidence {
        kind: "basic".into(),
        nodes: vec![],
        links: vec![],
        groups: vec![],
        rcc_digits: vec![],
        fins: vec![],
        group_roles: vec![],
        relations: vec![],
        uniqueness: None,
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
            evidence.fins = fins.iter().map(|&cell| candidate(cell, *digit)).collect();
            evidence.kind = if technique == "locked_candidates" {
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
            evidence.kind = "als".into();
            evidence.rcc_digits = rcc_values.clone();
            for group in als_chain {
                evidence.groups.push(TeachingGroup {
                    cells: group.cells.iter().map(|&c| c as u8).collect(),
                    digits: group.candidates.clone(),
                    house: group.sector as u8,
                });
                for &cell in &group.cells {
                    candidates.extend(fab.cell_cands[cell].iter().map(|d| candidate(cell, d)));
                }
                houses.push(house(group.sector, "source"));
            }
            evidence.group_roles = (0..als_chain.len())
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
                evidence.relations = rcc_values
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
                        evidence.relations.push(TeachingRelation {
                            from: 0,
                            to: i as u16,
                            digit,
                            kind: "partition".into(),
                        });
                    }
                }
            } else {
                evidence.relations = rcc_values
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
            evidence.kind = "chain".into();
            for (id, &(cell, digit, polarity)) in chain.iter().enumerate() {
                let member = candidate(cell, digit);
                candidates.push(member.clone());
                evidence.nodes.push(TeachingNode {
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
            evidence.links = link_types
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
            evidence.kind = "uniqueness".into();
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
            evidence.uniqueness = Some(TeachingUniqueness {
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
            evidence.kind = "forcing".into();
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
                    evidence.fins = finding.involved_cells[4..finding.involved_cells.len() - 1]
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
    let proof = TeachingProof {
        evidence,
        branches: vec![],
    };
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
