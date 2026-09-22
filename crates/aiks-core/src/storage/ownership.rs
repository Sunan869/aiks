//! Cooperative process ownership for the business database. This is not a
//! filesystem sandbox and cannot stop unrelated SQLite applications.

use std::fs::{File, OpenOptions};
use std::path::{Path, PathBuf};

use anyhow::{bail, Context};
use fs2::FileExt;

pub struct BusinessDbLease {
    // Keep the handle alive; closing it (including process death) releases the
    // OS lock. Never unlink a lock file while another process could hold it.
    _file: File,
    database_path: PathBuf,
}

impl BusinessDbLease {
    pub fn acquire(path: &Path) -> anyhow::Result<Self> {
        let absolute = if path.is_absolute() {
            path.to_path_buf()
        } else {
            std::env::current_dir()?.join(path)
        };
        let name = absolute
            .file_name()
            .context("Database filename is required")?;
        let parent = absolute.parent().context("Database parent is required")?;
        for component in parent.ancestors() {
            reject_link(component, false)?;
        }
        std::fs::create_dir_all(parent).context("Cannot create database directory")?;
        let database_path = parent.canonicalize()?.join(name);
        reject_link(&database_path, true)?;
        let mut lock_name = database_path.as_os_str().to_os_string();
        lock_name.push(".aiks-lock");
        let lock_path = PathBuf::from(lock_name);
        reject_link(&lock_path, true)?;
        let mut options = OpenOptions::new();
        options.read(true).write(true).create(true).truncate(false);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600);
        }
        let file = options
            .open(&lock_path)
            .context("Cannot open database ownership lock")?;
        FileExt::try_lock_exclusive(&file)
            .context("Business database is already owned by another process")?;
        // Check again after acquiring ownership, before any SQLite operation.
        reject_link(&database_path, true)?;
        Ok(Self {
            _file: file,
            database_path,
        })
    }

    pub(crate) fn database_path(&self) -> &Path {
        &self.database_path
    }
}

fn reject_link(path: &Path, must_be_file: bool) -> anyhow::Result<()> {
    let metadata = match std::fs::symlink_metadata(path) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(error) => return Err(error.into()),
    };
    if metadata.file_type().is_symlink() {
        bail!("Linked database paths are not supported");
    }
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        if metadata.file_attributes() & 0x400 != 0 {
            bail!("Reparse-point database paths are not supported");
        }
    }
    if must_be_file && !metadata.is_file() {
        bail!("Database and lock paths must be regular files");
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        if must_be_file && metadata.nlink() > 1 {
            bail!("Hard-linked database paths are not supported");
        }
    }
    Ok(())
}
