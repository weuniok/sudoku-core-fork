//! Teaching adapter for the existing forcing finders. Only explicitly allowed
//! named techniques propagate; structural candidate constraints always apply.
use super::teaching;
use super::teaching_contract::{
    TeachingBranch, TeachingCandidate, TeachingContradiction, TeachingDeduction,
};
use super::teaching_proof::adapt;
use super::{
    aic_engine,
    explain::{Finding, InferenceResult, ProofCertificate},
    fabric::{idx_to_pos, sector_cells, CandidateFabric},
    search_budget::{exhaust, tick},
};
use crate::{Grid, Position};
use std::{cell::RefCell, collections::VecDeque};

fn contradiction(grid: &Grid) -> Option<TeachingContradiction> {
    let fab = CandidateFabric::from_grid(grid);
    for cell in 0..81 {
        if fab.values[cell].is_none() && fab.cell_cands[cell].is_empty() {
            return Some(TeachingContradiction {
                kind: "empty_cell".into(),
                cells: vec![cell as u8],
                house: None,
                digit: None,
            });
        }
    }
    for house in 0..27 {
        for digit in 1..=9 {
            let placed: Vec<u8> = sector_cells(house)
                .into_iter()
                .filter(|&c| fab.values[c] == Some(digit))
                .map(|c| c as u8)
                .collect();
            if placed.len() > 1 {
                return Some(TeachingContradiction {
                    kind: "duplicate".into(),
                    cells: placed,
                    house: Some(house as u8),
                    digit: Some(digit),
                });
            }
            if placed.is_empty() && fab.sector_cand_count(house, digit) == 0 {
                return Some(TeachingContradiction {
                    kind: "missing_digit".into(),
                    cells: sector_cells(house).map(|c| c as u8).to_vec(),
                    house: Some(house as u8),
                    digit: Some(digit),
                });
            }
        }
    }
    None
}

fn place(grid: &mut Grid, pos: Position, digit: u8) {
    let prior: Vec<_> = (0..81)
        .map(|c| grid.get_candidates(idx_to_pos(c)))
        .collect();
    grid.set_cell_unchecked(pos, Some(digit));
    grid.recalculate_candidates();
    // Placing cannot resurrect accepted eliminations or earlier branch deductions.
    for (cell, previous) in prior.iter().enumerate() {
        let p = idx_to_pos(cell);
        if grid.get(p).is_none() {
            let candidates = grid.get_candidates(p).intersection(previous);
            grid.cell_mut(p).set_candidates(candidates);
        }
    }
}

fn propagate(
    grid: &Grid,
    pos: Position,
    digit: u8,
    dynamic: bool,
    allowed: &[&str],
    unique: bool,
) -> (Grid, TeachingBranch) {
    let mut result = grid.deep_clone();
    let mut branch = TeachingBranch {
        assumption: TeachingCandidate {
            cell: (pos.row * 9 + pos.col) as u8,
            digit,
        },
        steps: vec![],
        contradiction: None,
    };
    place(&mut result, pos, digit);
    for _ in 0..=256 {
        if !tick() {
            return (result, branch);
        }
        branch.contradiction = contradiction(&result);
        if branch.contradiction.is_some() || result.is_complete() {
            return (result, branch);
        }
        let fab = CandidateFabric::from_grid(&result);
        let mut selected = None;
        for &id in allowed {
            if teaching::is_forcing(id)
                || (!unique && teaching::requires_uniqueness(id))
                || (!dynamic && id != "naked_single" && id != "hidden_single")
            {
                continue;
            }
            if let Some(finding) = teaching::find_direct(&fab, id) {
                selected = Some((id, finding));
                break;
            }
        }
        let Some((id, finding)) = selected else {
            return (result, branch);
        };
        if branch.steps.len() >= 256 {
            exhaust();
            return (result, branch);
        }
        let hint = match adapt(&finding, &fab, id, String::new()) {
            Ok(h) => h,
            Err(_) => {
                exhaust();
                return (result, branch);
            }
        };
        branch.steps.push(TeachingDeduction {
            technique_id: id.into(),
            source_cells: hint.source_cells,
            candidates: hint.candidates,
            placements: hint.placements,
            eliminations: hint.eliminations,
            houses: hint.houses,
            proof: hint.proof.evidence,
        });
        match finding.inference {
            InferenceResult::Placement { cell, value } => {
                place(&mut result, idx_to_pos(cell), value)
            }
            InferenceResult::Elimination { cell, values } => {
                for value in values {
                    result.cell_mut(idx_to_pos(cell)).remove_candidate(value);
                }
            }
        }
    }
    exhaust();
    (result, branch)
}

pub(super) fn find(
    grid: &Grid,
    technique: &str,
    allowed: &[&str],
    unique: bool,
) -> Result<(Option<Finding>, Vec<TeachingBranch>), String> {
    // Existing finders use at most nine assumptions for a result. Keep only the
    // last nine complete traces, not every unsuccessful pattern searched.
    let traces = RefCell::new(VecDeque::new());
    let propagate = |input: &Grid, pos: Position, digit: u8| {
        let (result, branch) = propagate(
            input,
            pos,
            digit,
            technique == "dynamic_forcing_chain",
            allowed,
            unique,
        );
        let contradiction = branch.contradiction.is_some();
        let mut traces = traces.borrow_mut();
        if traces.len() == 9 {
            traces.pop_front();
        }
        traces.push_back(branch);
        (result, contradiction)
    };
    let finding = match technique {
        "nishio_forcing_chain" => aic_engine::find_nishio_fc(grid, &propagate),
        "kraken_fish" => aic_engine::find_kraken_fish(grid, &propagate),
        "cell_forcing_chain" => aic_engine::find_cell_fc(grid, &propagate),
        "region_forcing_chain" => aic_engine::find_region_fc(grid, &propagate),
        "dynamic_forcing_chain" => aic_engine::find_dynamic_fc(grid, &propagate),
        _ => return Err("Unsupported forcing technique".into()),
    };
    let Some(ProofCertificate::Forcing { branches, .. }) =
        finding.as_ref().and_then(|f| f.proof.as_ref())
    else {
        return Ok((finding, vec![]));
    };
    let mut traces = traces.into_inner();
    if *branches == 0 || *branches > 9 || *branches > traces.len() {
        return Err("Incomplete forcing proof".into());
    }
    let retained = traces
        .split_off(traces.len() - branches)
        .into_iter()
        .collect();
    Ok((finding, retained))
}
