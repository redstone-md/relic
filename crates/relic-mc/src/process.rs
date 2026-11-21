//! Explains why a child process could not be started.

use std::io;
use std::path::Path;

/// Rewrites a spawn failure so that it names the program, the working
/// directory and which of the two is at fault. The operating system reports
/// both a missing program and a missing working directory as the same
/// "No such file or directory".
pub fn spawn_error(program: &Path, dir: &Path, err: io::Error) -> io::Error {
    let reason = if !dir.is_dir() {
        format!("the working directory {} does not exist", dir.display())
    } else if program.components().count() > 1 && !dir.join(program).exists() {
        let from = if program.is_relative() {
            format!(" (a relative path, looked up from {})", dir.display())
        } else {
            String::new()
        };
        format!("the program file does not exist{from}")
    } else if program.components().count() == 1 {
        "the program was not found on PATH".to_owned()
    } else {
        "the file exists; check that it is executable and built for this system".to_owned()
    };
    io::Error::new(
        err.kind(),
        format!(
            "could not start {} in {}: {err}: {reason}",
            program.display(),
            dir.display()
        ),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names_the_missing_part() {
        let dir = tempfile::tempdir().unwrap();
        let not_found = || io::Error::from(io::ErrorKind::NotFound);

        let gone = dir.path().join("gone");
        let text = spawn_error(Path::new("java"), &gone, not_found()).to_string();
        assert!(text.contains("working directory"), "{text}");

        let text = spawn_error(Path::new("runtimes/bin/java"), dir.path(), not_found()).to_string();
        assert!(text.contains("program file does not exist"), "{text}");
        assert!(text.contains("relative path"), "{text}");

        let text = spawn_error(Path::new("java"), dir.path(), not_found()).to_string();
        assert!(text.contains("PATH"), "{text}");
    }
}
