use std::{path::PathBuf, process::Command};

fn run_fixture(name: &str) -> std::process::Output {
    let fixture = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures").join(name);
    Command::new(env!("CARGO_BIN_EXE_verifier")).arg(fixture).output().expect("run verifier driver")
}

#[test]
fn verifies_tuple_and_unit_contracts() {
    let output = run_fixture("tuple_contracts.rs");

    assert!(output.status.success(), "driver failed: {}", String::from_utf8_lossy(&output.stderr));
    assert!(
        output.stderr.is_empty(),
        "unexpected diagnostics: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn does_not_translate_integer_not_as_boolean_not() {
    let output = run_fixture("unsupported_bitwise_not.rs");

    assert!(output.status.success(), "driver failed: {}", String::from_utf8_lossy(&output.stderr));
    let stderr = String::from_utf8(output.stderr).expect("driver error output is UTF-8");
    assert!(
        stderr.contains("complement: skipped")
            && stderr.contains("bitwise not on unsupported type `u8`"),
        "missing unsupported-operation diagnostic:\n{stderr}"
    );
}

#[test]
fn reports_unsupported_mir_without_panicking() {
    let output = run_fixture("unsupported_call.rs");

    assert!(output.status.success(), "driver failed: {}", String::from_utf8_lossy(&output.stderr));
    let stderr = String::from_utf8(output.stderr).expect("driver error output is UTF-8");
    assert!(stderr.contains("call: skipped"), "missing skipped-function diagnostic:\n{stderr}");
    assert!(
        stderr.contains("unsupported terminator `"),
        "missing unsupported terminator:\n{stderr}"
    );
    assert!(!stderr.contains("panicked"), "unsupported MIR panicked:\n{stderr}");
}

#[test]
fn rejects_result_as_a_parameter_name() {
    let output = run_fixture("reserved_result.rs");

    assert!(!output.status.success(), "driver accepted the reserved name");
    let stderr = String::from_utf8(output.stderr).expect("driver error output is UTF-8");
    assert!(stderr.contains("`result` is reserved in contracts"), "missing error:\n{stderr}");
}

#[test]
fn reads_structured_attribute_arguments() {
    let output = run_fixture("structured_attributes.rs");

    assert!(output.status.success(), "driver failed: {}", String::from_utf8_lossy(&output.stderr));
    assert!(
        output.stderr.is_empty(),
        "unexpected diagnostics: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn maps_nested_loops_by_source_order() {
    let output = run_fixture("nested_loops.rs");

    assert!(output.status.success(), "driver failed: {}", String::from_utf8_lossy(&output.stderr));
    assert!(
        output.stderr.is_empty(),
        "unexpected diagnostics: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn verifies_contract_and_loop_invariant_obligations() {
    let output = run_fixture("valid_contracts.rs");

    assert!(output.status.success(), "driver failed: {}", String::from_utf8_lossy(&output.stderr));
    let stdout = String::from_utf8(output.stdout).expect("driver output is UTF-8");
    assert!(stdout.is_empty(), "unexpected verifier output:\n{stdout}");
    let stderr = String::from_utf8(output.stderr).expect("driver error output is UTF-8");
    assert!(stderr.is_empty(), "verification failed:\n{stderr}");
}

#[test]
fn reports_a_failed_obligation_with_a_model() {
    let output = run_fixture("invalid_contract.rs");

    assert!(!output.status.success(), "driver accepted an invalid contract");
    let stdout = String::from_utf8(output.stdout).expect("driver output is UTF-8");
    assert!(stdout.is_empty(), "unexpected verifier output:\n{stdout}");
    let stderr = String::from_utf8(output.stderr).expect("driver error output is UTF-8");
    assert!(stderr.contains("Postcondition 0 failed"), "missing failed VC:\n{stderr}");
    assert!(stderr.contains("define-fun input!0"), "missing counterexample model:\n{stderr}");
}

#[test]
fn rejects_a_loop_without_an_invariant() {
    let output = run_fixture("no_invariant.rs");

    assert!(output.status.success(), "driver failed: {}", String::from_utf8_lossy(&output.stderr));
    let stderr = String::from_utf8(output.stderr).expect("driver error output is UTF-8");
    assert!(
        stderr.contains("requires at least one `#[verifier::invariant(...)]`"),
        "missing invariant diagnostic:\n{stderr}"
    );
    assert!(!stderr.contains("step exploration limit"), "loop was unfolded:\n{stderr}");
}
