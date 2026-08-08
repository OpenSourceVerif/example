use std::{path::PathBuf, process::Command};

#[test]
fn emits_contract_and_loop_invariant_obligations() {
    let fixture =
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/valid_contracts.rs");
    let output = Command::new(env!("CARGO_BIN_EXE_verifier"))
        .arg(fixture)
        .output()
        .expect("run verifier driver");

    assert!(output.status.success(), "driver failed: {}", String::from_utf8_lossy(&output.stderr));
    let stdout = String::from_utf8(output.stdout).expect("driver output is UTF-8");
    assert!(stdout.contains("Postcondition"), "missing postcondition VC:\n{stdout}");
    assert!(
        stdout.contains("LoopInvariantInitialization"),
        "missing invariant initialization VC:\n{stdout}"
    );
    assert!(
        stdout.contains("LoopInvariantPreservation"),
        "missing invariant preservation VC:\n{stdout}"
    );
    assert!(
        !stdout.contains("step exploration limit"),
        "loop was unfolded instead of cut:\n{stdout}"
    );
}

#[test]
fn rejects_a_loop_without_an_invariant() {
    let fixture = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/no_invariant.rs");
    let output = Command::new(env!("CARGO_BIN_EXE_verifier"))
        .arg(fixture)
        .output()
        .expect("run verifier driver");

    assert!(output.status.success(), "driver failed: {}", String::from_utf8_lossy(&output.stderr));
    let stderr = String::from_utf8(output.stderr).expect("driver error output is UTF-8");
    assert!(
        stderr.contains("requires at least one `#[verifier::invariant(...)]`"),
        "missing invariant diagnostic:\n{stderr}"
    );
    assert!(!stderr.contains("step exploration limit"), "loop was unfolded:\n{stderr}");
}
