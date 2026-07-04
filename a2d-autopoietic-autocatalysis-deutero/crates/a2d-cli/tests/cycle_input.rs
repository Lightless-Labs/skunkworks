use std::process::{Command, Stdio};

#[test]
fn cycle_input_requires_json_object_before_running_cycle() {
    let output = Command::new(env!("CARGO_BIN_EXE_a2d"))
        .args(["cycle-input", "-"])
        .stdin(Stdio::piped())
        .output()
        .expect("run cycle-input with stdin");

    assert_eq!(output.status.code(), Some(1));
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(!stdout.contains("A²D Catalytic Cycle"), "{stdout}");
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("cycle-input requires a JSON object artifact bundle"),
        "{stderr}"
    );
}

#[test]
fn cycle_input_rejects_reserved_runtime_artifacts_before_running_cycle() {
    let mut child = Command::new(env!("CARGO_BIN_EXE_a2d"))
        .args(["cycle-input", "-"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn cycle-input");
    {
        use std::io::Write;
        let stdin = child.stdin.as_mut().expect("stdin is piped");
        stdin
            .write_all(br#"{"requirements":"REQS","fitness_report":{}}"#)
            .expect("write cycle input");
    }
    let output = child.wait_with_output().expect("wait for cycle-input");

    assert_eq!(output.status.code(), Some(1));
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(!stdout.contains("A²D Catalytic Cycle"), "{stdout}");
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("reserved runtime artifact"), "{stderr}");
    assert!(stderr.contains("fitness_report"), "{stderr}");
}
