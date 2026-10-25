//! Swapping the running executable for a downloaded, verified one.
//!
//! The new binary is staged as a sibling of the executable so the final step
//! is a rename on the same filesystem. Staging happens before any download,
//! so an install directory Probe cannot write to fails straight away.

use std::fs;
use std::io::{self, Write};
use std::path::{Path, PathBuf};

use sha2::{Digest, Sha256};

use super::UpdateError;

/// A not-yet-installed binary next to the executable it will replace.
/// Removed on drop unless [`install`] moved it into place.
pub struct Staged {
    path: PathBuf,
    file: Option<fs::File>,
}

impl Drop for Staged {
    fn drop(&mut self) {
        self.file = None;
        let _ = fs::remove_file(&self.path);
    }
}

/// Create the staging file `<exe>.update` next to `exe`.
pub fn stage_next_to(exe: &Path) -> Result<Staged, UpdateError> {
    let path = sibling(exe, "update")?;
    // A leftover from an interrupted attempt would make `create_new` fail.
    match fs::remove_file(&path) {
        Ok(()) => {}
        Err(error) if error.kind() == io::ErrorKind::NotFound => {}
        Err(error) => return Err(staging_error(&path, error)),
    }
    let file = create_executable(&path).map_err(|error| staging_error(&path, error))?;
    Ok(Staged {
        path,
        file: Some(file),
    })
}

fn staging_error(path: &Path, error: io::Error) -> UpdateError {
    UpdateError::Install(format!(
        "cannot write next to the executable ({}): {error}",
        path.display()
    ))
}

#[cfg(unix)]
fn create_executable(path: &Path) -> io::Result<fs::File> {
    use std::os::unix::fs::OpenOptionsExt;
    fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o755)
        .open(path)
}

#[cfg(not(unix))]
fn create_executable(path: &Path) -> io::Result<fs::File> {
    fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)
}

/// The creation mode is filtered through the umask; set it explicitly so a
/// restrictive umask can't produce a binary only its owner may run.
#[cfg(unix)]
fn make_executable(path: &Path) -> io::Result<()> {
    use std::os::unix::fs::PermissionsExt;
    fs::set_permissions(path, fs::Permissions::from_mode(0o755))
}

#[cfg(not(unix))]
fn make_executable(_path: &Path) -> io::Result<()> {
    Ok(())
}

/// Verify `bytes` against `expected_sha256`, write them to `staged`, and
/// move the result over `exe`. On a hash mismatch or any IO failure `exe`
/// is left untouched and the staging file is removed.
pub fn install(
    exe: &Path,
    mut staged: Staged,
    bytes: &[u8],
    expected_sha256: &str,
) -> Result<(), UpdateError> {
    let actual = sha256_hex(bytes);
    if actual != expected_sha256 {
        return Err(UpdateError::Integrity(format!(
            "checksum mismatch: expected {expected_sha256}, got {actual}"
        )));
    }

    let Some(mut file) = staged.file.take() else {
        return Err(UpdateError::Install("staging file already used".to_owned()));
    };
    file.write_all(bytes)
        .and_then(|()| file.sync_all())
        .and_then(|()| make_executable(&staged.path))
        .map_err(|error| UpdateError::Install(format!("writing the new binary: {error}")))?;
    drop(file);

    replace(exe, &staged.path)
        .map_err(|error| UpdateError::Install(format!("replacing the executable: {error}")))
}

/// Lowercase hex sha256 of `bytes`.
pub fn sha256_hex(bytes: &[u8]) -> String {
    Sha256::digest(bytes)
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

/// Unix lets a running executable be renamed over: the process keeps the
/// old inode and the next launch picks up the new file.
#[cfg(not(windows))]
fn replace(exe: &Path, new: &Path) -> io::Result<()> {
    fs::rename(new, exe)
}

/// Windows will not overwrite or delete a running executable but does allow
/// renaming it, so move it aside to `<exe>.old` first. That file is removed
/// on a later start by [`remove_leftovers`].
#[cfg(windows)]
fn replace(exe: &Path, new: &Path) -> io::Result<()> {
    let old = sibling(exe, "old").map_err(|error| io::Error::other(error.to_string()))?;
    match fs::remove_file(&old) {
        Ok(()) => {}
        Err(error) if error.kind() == io::ErrorKind::NotFound => {}
        Err(error) => return Err(error),
    }
    fs::rename(exe, &old)?;
    if let Err(error) = fs::rename(new, exe) {
        // Put the original back so Probe still starts next time.
        let _ = fs::rename(&old, exe);
        return Err(error);
    }
    Ok(())
}

/// Delete what an earlier update left beside `exe`: the replaced Windows
/// binary, and a staging file from an attempt that never finished. Failure
/// is harmless (the old process may still be exiting) and retried next start.
pub fn remove_leftovers(exe: &Path) {
    for suffix in ["old", "update"] {
        if let Ok(path) = sibling(exe, suffix) {
            match fs::remove_file(&path) {
                Ok(()) => tracing::info!("removed update leftover {}", path.display()),
                Err(error) if error.kind() == io::ErrorKind::NotFound => {}
                Err(error) => tracing::debug!("could not remove {}: {error}", path.display()),
            }
        }
    }
}

/// `<exe>.<suffix>` in the executable's directory.
fn sibling(exe: &Path, suffix: &str) -> Result<PathBuf, UpdateError> {
    let name = exe
        .file_name()
        .ok_or_else(|| UpdateError::Install(format!("not a file path: {}", exe.display())))?;
    let mut name = name.to_os_string();
    name.push(".");
    name.push(suffix);
    Ok(exe.with_file_name(name))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_dir() -> PathBuf {
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0);
        let dir = std::env::temp_dir().join(format!("probe-update-{nanos}-{}", std::process::id()));
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn fake_exe(dir: &Path) -> PathBuf {
        let exe = dir.join("probe");
        fs::write(&exe, b"old binary").unwrap();
        exe
    }

    #[test]
    fn hashes_as_lowercase_hex() {
        assert_eq!(
            sha256_hex(b"abc"),
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
    }

    #[test]
    fn verified_binary_replaces_the_executable() {
        let dir = temp_dir();
        let exe = fake_exe(&dir);
        let new = b"new binary";

        let staged = stage_next_to(&exe).unwrap();
        install(&exe, staged, new, &sha256_hex(new)).unwrap();

        assert_eq!(fs::read(&exe).unwrap(), new);
        assert!(!dir.join("probe.update").exists());
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mode = fs::metadata(&exe).unwrap().permissions().mode();
            assert_eq!(mode & 0o777, 0o755);
        }
        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn checksum_mismatch_leaves_the_executable_untouched() {
        let dir = temp_dir();
        let exe = fake_exe(&dir);

        let staged = stage_next_to(&exe).unwrap();
        let error = install(&exe, staged, b"tampered", &sha256_hex(b"genuine")).unwrap_err();

        assert!(matches!(error, UpdateError::Integrity(_)), "{error}");
        assert_eq!(fs::read(&exe).unwrap(), b"old binary");
        assert!(!dir.join("probe.update").exists());
        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn staging_replaces_a_leftover_from_an_interrupted_attempt() {
        let dir = temp_dir();
        let exe = fake_exe(&dir);
        fs::write(dir.join("probe.update"), b"half-written").unwrap();

        let staged = stage_next_to(&exe).unwrap();
        install(&exe, staged, b"new", &sha256_hex(b"new")).unwrap();

        assert_eq!(fs::read(&exe).unwrap(), b"new");
        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn staging_in_a_missing_directory_fails_before_any_download() {
        let dir = temp_dir();
        let exe = dir.join("no-such-dir").join("probe");
        assert!(matches!(stage_next_to(&exe), Err(UpdateError::Install(_))));
        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn leftovers_are_removed_and_absence_is_fine() {
        let dir = temp_dir();
        let exe = fake_exe(&dir);
        fs::write(dir.join("probe.old"), b"x").unwrap();
        fs::write(dir.join("probe.update"), b"x").unwrap();

        remove_leftovers(&exe);
        remove_leftovers(&exe);

        assert!(!dir.join("probe.old").exists());
        assert!(!dir.join("probe.update").exists());
        assert!(exe.exists());
        fs::remove_dir_all(&dir).unwrap();
    }
}
