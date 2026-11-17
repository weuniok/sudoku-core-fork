use super::teaching::*;
use super::teaching_contract::{TeachingCandidate, TeachingHint, TeachingSearchResult};
use crate::{Grid, Position};
use serde_json::{json, Value};
use std::collections::BTreeSet;

fn cases() -> Vec<(Value, u32)> {
    let first: Value =
        serde_json::from_str(include_str!("../../tests/fixtures/teaching-hints.json")).unwrap();
    let expanded: Value = serde_json::from_str(include_str!(
        "../../tests/fixtures/expanded-teaching-hints.json"
    ))
    .unwrap();
    let mut result: Vec<_> = first["cases"]
        .as_array()
        .unwrap()
        .iter()
        .map(|c| (c.clone(), 200_000))
        .collect();
    for (id, found) in first["results"].as_object().unwrap() {
        result.push((
            json!({"techniqueId":id,"position":first["position"],"result":found}),
            200_000,
        ));
    }
    result.extend(
        expanded["cases"]
            .as_array()
            .unwrap()
            .iter()
            .map(|c| (c.clone(), 2_000_000)),
    );
    result
}
fn request(case: &Value, limit: u32, allowed: &[&str]) -> TeachingSearchResult {
    search_teaching_hint_with_options(
        case["position"]["puzzle"].as_str().unwrap(),
        case["position"]["values"].as_str().unwrap(),
        &serde_json::from_value::<Vec<u16>>(case["position"]["exclusions"].clone()).unwrap(),
        case["techniqueId"].as_str().unwrap(),
        limit,
        allowed,
    )
    .unwrap()
}

#[test]
fn all_forty_teaching_methods_have_deterministic_positive_wire_fixtures() {
    let mut covered = BTreeSet::new();
    for (case, limit) in cases() {
        let before = case.clone();
        for _ in 0..3 {
            let result = request(&case, limit, TEACHING_TECHNIQUES);
            assert_eq!(
                serde_json::to_value(&result).unwrap(),
                case["result"],
                "{}",
                case["techniqueId"]
            );
            assert!(result.work_used <= limit);
            for h in result.hints {
                covered.insert(h.technique_id.clone());
                assert_eq!(h.schema_version, 2);
                assert!(h.proof.branches.len() <= 9);
                for branch in &h.proof.branches {
                    assert!(branch.steps.len() <= 256);
                    assert!(branch.steps.iter().all(|s| !is_forcing(&s.technique_id)));
                }
                if let Some(answer) = case["solution"].as_str() {
                    assert!(Grid::from_string(answer).unwrap().is_complete());
                    for p in h.placements {
                        assert_eq!(answer.as_bytes()[p.cell as usize] - b'0', p.digit);
                    }
                    for e in h.eliminations {
                        assert_ne!(answer.as_bytes()[e.cell as usize] - b'0', e.digit);
                    }
                }
            }
        }
        assert_eq!(case, before);
    }
    assert_eq!(
        covered,
        TEACHING_TECHNIQUES.iter().map(|s| s.to_string()).collect()
    );
}

#[test]
fn completed_positions_are_negative_for_every_technique_and_unsupported_names_are_explicit() {
    let solution =
        "534678912672195348198342567859761423426853791713924856961537284287419635345286179";
    for id in TEACHING_TECHNIQUES {
        let r = search_teaching_hint(solution, solution, &[0; 81], id, 2_000_000).unwrap();
        assert_eq!(r.status, "not_found", "{id}");
        assert!(r.hints.is_empty());
    }
    for id in [
        "backtracking",
        "arithmetic_counting",
        "three_d_medusa",
        "aligned_pair_exclusion",
        "aligned_triplet_exclusion",
        "unknown",
    ] {
        assert_eq!(
            search_teaching_hint(solution, solution, &[0; 81], id, 10)
                .unwrap()
                .status,
            "unsupported"
        );
    }
}

#[test]
fn disabled_named_deductions_cannot_be_hidden_inside_forcing_proofs() {
    let case = cases()
        .into_iter()
        .find(|(c, _)| c["techniqueId"] == "cell_forcing_chain")
        .unwrap()
        .0;
    let without = request(&case, 200_000, &["cell_forcing_chain"]);
    assert_eq!(without.status, "not_found");
    assert!(without.hints.is_empty());
    let with = request(&case, 200_000, &["cell_forcing_chain", "naked_single"]);
    assert_eq!(with.status, "found");
    assert!(with.hints[0]
        .proof
        .branches
        .iter()
        .all(|b| b.steps.iter().all(|s| s.technique_id == "naked_single")));
    let short = request(&case, 1, &["cell_forcing_chain", "naked_single"]);
    assert_eq!(short.status, "budget_exhausted");
    assert!(short.hints.is_empty());
}

#[test]
fn uniqueness_is_verified_from_original_clues_and_budget_exhaustion_is_not_a_proof() {
    let blank = "0".repeat(81);
    for id in [
        "unique_rectangle",
        "hidden_rectangle",
        "avoidable_rectangle",
        "extended_unique_rectangle",
        "bug",
    ] {
        assert_eq!(
            search_teaching_hint(&blank, &blank, &[0; 81], id, 2_000_000)
                .unwrap()
                .status,
            "non_unique_puzzle"
        );
        assert_eq!(
            search_teaching_hint(&blank, &blank, &[0; 81], id, 1)
                .unwrap()
                .status,
            "budget_exhausted"
        );
    }
}

#[test]
fn xy_and_xyz_names_and_w_wing_implications_follow_the_actual_pattern() {
    let blank = "0".repeat(81);
    let mut masks = vec![0; 81];
    for (c, m) in [(0, 3), (4, 5), (36, 6)] {
        masks[c] = 511 ^ m;
    }
    assert_eq!(
        search_teaching_hint(&blank, &blank, &masks, "xy_wing", 200_000)
            .unwrap()
            .status,
        "found"
    );
    assert_eq!(
        search_teaching_hint(&blank, &blank, &masks, "xyz_wing", 200_000)
            .unwrap()
            .status,
        "not_found"
    );
    let case = cases()
        .into_iter()
        .find(|(c, _)| c["techniqueId"] == "w_wing")
        .unwrap()
        .0;
    let hint = request(&case, 200_000, TEACHING_TECHNIQUES).hints.remove(0);
    assert_eq!(hint.proof.nodes.len(), 6);
    for (i, node) in hint.proof.nodes.iter().enumerate() {
        assert_eq!(node.polarity, if i % 2 == 0 { "off" } else { "on" });
    }
    let original = Grid::from_string(&blank).unwrap();
    // An exhausted interactive request must not leave a scope on this thread.
    let before = crate::Solver::new().count_solutions(&original, 2);
    search_teaching_hint(&blank, &blank, &masks, "aic", 1).unwrap();
    assert_eq!(crate::Solver::new().count_solutions(&original, 2), before);
    assert_eq!(original.get(Position::new(0, 0)), None);
}

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
    assert_eq!(
        hint.placements,
        vec![TeachingCandidate { cell: 0, digit: 1 }]
    );
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
    assert_eq!(
        hint.eliminations,
        vec![TeachingCandidate { cell: 40, digit: 3 }]
    );
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
