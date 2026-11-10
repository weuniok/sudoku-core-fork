//! Reproducible positive teaching fixtures. Diagrams preserve actual candidate
//! state; uniqueness examples start from independently checked unique puzzles.
use serde_json::{json, Value};
use sudoku_core::{search_teaching_hint, Grid, Position, Solver, TEACHING_TECHNIQUES};

const SOLUTION: &str =
    "534678912672195348198342567859761423426853791713924856961537284287419635345286179";
const TYPE1: &str =
    "034078912072090348198342567859761423426853791713924856961037284287419635345286179";
const TYPE4: &str =
    "034008902072095348098342567859061423426853791713924856961537284287419635345286179";
const EXTENDED: &str =
    "034070912072090348198342567859761423426853791713924856961037284287419635345280179";

fn masks(pattern: &[(usize, &[u8])]) -> Vec<u16> {
    let mut result = vec![0; 81];
    for &(cell, digits) in pattern {
        result[cell] = 511 ^ digits.iter().fold(0, |m, d| m | (1 << (d - 1)));
    }
    result
}
fn witness(values: &str, exclusions: &[u16]) -> String {
    fn solve(values: &mut [u8; 81], allowed: &[u16], work: &mut usize) -> bool {
        *work += 1;
        assert!(*work < 3_000_000, "Example witness search limit");
        let mut candidates = [0u16; 81];
        let mut options = Vec::new();
        for cell in 0..81 {
            if values[cell] != 0 {
                continue;
            }
            let mut mask = allowed[cell];
            for peer in 0..81 {
                if values[peer] != 0
                    && (cell / 9 == peer / 9
                        || cell % 9 == peer % 9
                        || (cell / 27 == peer / 27 && cell % 9 / 3 == peer % 9 / 3))
                {
                    mask &= !(1 << (values[peer] - 1));
                }
            }
            if mask == 0 {
                return false;
            }
            candidates[cell] = mask;
            if options.is_empty() || (mask.count_ones() as usize) < options.len() {
                options = (1..=9)
                    .filter(|d| mask & (1 << (d - 1)) != 0)
                    .map(|d| (cell, d))
                    .collect();
            }
        }
        if options.is_empty() {
            return true;
        }
        for house in 0..27 {
            let cells: Vec<usize> = (0..9)
                .map(|i| {
                    if house < 9 {
                        house * 9 + i
                    } else if house < 18 {
                        i * 9 + house - 9
                    } else {
                        (house - 18) / 3 * 27 + (house - 18) % 3 * 3 + i / 3 * 9 + i % 3
                    }
                })
                .collect();
            for digit in 1..=9 {
                if cells.iter().any(|&c| values[c] == digit) {
                    continue;
                }
                let positions: Vec<_> = cells
                    .iter()
                    .copied()
                    .filter(|&c| candidates[c] & (1 << (digit - 1)) != 0)
                    .map(|c| (c, digit))
                    .collect();
                if positions.is_empty() {
                    return false;
                }
                if positions.len() < options.len() {
                    options = positions;
                }
            }
        }
        for (cell, digit) in options {
            values[cell] = digit;
            if solve(values, allowed, work) {
                return true;
            }
            values[cell] = 0;
        }
        false
    }
    let mut digits = [0; 81];
    for (cell, b) in values.bytes().enumerate() {
        digits[cell] = b - b'0';
    }
    assert!(solve(
        &mut digits,
        &exclusions.iter().map(|m| 511 ^ m).collect::<Vec<_>>(),
        &mut 0
    ));
    digits.iter().map(|d| char::from(b'0' + d)).collect()
}
fn record(id: &str, puzzle: &str, values: &str, exclusions: Vec<u16>) -> Value {
    let result = search_teaching_hint(puzzle, values, &exclusions, id, 2_000_000).unwrap();
    eprintln!("{id}: {} ({} work)", result.status, result.work_used);
    let solution = witness(values, &exclusions);
    for hint in &result.hints {
        for p in &hint.placements {
            assert_eq!(
                solution.as_bytes()[p.cell as usize] - b'0',
                p.digit,
                "Unsound {id} placement"
            );
        }
        for e in &hint.eliminations {
            assert_ne!(
                solution.as_bytes()[e.cell as usize] - b'0',
                e.digit,
                "Unsound {id} elimination"
            );
        }
    }
    json!({"techniqueId":id,"position":{"puzzle":puzzle,"values":values,"exclusions":exclusions},"solution":solution,"result":result})
}
fn diagram(id: &str, exclusions: Vec<u16>) -> Value {
    record(id, &"0".repeat(81), &"0".repeat(81), exclusions)
}
fn unique(id: &str, puzzle: &str, pattern: &[(usize, &[u8])], placed: &[usize]) -> Value {
    let grid = Grid::from_string(puzzle).unwrap();
    assert!(Solver::new().has_unique_solution(&grid));
    let mut values = puzzle.as_bytes().to_vec();
    for &cell in placed {
        values[cell] = SOLUTION.as_bytes()[cell];
    }
    let mut exclusions = vec![0; 81];
    for (cell, value) in values.iter().enumerate() {
        if *value == b'0' {
            exclusions[cell] = 511 ^ (1 << (SOLUTION.as_bytes()[cell] - b'1'));
        }
    }
    for &(cell, digits) in pattern {
        if values[cell] != b'0' {
            continue;
        }
        let legal = grid.get_candidates(Position::new(cell / 9, cell % 9));
        assert!(digits.iter().all(|&d| legal.contains(d)));
        assert!(digits.contains(&(SOLUTION.as_bytes()[cell] - b'0')));
        exclusions[cell] = 511 ^ digits.iter().fold(0, |m, d| m | (1 << (d - 1)));
    }
    record(
        id,
        puzzle,
        std::str::from_utf8(&values).unwrap(),
        exclusions,
    )
}
fn main() {
    let mut cases = vec![];
    for (id, size, hidden) in [
        ("hidden_pair", 2, true),
        ("naked_triple", 3, false),
        ("hidden_triple", 3, true),
        ("naked_quad", 4, false),
        ("hidden_quad", 4, true),
    ] {
        let mut exclusions = vec![0; 81];
        if hidden {
            for mask in &mut exclusions[size..9] {
                *mask = (1 << size) - 1;
            }
        } else {
            for mask in &mut exclusions[..size] {
                *mask = 511 ^ ((1 << size) - 1);
            }
        }
        cases.push(diagram(id, exclusions));
    }
    for (id, size, fin) in [
        ("finned_x_wing", 2, true),
        ("swordfish", 3, false),
        ("finned_swordfish", 3, true),
        ("jellyfish", 4, false),
        ("finned_jellyfish", 4, true),
    ] {
        let mut exclusions = vec![0; 81];
        let bases = if fin && size > 2 {
            [0, 3, 6, 7]
        } else {
            [0, 1, 2, 3]
        };
        let covers = if fin && size > 2 {
            [0, 1, 3, 8]
        } else {
            [0, 5, 6, 1]
        };
        for &row in &bases[..size] {
            for col in 0..9 {
                let fin_col = if size > 2 { 2 } else { 3 };
                if !covers[..size].contains(&col) && !(fin && row == 0 && col == fin_col) {
                    exclusions[row * 9 + col] |= 16;
                }
            }
        }
        cases.push(diagram(id, exclusions));
    }
    let mut general = vec![0; 81];
    for row in 0..2 {
        for col in 0..9 {
            if ![0, 3, 4, 5].contains(&col) {
                general[row * 9 + col] = 16;
            }
        }
    }
    cases.push(diagram("franken_fish", general.clone()));
    cases.push(diagram("mutant_fish", general));
    let mut siamese = vec![0; 81];
    for (cell, value) in SOLUTION.bytes().enumerate() {
        siamese[cell] = 511 ^ ((1 << (value - b'1')) | 16);
    }
    for row in [0, 3, 8] {
        for col in 0..9 {
            if ![0, 1].contains(&col) && !(row == 8 && col == 2) {
                siamese[row * 9 + col] |= 16;
            }
        }
    }
    cases.push(diagram("siamese_fish", siamese));
    let mut er = vec![0; 81];
    for row in 0..9 {
        for col in 0..9 {
            if (row < 3 && col < 3 && ![(0, 1), (1, 0), (2, 0)].contains(&(row, col)))
                || (col == 4 && row != 0 && row != 4)
            {
                er[row * 9 + col] = 1;
            }
        }
    }
    cases.push(diagram("empty_rectangle", er));
    cases.push(unique(
        "unique_rectangle",
        TYPE1,
        &[(0, &[5, 6]), (3, &[5, 6]), (9, &[5, 6]), (12, &[1, 5, 6])],
        &[],
    ));
    cases.push(unique(
        "avoidable_rectangle",
        TYPE1,
        &[(0, &[5, 6]), (3, &[5, 6]), (9, &[5, 6]), (12, &[1, 5, 6])],
        &[0, 3, 9],
    ));
    cases.push(unique(
        "hidden_rectangle",
        TYPE4,
        &[
            (9, &[1, 6]),
            (12, &[1, 6]),
            (0, &[1, 5, 6]),
            (3, &[1, 6, 7]),
        ],
        &[],
    ));
    cases.push(unique(
        "extended_unique_rectangle",
        EXTENDED,
        &[
            (0, &[5, 6]),
            (3, &[5, 6]),
            (5, &[5, 6, 8]),
            (9, &[5, 6]),
            (12, &[1, 5, 6]),
            (14, &[5, 6]),
        ],
        &[],
    ));
    cases.push(diagram(
        "xyz_wing",
        masks(&[(0, &[1, 2, 3]), (1, &[1, 3]), (9, &[2, 3])]),
    ));
    let mut w = masks(&[(0, &[1, 2]), (40, &[1, 2])]);
    for col in 0..9 {
        if col != 0 && col != 4 {
            w[27 + col] |= 1;
        }
    }
    cases.push(diagram("w_wing", w));
    let mut x = vec![0; 81];
    for row in [0, 3] {
        for col in 0..9 {
            if col != 0 && col != 3 {
                x[row * 9 + col] |= 1;
            }
        }
    }
    cases.push(diagram("x_chain", x));
    let mut sdc = masks(&[
        (0, &[1, 2, 3, 4]),
        (1, &[1, 2, 3, 4]),
        (9, &[1, 2]),
        (3, &[3, 4]),
        (10, &[1, 2, 5]),
    ]);
    let mut values = b"000000000".repeat(9);
    values[2] = b'9';
    sdc[2] = 0;
    cases.push(record(
        "sue_de_coq",
        &"0".repeat(81),
        std::str::from_utf8(&values).unwrap(),
        sdc,
    ));
    cases.push(diagram(
        "als_xz",
        masks(&[
            (0, &[1, 2]),
            (1, &[1, 3]),
            (2, &[3, 4]),
            (3, &[4, 5]),
            (4, &[2, 5]),
        ]),
    ));
    cases.push(diagram(
        "als_xy_wing",
        masks(&[(0, &[1, 3]), (3, &[1, 2]), (30, &[2, 3]), (27, &[3, 4])]),
    ));
    cases.push(diagram(
        "als_chain",
        masks(&[
            (0, &[1, 4]),
            (3, &[1, 2]),
            (30, &[2, 3]),
            (27, &[3, 4]),
            (9, &[4, 5]),
        ]),
    ));
    cases.push(diagram(
        "death_blossom",
        masks(&[(0, &[1, 2]), (3, &[1, 3]), (27, &[2, 3]), (30, &[3, 4])]),
    ));
    let bug = "534678912672195348198342567859761423426853791713924856060500284280400600040286100";
    cases.push(record("bug", bug, bug, vec![0; 81]));
    cases.push(diagram(
        "nishio_forcing_chain",
        masks(&[(0, &[1, 2]), (1, &[1])]),
    ));
    cases.push(diagram(
        "dynamic_forcing_chain",
        masks(&[(0, &[1, 2]), (1, &[1])]),
    ));
    cases.push(diagram(
        "cell_forcing_chain",
        masks(&[(0, &[1, 2]), (80, &[9])]),
    ));
    let mut region = masks(&[(0, &[1, 2]), (1, &[1, 3]), (80, &[9])]);
    for mask in &mut region[2..9] {
        *mask |= 1;
    }
    cases.push(diagram("region_forcing_chain", region));
    let mut kraken = masks(&[(15, &[1])]);
    for row in [0, 3] {
        for col in 0..9 {
            if col != 0 && col != 3 && !(row == 0 && col == 6) {
                kraken[row * 9 + col] |= 1;
            }
        }
    }
    cases.push(diagram("kraken_fish", kraken));
    assert!(
        cases.iter().all(|c| c["result"]["status"] == "found"),
        "Every exposed technique requires a positive fixture"
    );
    println!(
        "{}",
        serde_json::to_string_pretty(
            &json!({"cases":cases,"workLimit":2_000_000,"allowedTechniqueIds":TEACHING_TECHNIQUES})
        )
        .unwrap()
    );
}
