use anyhow::{Context, Result};
use std::ffi::OsString;
use std::fs::{self, OpenOptions};
use std::io::{Read, Write};
use std::os::unix::fs::OpenOptionsExt;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

static TEMP_SEQUENCE: AtomicU64 = AtomicU64::new(0);

pub fn is_executable(path: &Path) -> bool {
    path.metadata()
        .is_ok_and(|metadata| metadata.is_file() && metadata.permissions().mode() & 0o111 != 0)
}

pub fn command_exists(program: &str) -> bool {
    let path = Path::new(program);
    if path.components().count() > 1 {
        return is_executable(path);
    }
    std::env::var_os("PATH")
        .map(|paths| std::env::split_paths(&paths).any(|dir| is_executable(&dir.join(program))))
        .unwrap_or(false)
}

/// Snapshot absence only on NotFound; unreadable originals must stop a transaction.
pub fn snapshot(path: &Path) -> Result<Option<Vec<u8>>> {
    match fs::read(path) {
        Ok(bytes) => Ok(Some(bytes)),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(error).with_context(|| format!("snapshot {}", path.display())),
    }
}

pub fn restore(path: &Path, contents: Option<&[u8]>) -> Result<()> {
    match contents {
        Some(bytes) => atomic_write(path, bytes),
        None => match fs::remove_file(path) {
            Ok(()) => Ok(()),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(error) => Err(error).context("remove newly created configuration"),
        },
    }
}

/// Nonblocking open rejects FIFOs/devices before reading; the descriptor's
/// metadata and byte limit also cover file replacement and concurrent growth.
pub fn read_text_bounded(path: &Path, limit: usize) -> Result<String> {
    let file = OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_NONBLOCK | libc::O_NOFOLLOW)
        .open(path)?;
    let metadata = file.metadata()?;
    anyhow::ensure!(metadata.is_file(), "expected a regular file");
    anyhow::ensure!(metadata.len() <= limit as u64, "file exceeds {limit} bytes");
    let mut bytes = Vec::new();
    file.take(limit as u64 + 1).read_to_end(&mut bytes)?;
    anyhow::ensure!(bytes.len() <= limit, "file exceeds {limit} bytes");
    String::from_utf8(bytes).context("file is not UTF-8")
}

pub fn atomic_write(path: &Path, contents: &[u8]) -> Result<()> {
    let parent = parent_dir(path);
    fs::create_dir_all(parent)?;
    let temporary = unique_temporary_path(path)?;
    let result = (|| -> Result<()> {
        let mut file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&temporary)
            .with_context(|| format!("create temporary file {}", temporary.display()))?;
        file.write_all(contents)?;
        file.sync_all()?;
        fs::rename(&temporary, path)?;
        fs::File::open(parent)?.sync_all()?;
        Ok(())
    })();
    if result.is_err() {
        let _ = fs::remove_file(&temporary);
    }
    result
}

pub fn atomic_write_noreplace(path: &Path, contents: &[u8]) -> Result<()> {
    let parent = parent_dir(path);
    fs::create_dir_all(parent)?;
    let temporary = unique_temporary_path(path)?;
    let result = (|| -> Result<()> {
        let mut file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&temporary)
            .with_context(|| format!("create temporary file {}", temporary.display()))?;
        file.write_all(contents)?;
        file.sync_all()?;
        rename_noreplace(&temporary, path)
    })();
    if result.is_err() {
        let _ = fs::remove_file(&temporary);
    }
    result
}

pub fn rename_noreplace(from: &Path, to: &Path) -> Result<()> {
    use std::os::unix::ffi::OsStrExt;

    let from_c = std::ffi::CString::new(from.as_os_str().as_bytes())?;
    let to_c = std::ffi::CString::new(to.as_os_str().as_bytes())?;
    let result = unsafe {
        libc::renameat2(
            libc::AT_FDCWD,
            from_c.as_ptr(),
            libc::AT_FDCWD,
            to_c.as_ptr(),
            libc::RENAME_NOREPLACE,
        )
    };
    if result != 0 {
        return Err(std::io::Error::last_os_error()).context("publish without replacement");
    }
    fs::File::open(parent_dir(to))?.sync_all()?;
    Ok(())
}

fn unique_temporary_path(path: &Path) -> Result<PathBuf> {
    let file_name = path
        .file_name()
        .context("atomic-write destination has no file name")?;
    for _ in 0..64 {
        let sequence = TEMP_SEQUENCE.fetch_add(1, Ordering::Relaxed);
        let mut name = OsString::from(".");
        name.push(file_name);
        name.push(format!(".{}.{}.tmp", std::process::id(), sequence));
        let candidate = path.with_file_name(name);
        match fs::symlink_metadata(&candidate) {
            Ok(_) => continue,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(candidate),
            Err(error) => return Err(error).context("inspect atomic-write temporary path"),
        }
    }
    anyhow::bail!("could not allocate a temporary file for {}", path.display())
}

fn parent_dir(path: &Path) -> &Path {
    path.parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn atomic_write_replaces_contents_without_predictable_sidecar() {
        let base = std::env::temp_dir().join(format!("shsynth-fsutil-{}", std::process::id()));
        let _ = fs::remove_dir_all(&base);
        let path = base.join("settings.conf");
        atomic_write(&path, b"first").unwrap();
        atomic_write(&path, b"second").unwrap();
        assert_eq!(fs::read(&path).unwrap(), b"second");
        assert_eq!(fs::read_dir(&base).unwrap().count(), 1);
        let _ = fs::remove_dir_all(base);
    }

    #[test]
    fn atomic_noreplace_keeps_an_existing_destination() {
        let base = std::env::temp_dir().join(format!("shsynth-noreplace-{}", std::process::id()));
        let _ = fs::remove_dir_all(&base);
        let path = base.join("project.shsong");
        atomic_write(&path, b"original").unwrap();

        assert!(atomic_write_noreplace(&path, b"replacement").is_err());
        assert_eq!(fs::read(&path).unwrap(), b"original");
        assert_eq!(fs::read_dir(&base).unwrap().count(), 1);
        let _ = fs::remove_dir_all(base);
    }
    #[test]
    fn bounded_reader_rejects_sparse_files_and_nonregular_inputs() {
        let base = std::env::temp_dir().join(format!("shr-bounded-read-{}", std::process::id()));
        fs::create_dir_all(&base).unwrap();
        let file = base.join("input");
        fs::write(&file, b"1234").unwrap();
        assert_eq!(read_text_bounded(&file, 4).unwrap(), "1234");
        assert!(read_text_bounded(&file, 3).is_err());
        fs::File::create(&file).unwrap().set_len(1 << 32).unwrap();
        assert!(read_text_bounded(&file, 16).is_err());
        assert!(read_text_bounded(&base, 16).is_err());
        assert!(snapshot(&base).is_err());
        assert!(snapshot(&base.join("absent")).unwrap().is_none());
        assert!(restore(&base, Some(b"cannot replace directory")).is_err());
        fs::remove_dir_all(base).unwrap();
    }
}
