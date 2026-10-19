use super::*;
use explain::ExplanationData;

const EASY: &str =
    "530070000600195000098000060800060003400803001700020006060000280000419005000080079";
const SOLUTION: &str =
    "534678912672195348198342567859761423426853791713924856961537284287419635345286179";

#[test]
fn propagation_reports_duplicate_row_column_and_box_values() {
    for other in [
        Position::new(0, 4),
        Position::new(4, 0),
        Position::new(1, 1),
    ] {
        let mut grid = Grid::new_classic();
        grid.set_given(Position::new(0, 0), 5);
        grid.set_given(other, 5);
        grid.recalculate_candidates();
        assert!(
            backtrack::has_contradiction(&grid),
            "missed duplicate at {other:?}"
        );
        let (_, contradiction) = backtrack::propagate_singles(&grid, Position::new(8, 8), 1);
        assert!(contradiction);
        let (_, contradiction) = propagate_full(&grid, Position::new(8, 8), 1);
        assert!(contradiction);
    }
}

#[test]
fn each_difficulty_accepts_its_maximum_technique_with_the_clue_count_boundary() {
    assert_eq!(
        Solver::technique_to_difficulty(Technique::NakedSingle, 35),
        Difficulty::Beginner
    );
    assert_eq!(
        Solver::technique_to_difficulty(Technique::NakedSingle, 36),
        Difficulty::Easy
    );
    for &difficulty in &Difficulty::all_levels()[2..] {
        assert_eq!(
            Solver::technique_to_difficulty(difficulty.max_technique(), 45),
            difficulty
        );
    }
    assert_eq!(
        Solver::technique_to_difficulty(Technique::ArithmeticCounting, 81),
        Difficulty::Extreme
    );
}

#[test]
fn propagation_solves_correct_assumption_and_rejects_wrong_assumption() {
    let grid = Grid::from_string(EASY).unwrap();
    let before = grid.to_string_compact();
    for propagate in [backtrack::propagate_singles, propagate_full] {
        let (solved, contradiction) = propagate(&grid, Position::new(4, 4), 5);
        assert!(!contradiction);
        assert_eq!(solved.to_string_compact(), SOLUTION);
        assert!(solved.is_complete());
        let (_, contradiction) = propagate(&grid, Position::new(0, 2), 1);
        assert!(contradiction, "incorrect trial value must be rejected");
    }
    assert_eq!(grid.to_string_compact(), before);
}

#[test]
fn propagation_retains_an_unresolved_assumption_without_claiming_a_solution() {
    let grid = Grid::new_classic();
    let (partial, contradiction) = backtrack::propagate_singles(&grid, Position::new(4, 4), 5);
    assert!(!contradiction);
    assert!(!partial.is_complete());
    assert_eq!(partial.empty_count(), 80);
    assert_eq!(partial.get(Position::new(4, 4)), Some(5));
    assert!(!partial.get_candidates(Position::new(4, 0)).contains(5));
    assert!(!partial.get_candidates(Position::new(0, 4)).contains(5));
    assert!(!partial.get_candidates(Position::new(3, 3)).contains(5));
    assert!(partial.get_candidates(Position::new(0, 0)).contains(5));
    assert_eq!(grid.empty_count(), 81);
}

#[test]
fn full_propagation_stops_when_the_remaining_rectangle_is_ambiguous() {
    let mut puzzle = SOLUTION.as_bytes().to_vec();
    for idx in [3, 4, 30, 31, 80] {
        puzzle[idx] = b'.';
    }
    let grid = Grid::from_string(std::str::from_utf8(&puzzle).unwrap()).unwrap();
    let (partial, contradiction) = propagate_full(&grid, Position::new(8, 8), 9);
    assert!(!contradiction);
    assert_eq!(partial.empty_count(), 4);
    assert_eq!(Solver::new().count_solutions(&partial, 3), 2);
    assert!(partial.validate().is_valid);
}

#[test]
fn aic_deductions_preserve_every_completion_of_an_ambiguous_rectangle() {
    let mut puzzle = SOLUTION.as_bytes().to_vec();
    for idx in [3, 4, 30, 31] {
        puzzle[idx] = b'.';
    }
    let grid = Grid::from_string(std::str::from_utf8(&puzzle).unwrap()).unwrap();
    assert_eq!(Solver::new().count_solutions(&grid, 3), 2);
    let mut alternative = SOLUTION.as_bytes().to_vec();
    for idx in [3, 4, 30, 31] {
        alternative[idx] = if alternative[idx] == b'6' { b'7' } else { b'6' };
    }
    let solutions = [
        Grid::from_string(SOLUTION).unwrap(),
        Grid::from_string(std::str::from_utf8(&alternative).unwrap()).unwrap(),
    ];
    assert!(solutions.iter().all(Grid::is_complete));
    let fab = CandidateFabric::from_grid(&grid);
    let graph = aic_engine::build_link_graph(&fab);
    for finding in [
        aic_engine::find_x_chain(&fab, &graph),
        aic_engine::find_aic(&fab, &graph),
    ] {
        if let Some(finding) = finding {
            for solution in &solutions {
                match &finding.inference {
                    InferenceResult::Placement { cell, value } => {
                        assert_eq!(solution.get(idx_to_pos(*cell)), Some(*value), "{finding:?}")
                    }
                    InferenceResult::Elimination { cell, values } => assert!(
                        !values.contains(&solution.get(idx_to_pos(*cell)).unwrap()),
                        "deduction removes a valid completion: {finding:?}"
                    ),
                }
            }
        }
    }
}

#[test]
fn x_chain_and_aic_proofs_have_valid_alternating_endpoint_implications() {
    // The two conjugate row pairs form 1r1c1 = 1r1c4 - 1r4c4 = 1r4c1.
    // Any 1 in column 1 that sees both endpoints can therefore be removed.
    let mut grid = Grid::new_classic();
    for row in [0, 3] {
        for col in 0..9 {
            if col != 0 && col != 3 {
                grid.cell_mut(Position::new(row, col)).remove_candidate(1);
            }
        }
    }
    let fab = CandidateFabric::from_grid(&grid);
    let graph = aic_engine::build_link_graph(&fab);
    for finding in [
        aic_engine::find_x_chain(&fab, &graph),
        aic_engine::find_aic(&fab, &graph),
    ] {
        let finding = finding.expect("conjugate-pair chain should yield an elimination");
        let Some(ProofCertificate::Aic { chain, link_types }) = &finding.proof else {
            panic!("expected chain evidence")
        };
        assert!(chain.len() >= 4);
        assert_eq!(link_types.len(), chain.len() - 1);
        assert_eq!(link_types.first(), Some(&LinkType::Strong));
        assert_eq!(link_types.last(), Some(&LinkType::Strong));
        for (index, pair) in chain.windows(2).enumerate() {
            let ((a, da, polarity), (b, db, _)) = (pair[0], pair[1]);
            assert_eq!(
                polarity,
                if index % 2 == 0 {
                    Polarity::Off
                } else {
                    Polarity::On
                }
            );
            match link_types[index] {
                LinkType::Strong => {
                    assert_eq!(index % 2, 0);
                    assert!(
                        (a == b && da != db && fab.cell_cands[a].count() == 2)
                            || (da == db
                                && fab.cell_sectors[a]
                                    .iter()
                                    .any(|&sector| fab.cell_sectors[b].contains(&sector)
                                        && fab.sector_digit_count[sector][da as usize - 1] == 2)),
                        "certificate claims an unsupported strong link"
                    );
                }
                LinkType::WeakInference => {
                    assert_eq!(index % 2, 1);
                    assert!((a == b && da != db) || (da == db && fab.sees(a, b)));
                }
            }
        }
        assert_eq!(chain.last().unwrap().2, Polarity::On);
        let InferenceResult::Elimination { cell, values } = finding.inference else {
            panic!("expected elimination")
        };
        assert_eq!(values, vec![1]);
        assert!(fab.sees(cell, chain[0].0));
        assert!(fab.sees(cell, chain.last().unwrap().0));
        // Independently enumerate the boolean implications encoded by the proof.
        // No assignment satisfying every XOR/NAND link may leave both endpoints OFF.
        for mask in 0usize..(1 << chain.len()) {
            let on = |idx: usize| mask & (1 << idx) != 0;
            let satisfies_links = link_types.iter().enumerate().all(|(idx, link)| match link {
                LinkType::Strong => on(idx) != on(idx + 1),
                LinkType::WeakInference => !(on(idx) && on(idx + 1)),
            });
            if satisfies_links {
                assert!(
                    on(0) || on(chain.len() - 1),
                    "proof permits both endpoints to be false"
                );
            }
        }
    }
}

#[test]
fn aic_same_cell_endpoints_remove_only_the_unlinked_third_candidate() {
    let mut grid = Grid::new_classic();
    set_candidates(&mut grid, Position::new(0, 0), &[1, 2, 3]);
    set_candidates(&mut grid, Position::new(0, 3), &[1, 2]);
    for col in 1..9 {
        if col != 3 {
            grid.cell_mut(Position::new(0, col)).remove_candidate(1);
            grid.cell_mut(Position::new(0, col)).remove_candidate(2);
        }
    }
    let fab = CandidateFabric::from_grid(&grid);
    let finding = aic_engine::find_aic(&fab, &aic_engine::build_link_graph(&fab)).unwrap();
    assert!(
        matches!(finding.inference, InferenceResult::Elimination { cell: 0, ref values } if values == &[3])
    );
    let Some(ProofCertificate::Aic { chain, link_types }) = finding.proof else {
        panic!("missing chain")
    };
    assert_eq!(chain.first().unwrap().0, chain.last().unwrap().0);
    assert_ne!(chain.first().unwrap().1, chain.last().unwrap().1);
    assert_eq!(
        link_types,
        vec![LinkType::Strong, LinkType::WeakInference, LinkType::Strong]
    );
    assert_eq!(chain.first().unwrap().2, Polarity::Off);
    assert_eq!(chain.last().unwrap().2, Polarity::On);
    // Both digits 1 and 2 have only r1c1/r1c4 available in row 1.
    // Exhaustively assigning distinct digits to those cells leaves no 3.
    let assignments: Vec<_> = [1, 2, 3]
        .into_iter()
        .flat_map(|a| [1, 2].into_iter().map(move |b| (a, b)))
        .filter(|&(a, b)| a != b && [a, b].contains(&1) && [a, b].contains(&2))
        .collect();
    assert_eq!(assignments, vec![(1, 2), (2, 1)]);
}

fn set_candidates(grid: &mut Grid, pos: Position, values: &[u8]) {
    for digit in 1..=9 {
        if !values.contains(&digit) {
            grid.cell_mut(pos).remove_candidate(digit);
        }
    }
}

fn forcing_inputs() -> Grid {
    let mut grid = Grid::new_classic();
    for col in 0..9 {
        if col < 2 {
            set_candidates(&mut grid, Position::new(0, col), &[1, 2]);
        } else {
            grid.cell_mut(Position::new(0, col)).remove_candidate(1);
            grid.cell_mut(Position::new(0, col)).remove_candidate(2);
        }
    }
    set_candidates(&mut grid, Position::new(8, 8), &[3, 4, 5, 6]);
    grid
}

type Propagation<'a> = dyn Fn(&Grid, Position, u8) -> (Grid, bool) + 'a;
type ForcingSearch = for<'a> fn(&Grid, &'a Propagation<'a>) -> Option<Finding>;

#[test]
fn forcing_families_require_all_branches_to_agree_on_a_placement() {
    let grid = forcing_inputs();
    let target = Position::new(8, 8);
    let propagate = move |grid: &Grid, pos: Position, value: u8| {
        let mut branch = grid.deep_clone();
        branch.set_cell_unchecked(pos, Some(value));
        branch.set_cell_unchecked(target, Some(3));
        (branch, false)
    };
    let searches: [(ForcingSearch, Technique); 3] = [
        (aic_engine::find_cell_fc, Technique::CellForcingChain),
        (aic_engine::find_region_fc, Technique::RegionForcingChain),
        (aic_engine::find_dynamic_fc, Technique::DynamicForcingChain),
    ];
    for (search, technique) in searches {
        let finding = search(&grid, &propagate).unwrap();
        assert_eq!(finding.technique, technique);
        assert!(matches!(
            finding.inference,
            InferenceResult::Placement { cell: 80, value: 3 }
        ));
        let Some(ProofCertificate::Forcing { source, branches }) = finding.proof else {
            panic!("missing forcing proof")
        };
        assert_eq!(branches, 2);
        if technique == Technique::RegionForcingChain {
            assert!(matches!(
                source,
                ForcingSource::Region {
                    sector: 0,
                    digit: 1
                }
            ));
        } else {
            assert!(matches!(source, ForcingSource::Cell(0)));
        }
    }
}

#[test]
fn forcing_families_combine_placed_and_candidate_only_branches_for_eliminations() {
    let grid = forcing_inputs();
    let target = Position::new(8, 8);
    let propagate = move |grid: &Grid, pos: Position, value: u8| {
        let mut branch = grid.deep_clone();
        branch.set_cell_unchecked(pos, Some(value));
        if pos.col == 0 && value == 1 {
            branch.set_cell_unchecked(target, Some(3));
        } else {
            set_candidates(&mut branch, target, &[4, 5]);
        }
        (branch, false)
    };
    let searches: [ForcingSearch; 3] = [
        aic_engine::find_cell_fc,
        aic_engine::find_region_fc,
        aic_engine::find_dynamic_fc,
    ];
    for search in searches {
        let finding = search(&grid, &propagate).unwrap();
        assert!(
            matches!(finding.inference, InferenceResult::Elimination { cell: 80, ref values } if values == &[6])
        );
        assert_eq!(finding.involved_cells, vec![0, 80]);
        assert!(finding
            .to_hint()
            .explanation
            .contains("all candidates of (1, 1)"));
    }
}

#[test]
fn forcing_searches_do_not_claim_a_conclusion_from_unchanged_branches() {
    let grid = forcing_inputs();
    let unchanged = |grid: &Grid, _: Position, _: u8| (grid.deep_clone(), false);
    for search in [
        aic_engine::find_cell_fc as ForcingSearch,
        aic_engine::find_region_fc,
        aic_engine::find_dynamic_fc,
        aic_engine::find_nishio_fc,
    ] {
        assert!(search(&grid, &unchanged).is_none());
    }
    let contradictory = |grid: &Grid, _: Position, _: u8| (grid.deep_clone(), true);
    // Cell/region agreement rules must not infer a common value from an empty
    // set of surviving branches; separate Nishio rules handle contradictions.
    assert!(aic_engine::find_cell_fc(&grid, &contradictory).is_none());
    assert!(aic_engine::find_region_fc(&grid, &contradictory).is_none());
}

#[test]
fn nishio_and_dynamic_forcing_reject_only_the_contradictory_assumption() {
    let grid = forcing_inputs();
    let propagate = |grid: &Grid, pos: Position, value: u8| {
        (grid.deep_clone(), pos == Position::new(0, 0) && value == 2)
    };
    for search in [
        aic_engine::find_nishio_fc as ForcingSearch,
        aic_engine::find_dynamic_fc,
    ] {
        let finding = search(&grid, &propagate).unwrap();
        assert!(
            matches!(finding.inference, InferenceResult::Elimination { cell: 0, ref values } if values == &[2])
        );
        assert!(matches!(
            finding.proof,
            Some(ProofCertificate::Forcing {
                source: ForcingSource::Nishio { cell: 0, digit: 2 },
                branches: 1
            })
        ));
    }
}

// Independent exact-cover search for a single digit: one candidate per row,
// column and box. It checks deductions without using any solving technique.
fn digit_placement_exists(fab: &CandidateFabric, digit: u8, required: Option<usize>) -> bool {
    fn visit(
        fab: &CandidateFabric,
        digit: u8,
        required: Option<usize>,
        row: usize,
        columns: u16,
        boxes: u16,
    ) -> bool {
        if row == 9 {
            return true;
        }
        for col in 0..9 {
            let cell = row * 9 + col;
            if required.is_some_and(|fixed| fixed / 9 == row && fixed != cell) {
                continue;
            }
            let box_idx = row / 3 * 3 + col / 3;
            if columns & (1 << col) == 0
                && boxes & (1 << box_idx) == 0
                && fab.cell_cands[cell].contains(digit)
                && visit(
                    fab,
                    digit,
                    required,
                    row + 1,
                    columns | (1 << col),
                    boxes | (1 << box_idx),
                )
            {
                return true;
            }
        }
        false
    }
    visit(fab, digit, required, 0, 0, 0)
}

#[test]
fn empty_rectangle_handles_grouped_arms_in_both_orientations() {
    for transpose in [false, true] {
        let convert = |row, col| {
            if transpose {
                Position::new(col, row)
            } else {
                Position::new(row, col)
            }
        };
        let mut grid = Grid::new_classic();
        for row in 0..9 {
            for col in 0..9 {
                if (row < 3 && col < 3 && ![(0, 1), (1, 0), (2, 0)].contains(&(row, col)))
                    || (col == 4 && row != 0 && row != 4)
                {
                    grid.cell_mut(convert(row, col)).remove_candidate(1);
                }
            }
        }
        let fab = CandidateFabric::from_grid(&grid);
        let finding = aic_engine::find_empty_rectangle(&fab)
            .expect("box arm and external conjugate pair form an ER");
        let target = convert(4, 0);
        assert_eq!(finding.technique, Technique::EmptyRectangle);
        assert!(
            matches!(finding.inference, InferenceResult::Elimination { cell, ref values } if cell == target.row * 9 + target.col && values == &[1])
        );
        assert!(digit_placement_exists(&fab, 1, None));
        assert!(
            !digit_placement_exists(&fab, 1, Some(target.row * 9 + target.col)),
            "ER removes a feasible placement"
        );
        let Some(ProofCertificate::Fish {
            digit,
            base_sectors,
            cover_sectors,
            fins,
        }) = finding.proof
        else {
            panic!("grouped ER evidence must include every candidate in the arm")
        };
        assert_eq!(digit, 1);
        assert_eq!(base_sectors.len(), 2);
        assert_eq!(cover_sectors.len(), 2);
        assert_eq!(fins.len(), 2);
        assert!(fins
            .iter()
            .all(|&fin| fab.sees(fin, target.row * 9 + target.col)));
        // Reintroducing an off-arm candidate invalidates the grouped inference.
        grid.cell_mut(convert(1, 1)).add_candidate(1);
        let broken = CandidateFabric::from_grid(&grid);
        assert!(digit_placement_exists(
            &broken,
            1,
            Some(target.row * 9 + target.col)
        ));
        if let Some(other) = aic_engine::find_empty_rectangle(&broken) {
            let InferenceResult::Elimination { cell, values } = other.inference else {
                panic!("ER should eliminate")
            };
            assert!(!digit_placement_exists(&broken, values[0], Some(cell)));
        }
    }
}

#[test]
fn kraken_fish_requires_each_fin_to_rule_out_the_target_digit() {
    let mut grid = Grid::new_classic();
    for row in [0, 3] {
        for col in 0..9 {
            if col != 0 && col != 3 && !(row == 0 && col == 6) {
                grid.cell_mut(Position::new(row, col)).remove_candidate(1);
            }
        }
    }
    let target = Position::new(1, 0);
    // Each branch shape independently proves the same elimination: the fin is
    // impossible, it forces a different digit, or it explicitly removes 1.
    for mode in 0..3 {
        let propagate = |grid: &Grid, pos: Position, digit: u8| {
            assert_eq!(pos, Position::new(0, 6));
            assert_eq!(digit, 1);
            let mut branch = grid.deep_clone();
            match mode {
                0 => {}
                1 => branch.set_cell_unchecked(target, Some(2)),
                _ => branch.cell_mut(target).remove_candidate(1),
            }
            (branch, mode == 0)
        };
        let finding = aic_engine::find_kraken_fish(&grid, &propagate).unwrap();
        assert_eq!(finding.technique, Technique::KrakenFish);
        assert!(
            matches!(finding.inference, InferenceResult::Elimination { cell: 9, ref values } if values == &[1])
        );
        assert!(
            finding.involved_cells.contains(&6),
            "fin must appear in the explanation"
        );
        assert!(matches!(
            finding.proof,
            Some(ProofCertificate::Forcing {
                source: ForcingSource::Region {
                    sector: 0,
                    digit: 1
                },
                branches: 1
            })
        ));
    }
    let unchanged = |grid: &Grid, _: Position, _: u8| (grid.deep_clone(), false);
    assert!(aic_engine::find_kraken_fish(&grid, &unchanged).is_none());
    let keeps_target = |grid: &Grid, _: Position, _: u8| {
        let mut branch = grid.deep_clone();
        branch.set_cell_unchecked(target, Some(1));
        (branch, false)
    };
    assert!(aic_engine::find_kraken_fish(&grid, &keeps_target).is_none());
}

#[test]
#[allow(deprecated)]
fn medusa_coloring_eliminates_a_candidate_seeing_both_colors() {
    let mut grid = Grid::new_classic();
    for row in 0..9 {
        for col in 0..9 {
            if ([0, 3].contains(&row) && ![0, 3].contains(&col))
                || (col == 3 && ![0, 3].contains(&row))
            {
                grid.cell_mut(Position::new(row, col)).remove_candidate(1);
            }
        }
    }
    let fab = CandidateFabric::from_grid(&grid);
    let finding = aic_engine::find_medusa(&fab, &aic_engine::build_link_graph(&fab)).unwrap();
    assert_eq!(finding.technique, Technique::ThreeDMedusa);
    let InferenceResult::Elimination { cell, values } = finding.inference else {
        panic!("Medusa should eliminate")
    };
    assert_eq!(values, vec![1]);
    assert!(digit_placement_exists(&fab, 1, None));
    assert!(!digit_placement_exists(&fab, 1, Some(cell)));
    assert!(matches!(finding.proof, Some(ProofCertificate::Aic { .. })));
}

#[test]
#[allow(deprecated)]
fn medusa_rejects_a_color_that_places_the_same_digit_twice_in_a_box() {
    let mut grid = Grid::new_classic();
    for row in 0..9 {
        for col in 0..9 {
            if (row == 0 && ![0, 3].contains(&col))
                || (row == 3 && ![2, 3].contains(&col))
                || (col == 3 && ![0, 3].contains(&row))
                || (col == 2 && ![1, 3].contains(&row))
            {
                grid.cell_mut(Position::new(row, col)).remove_candidate(1);
            }
        }
    }
    let fab = CandidateFabric::from_grid(&grid);
    let finding = aic_engine::find_medusa(&fab, &aic_engine::build_link_graph(&fab)).unwrap();
    assert_eq!(finding.technique, Technique::ThreeDMedusa);
    let InferenceResult::Elimination { cell, values } = finding.inference else {
        panic!("Medusa should eliminate")
    };
    assert_eq!(values, vec![1]);
    assert!(digit_placement_exists(&fab, 1, None));
    assert!(!digit_placement_exists(&fab, 1, Some(cell)));
}

#[test]
fn backtracking_explanation_identifies_the_first_empty_cell_and_solution_value() {
    let grid = Grid::from_string(EASY).unwrap();
    let hint = backtrack::find_backtracking_hint(&grid).unwrap().to_hint();
    assert_eq!(hint.technique, Technique::Backtracking);
    assert!(matches!(
        hint.hint_type,
        HintType::SetValue {
            pos: Position { row: 0, col: 2 },
            value: 4
        }
    ));
    assert_eq!(hint.involved_cells, vec![Position::new(0, 2)]);
    assert_eq!(hint.explanation, "The cell at (1, 3) must be 4.");
    assert!(matches!(hint.proof, Some(ProofCertificate::Backtracking)));
}

fn explanation_hint(explanation: ExplanationData) -> Hint {
    Finding {
        technique: Technique::AlsChain,
        inference: InferenceResult::Elimination {
            cell: 80,
            values: vec![2, 7],
        },
        involved_cells: vec![0, 10, 80],
        explanation,
        proof: Some(ProofCertificate::Als {
            als_chain: vec![AlsProofDescriptor {
                cells: vec![0, 10],
                candidates: vec![2, 5, 7],
                sector: 18,
            }],
            rcc_values: vec![5],
            z_value: Some(7),
        }),
    }
    .to_hint()
}

#[test]
fn arithmetic_hint_conversion_preserves_a_verifiable_proof_and_raw_explanation() {
    let mut grid = Grid::new_classic();
    for digit in 1..=9 {
        if digit != 3 {
            grid.cell_mut(Position::new(0, 0)).remove_candidate(digit);
        }
    }
    let proof = ArithmeticProof::from_terms(
        &grid,
        vec![ArithmeticTerm {
            requirement: ArithmeticRequirement::Cell { cell: 0 },
            weight: 1,
        }],
        0,
        3,
        true,
    )
    .expect("the sole candidate follows from its cell requirement");
    let expected_proof = serde_json::to_value(&proof).unwrap();
    let explanation = "Arithmetic Counting: the cell requirement forces 3.";
    let hint = Finding {
        technique: Technique::ArithmeticCounting,
        inference: InferenceResult::Placement { cell: 0, value: 3 },
        involved_cells: vec![0],
        explanation: ExplanationData::Raw(explanation.into()),
        proof: Some(ProofCertificate::Arithmetic(proof)),
    }
    .to_hint();

    assert_eq!(hint.technique, Technique::ArithmeticCounting);
    assert!(matches!(
        hint.hint_type,
        HintType::SetValue {
            pos: Position { row: 0, col: 0 },
            value: 3
        }
    ));
    assert_eq!(hint.involved_cells, vec![Position::new(0, 0)]);
    assert_eq!(hint.explanation, explanation);
    let Some(ProofCertificate::Arithmetic(carried)) = hint.proof else {
        panic!("arithmetic proof was lost during Finding-to-Hint conversion")
    };
    assert!(carried.verify(&grid));
    assert_eq!(serde_json::to_value(carried).unwrap(), expected_proof);
}

#[test]
fn elimination_hint_conversion_preserves_candidates_positions_and_als_evidence() {
    let hint = explanation_hint(ExplanationData::Als {
        variant: "ALS Chain".into(),
        chain_length: 3,
        shared_value: Some(7),
    });
    assert!(
        matches!(hint.hint_type, HintType::EliminateCandidates { pos: Position { row: 8, col: 8 }, ref values } if values == &[2, 7])
    );
    assert_eq!(
        hint.involved_cells,
        vec![
            Position::new(0, 0),
            Position::new(1, 1),
            Position::new(8, 8)
        ]
    );
    assert_eq!(
        hint.explanation,
        "ALS Chain: chain of 3 ALS linked by shared value 7."
    );
    let Some(ProofCertificate::Als {
        als_chain,
        rcc_values,
        z_value,
    }) = hint.proof
    else {
        panic!("missing ALS proof")
    };
    assert_eq!(als_chain[0].cells, vec![0, 10]);
    assert_eq!(als_chain[0].candidates, vec![2, 5, 7]);
    assert_eq!(als_chain[0].sector, 18);
    assert_eq!(rcc_values, vec![5]);
    assert_eq!(z_value, Some(7));
}

#[test]
fn explanations_name_locked_sets_and_technique_specific_details() {
    for (size, name) in [(2, "Pair"), (3, "Triple"), (4, "Quad"), (5, "Set")] {
        let hint = explanation_hint(ExplanationData::LockedSet {
            sector: 0,
            kind: "Hidden",
            size,
            cells: vec![0, 1],
            values: vec![1, 2],
            sector_name: "row 1".into(),
        });
        assert_eq!(
            hint.explanation,
            format!("Hidden {name} on [1, 2] in row 1.")
        );
    }
    let cases = [
        (
            ExplanationData::HiddenSingle {
                sector: 0,
                cell: 80,
                value: 3,
                sector_name: "column 9".into(),
            },
            "3 can only go in cell (9, 9) in column 9.",
        ),
        (
            ExplanationData::Intersection {
                kind: "Pointing Pair",
                digit: 4,
                from_sector: "box 1".into(),
                to_sector: "row 1".into(),
            },
            "Pointing Pair: in box 1, 4 is confined to row 1, eliminating from rest of row 1.",
        ),
        (
            ExplanationData::Als {
                variant: "ALS Chain".into(),
                chain_length: 4,
                shared_value: None,
            },
            "ALS Chain: chain of 4 ALS.",
        ),
        (
            ExplanationData::Chain {
                variant: "AIC".into(),
                chain_length: 6,
                values: vec![3, 7],
            },
            "AIC: chain of length 6.",
        ),
        (
            ExplanationData::Uniqueness {
                variant: "Unique Rectangle".into(),
            },
            "Unique Rectangle found.",
        ),
        (
            ExplanationData::ForcingChain {
                variant: "Cell Forcing Chain".into(),
                source_cell: 72,
            },
            "Cell Forcing Chain: all candidates of (9, 1) lead to same conclusion.",
        ),
        (
            ExplanationData::Raw("Existing explanation kept intact.".into()),
            "Existing explanation kept intact.",
        ),
    ];
    for (data, expected) in cases {
        assert_eq!(explanation_hint(data).explanation, expected);
    }
}

#[test]
fn fish_explanations_distinguish_sizes_variants_and_fins() {
    let cases = [
        (2, "Basic", "X-Wing"),
        (3, "Basic", "Swordfish"),
        (4, "Basic", "Jellyfish"),
        (2, "Finned", "Finned X-Wing"),
        (3, "Finned", "Finned Swordfish"),
        (4, "Finned", "Finned Jellyfish"),
        (3, "Franken", "Franken Fish"),
        (3, "Siamese", "Siamese Fish"),
        (3, "Mutant", "Mutant Fish"),
        (5, "Other", "Fish"),
    ];
    for (size, variant, name) in cases {
        for fins in [vec![], vec![8]] {
            let hint = explanation_hint(ExplanationData::Fish {
                size,
                digit: 7,
                base_sectors: vec!["row 1".into(), "row 4".into()],
                cover_sectors: vec!["column 1".into(), "column 9".into()],
                fins: fins.clone(),
                variant: variant.into(),
            });
            let suffix = if fins.is_empty() {
                "."
            } else {
                " (fins present)."
            };
            assert_eq!(hint.explanation, format!("{name} on 7 in bases [\"row 1\", \"row 4\"], covers [\"column 1\", \"column 9\"]{suffix}"));
        }
    }
}
