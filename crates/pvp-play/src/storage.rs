use std::{
    fs::{self, OpenOptions},
    io::{self, Write},
    path::{Path, PathBuf},
    time::{SystemTime, UNIX_EPOCH},
};

/// Resolve defaults from the executable, never the launcher's working directory.
pub fn default_record_root() -> io::Result<PathBuf> {
    record_root_for_executable(&std::env::current_exe()?)
}

fn record_root_for_executable(executable: &Path) -> io::Result<PathBuf> {
    let directory = executable
        .parent()
        .ok_or_else(|| io::Error::other("Executable has no parent directory"))?;
    // Cargo builds (including integration-test executables) keep recordings in
    // the workspace. A copied standalone executable uses its own directory.
    let base = directory.ancestors().find(|candidate| {
        candidate.join("Cargo.toml").is_file()
            && candidate.join("crates/pvp-play/Cargo.toml").is_file()
    });
    Ok(base.unwrap_or(directory).join("recordings"))
}

/// Keep diagnostics even when the requested recording location is unwritable.
pub fn report_error(preferred_root: &Path, message: &str) -> Option<PathBuf> {
    let fallback = std::env::temp_dir().join("pvp-play");
    for root in [preferred_root, fallback.as_path()] {
        let path = root.join("errors.log");
        if append_error(&path, message).is_ok() {
            return Some(path);
        }
    }
    eprintln!("{message}");
    None
}

fn append_error(path: &Path, message: &str) -> io::Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let mut file = OpenOptions::new().create(true).append(true).open(path)?;
    writeln!(
        file,
        "timestamp={:?} version={}\nexecutable={:?}\nworking_directory={:?}\n{message}\n",
        SystemTime::now().duration_since(UNIX_EPOCH),
        env!("CARGO_PKG_VERSION"),
        std::env::current_exe(),
        std::env::current_dir(),
    )?;
    file.flush()
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    pub fn temp_root(name: &str) -> PathBuf {
        std::env::temp_dir().join(format!(
            "pvp-play-{name}-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ))
    }

    #[test]
    fn default_path_depends_on_executable_location() {
        let workspace = temp_root("workspace-path");
        fs::create_dir_all(workspace.join("crates/pvp-play")).unwrap();
        fs::write(workspace.join("Cargo.toml"), "").unwrap();
        fs::write(workspace.join("crates/pvp-play/Cargo.toml"), "").unwrap();
        for relative in ["target/release/pvp-play.exe", "target/debug/deps/test.exe"] {
            assert_eq!(
                record_root_for_executable(&workspace.join(relative)).unwrap(),
                workspace.join("recordings")
            );
        }
        let standalone = temp_root("standalone-path");
        assert_eq!(
            record_root_for_executable(&standalone.join("pvp-play.exe")).unwrap(),
            standalone.join("recordings")
        );
    }

    #[test]
    fn diagnostics_preserve_messages_and_append() {
        let root = temp_root("diagnostics");
        let path = report_error(&root, "First failure: 保存先").unwrap();
        assert_eq!(report_error(&root, "Second failure"), Some(path.clone()));
        let log = fs::read_to_string(path).unwrap();
        assert!(log.contains("First failure: 保存先"));
        assert!(log.contains("Second failure"));
        assert!(log.contains("working_directory="));
        assert!(log.contains("executable="));
    }
}
