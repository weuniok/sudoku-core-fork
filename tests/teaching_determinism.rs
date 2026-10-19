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
    println!("HINT_FIXTURE:{}", serde_json::to_string(&result).unwrap());
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
