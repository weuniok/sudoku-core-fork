use std::process::Command;
use sudoku_core::search_teaching_hint;

#[test]
fn emit_teaching_hint() {
    if std::env::var("SUDOKU_TEACHING_CHILD").as_deref() != Ok("1") {
        return;
    }
    let mut exclusions = vec![0u16; 81];
    for (cell, mask) in [(0, 3), (1, 5), (2, 12), (3, 10)] {
        exclusions[cell] = 511 ^ mask;
    }
    let blank = "0".repeat(81);
    let result = search_teaching_hint(&blank, &blank, &exclusions, "aic", 200_000).unwrap();
    assert_eq!(result.status, "found");
    let fixtures: serde_json::Value =
        serde_json::from_str(include_str!("fixtures/expanded-teaching-hints.json")).unwrap();
    let expanded: Vec<_> = fixtures["cases"]
        .as_array()
        .unwrap()
        .iter()
        .map(|c| {
            search_teaching_hint(
                c["position"]["puzzle"].as_str().unwrap(),
                c["position"]["values"].as_str().unwrap(),
                &serde_json::from_value::<Vec<u16>>(c["position"]["exclusions"].clone()).unwrap(),
                c["techniqueId"].as_str().unwrap(),
                2_000_000,
            )
            .unwrap()
        })
        .collect();
    println!(
        "HINT_FIXTURE:{}",
        serde_json::to_string(&serde_json::json!({"aic":result,"expanded":expanded})).unwrap()
    );
}

#[test]
fn independent_hash_seeds_choose_the_same_aic_and_proof() {
    let executable = std::env::current_exe().unwrap();
    let mut expected = None;
    for _ in 0..4 {
        let output = Command::new(&executable)
            .args(["--exact", "emit_teaching_hint", "--nocapture"])
            .env("SUDOKU_TEACHING_CHILD", "1")
            .output()
            .unwrap();
        assert!(output.status.success());
        let stdout = String::from_utf8(output.stdout).unwrap();
        let hint = stdout
            .lines()
            .find(|line| line.starts_with("HINT_FIXTURE:"))
            .unwrap()
            .to_owned();
        if let Some(previous) = &expected {
            assert_eq!(&hint, previous);
        }
        expected = Some(hint);
    }
}
