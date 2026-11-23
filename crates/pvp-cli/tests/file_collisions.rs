use std::{
    fs,
    path::PathBuf,
    process::{Command, Output},
    time::{SystemTime, UNIX_EPOCH},
};

struct Workspace(PathBuf);

impl Workspace {
    fn new() -> Self {
        let stamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let path = std::env::temp_dir().join(format!("pvp-cli-{}-{stamp}", std::process::id()));
        fs::create_dir(&path).unwrap();
        Self(path)
    }

    fn run(&self, args: &[&str]) -> Output {
        Command::new(env!("CARGO_BIN_EXE_pvp-sim"))
            .current_dir(&self.0)
            .args(args)
            .output()
            .unwrap()
    }
}

impl Drop for Workspace {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn assert_collision(output: Output, flag: &str) {
    assert_eq!(output.status.code(), Some(2));
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains(&format!("--{flag} and --trace refer to the same file")),
        "{stderr}"
    );
}

#[test]
fn replay_preserves_input_when_trace_aliases_it() {
    let workspace = Workspace::new();
    let input = workspace.0.join("record.json");
    let duel = workspace.run(&["duel", "--max-steps", "1", "--record", "record.json"]);
    assert!(
        duel.status.success(),
        "{}",
        String::from_utf8_lossy(&duel.stderr)
    );
    let original = fs::read(&input).unwrap();
    fs::hard_link(&input, workspace.0.join("alias.json")).unwrap();

    for trace in ["record.json", "./record.json", "alias.json"] {
        assert_collision(
            workspace.run(&["replay", "--input", "record.json", "--trace", trace]),
            "input",
        );
        assert_eq!(fs::read(&input).unwrap(), original);
    }
    let replay = workspace.run(&[
        "replay",
        "--input",
        "record.json",
        "--trace",
        "trace.ndjson",
    ]);
    assert!(
        replay.status.success(),
        "{}",
        String::from_utf8_lossy(&replay.stderr)
    );
    assert!(workspace.0.join("trace.ndjson").metadata().unwrap().len() > 0);
}

#[test]
fn duel_rejects_record_trace_collisions_before_writing() {
    let workspace = Workspace::new();
    let output = workspace.0.join("new/result.json");
    assert_collision(
        workspace.run(&[
            "duel",
            "--max-steps",
            "1",
            "--record",
            "new/result.json",
            "--trace",
            "./new/result.json",
        ]),
        "record",
    );
    assert!(!output.exists());

    let kept = workspace.0.join("kept.json");
    fs::write(&kept, b"keep this content").unwrap();
    fs::hard_link(&kept, workspace.0.join("alias.json")).unwrap();
    assert_collision(
        workspace.run(&[
            "duel",
            "--max-steps",
            "1",
            "--record",
            "kept.json",
            "--trace",
            "alias.json",
        ]),
        "record",
    );
    assert_eq!(fs::read(&kept).unwrap(), b"keep this content");
}
