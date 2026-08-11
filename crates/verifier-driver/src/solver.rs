use std::{
    io::Write,
    process::{Command, Stdio},
};

pub(crate) fn check(script: &str) -> Result<Option<String>, String> {
    let mut child = Command::new("z3")
        .arg("-in")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|error| format!("failed to start z3: {error}"))?;

    let write = child
        .stdin
        .take()
        .ok_or_else(|| "failed to open z3 stdin".to_owned())?
        .write_all(script.as_bytes());
    let output =
        child.wait_with_output().map_err(|error| format!("failed to wait for z3: {error}"))?;
    write.map_err(|error| format!("failed to write SMT script to z3: {error}"))?;

    let stdout = String::from_utf8(output.stdout)
        .map_err(|error| format!("z3 produced non-UTF-8 output: {error}"))?;
    let stderr = String::from_utf8_lossy(&output.stderr);
    let mut lines = stdout.lines();
    match lines.next() {
        Some("unsat") => Ok(None),
        Some("sat") => {
            let start = stdout.find('\n').map_or(stdout.len(), |index| index + 1);
            Ok(Some(stdout[start..].trim_end().to_owned()))
        }
        Some("unknown") => Err("z3 could not determine whether the obligation is valid".to_owned()),
        _ if !output.status.success() => {
            Err(format!("z3 exited with {}\n{}\n{}", output.status, stdout.trim(), stderr.trim()))
        }
        Some(output) => Err(format!("unexpected z3 output: {output}")),
        None => Err(format!("z3 produced no result: {}", stderr.trim())),
    }
}
