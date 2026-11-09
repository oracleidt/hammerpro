use std::fs::OpenOptions;
use std::io::{Read, Write};
use std::path::Path;

use anyhow::{Result, ensure};
use zeroize::Zeroizing;

pub fn load_or_create(path: &Path) -> Result<Zeroizing<[u8; 32]>> {
    let mut options = OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    match options.open(path) {
        Ok(mut file) => {
            let key = Zeroizing::new(crate::protocol::random_secret()?);
            file.write_all(key.as_ref())?;
            file.sync_all()?;
            Ok(key)
        }
        Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {
            let mut bytes = Zeroizing::new(Vec::new());
            std::fs::File::open(path)?.take(33).read_to_end(&mut bytes)?;
            ensure!(bytes.len() == 32, "Invalid local collaboration identity file");
            let mut key = Zeroizing::new([0; 32]);
            key.copy_from_slice(&bytes);
            Ok(key)
        }
        Err(error) => Err(error.into()),
    }
}
