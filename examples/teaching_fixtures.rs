//! Emit deterministic cross-language fixtures for the Expo bridge contract.
use sudoku_core::search_teaching_hint;

fn case(technique: &str, exclusions: Vec<u16>) -> serde_json::Value {
    let blank = "0".repeat(81);
    let result = search_teaching_hint(&blank, &blank, &exclusions, technique, 200_000).unwrap();
    assert_eq!(result.status, "found", "{technique}");
    serde_json::json!({ "techniqueId": technique,
        "position": { "puzzle": blank, "values": blank, "exclusions": exclusions }, "result": result })
}

fn main() {
    let mut exclusions = vec![0u16; 81];
    for (cell, mask) in [(0, 3), (1, 5), (2, 12), (3, 10)] {
        exclusions[cell] = 511 ^ mask;
    }
    let blank = "0".repeat(81);
    let results: serde_json::Map<String, serde_json::Value> =
        ["aic", "wxyz_wing", "xy_wing", "naked_pair"]
            .into_iter()
            .map(|technique| {
                (
                    technique.into(),
                    serde_json::to_value(
                        search_teaching_hint(&blank, &blank, &exclusions, technique, 200_000)
                            .unwrap(),
                    )
                    .unwrap(),
                )
            })
            .collect();
    let mut single = vec![0u16; 81];
    single[0] = 511 ^ 1;
    let mut hidden = vec![0u16; 81];
    hidden[1..9].fill(1);
    let mut locked = vec![0u16; 81];
    for cell in [2, 9, 10, 11, 18, 19, 20] {
        locked[cell] = 1;
    }
    let mut pair = vec![0u16; 81];
    pair[0] = 511 ^ 3;
    pair[1] = 511 ^ 3;
    let mut fish = vec![0u16; 81];
    for row in [0, 3] {
        for col in 0..9 {
            if col != 0 && col != 4 {
                fish[row * 9 + col] = 1;
            }
        }
    }
    let mut wing = vec![0u16; 81];
    for (cell, mask) in [(0, 3), (4, 5), (36, 6)] {
        wing[cell] = 511 ^ mask;
    }
    let cases = vec![
        case("naked_single", single),
        case("hidden_single", hidden),
        case("locked_candidates", locked),
        case("naked_pair", pair),
        case("x_wing", fish),
        case("xy_wing", wing),
    ];
    println!("{}", serde_json::to_string_pretty(&serde_json::json!({
        "position": { "puzzle": blank, "values": blank, "exclusions": exclusions }, "results": results, "cases": cases,
    })).unwrap());
}
