use std::{
    fs::{self, File, OpenOptions},
    io::{self, Read, Seek, SeekFrom, Write},
    os::unix::fs::OpenOptionsExt,
    path::{Path, PathBuf},
};

use sha2::{Digest, Sha256};
use uuid::Uuid;

#[derive(Clone)]
pub struct PrivateMedia(PathBuf);

impl PrivateMedia {
    pub fn new(root: PathBuf) -> Self {
        Self(root)
    }

    fn stage(&self, id: Uuid) -> PathBuf {
        self.0.join(format!("staging-{id}"))
    }

    fn original(&self, id: Uuid) -> PathBuf {
        self.0.join(original_key(id))
    }

    pub(super) fn append(&self, id: Uuid, received: u64, bytes: &[u8]) -> io::Result<()> {
        let mut options = OpenOptions::new();
        options.write(true).mode(0o600).create(received == 0);
        let mut file = options.open(self.stage(id))?;
        if file.metadata()?.len() < received {
            return Err(io::Error::other(
                "staging file shorter than committed offset",
            ));
        }
        // Discard bytes written by a request that did not commit its offset.
        file.set_len(received)?;
        file.seek(SeekFrom::Start(received))?;
        file.write_all(bytes)?;
        file.sync_all()?;
        self.sync_root()
    }

    pub(super) fn matches(
        &self,
        id: Uuid,
        ready: bool,
        offset: u64,
        bytes: &[u8],
    ) -> io::Result<bool> {
        let path = if ready {
            self.original(id)
        } else {
            self.stage(id)
        };
        let mut file = File::open(path)?;
        file.seek(SeekFrom::Start(offset))?;
        let mut previous = vec![0; bytes.len()];
        file.read_exact(&mut previous)?;
        Ok(previous == bytes)
    }

    pub(super) fn publish(
        &self,
        id: Uuid,
        count: u64,
        hash: &[u8],
        media_type: &str,
    ) -> io::Result<Verification> {
        let stage = self.stage(id);
        if !verify(&stage, count, hash)? {
            return Ok(Verification::ChecksumMismatch);
        }
        let mut prefix = Vec::with_capacity(16);
        File::open(&stage)?.take(16).read_to_end(&mut prefix)?;
        if matches_type(media_type, &prefix) != Some(true) {
            return Ok(Verification::TypeMismatch);
        }

        let original = self.original(id);
        match fs::hard_link(&stage, &original) {
            Ok(()) => {}
            Err(error) if error.kind() == io::ErrorKind::AlreadyExists => {
                // The original can survive a lost database commit. Adopt only verified bytes.
                if !verify(&original, count, hash)? {
                    return Err(io::Error::other("existing original failed verification"));
                }
            }
            Err(error) => return Err(error),
        }
        self.sync_root()?;
        Ok(Verification::Valid)
    }

    pub(super) fn clean(&self, id: Uuid, ready: bool) -> io::Result<()> {
        remove(&self.stage(id))?;
        if !ready {
            remove(&self.original(id))?;
        }
        self.sync_root()
    }

    fn sync_root(&self) -> io::Result<()> {
        File::open(&self.0)?.sync_all()
    }
}

pub(super) enum Verification {
    Valid,
    ChecksumMismatch,
    TypeMismatch,
}

// Recognized types receive a signature check. Opaque project/source files stay passive.
pub(super) fn matches_type(media_type: &str, prefix: &[u8]) -> Option<bool> {
    Some(match media_type {
        "application/octet-stream" => true,
        "application/pdf" => prefix.starts_with(b"%PDF-"),
        "application/zip" => prefix.starts_with(b"PK\x03\x04") || prefix.starts_with(b"PK\x05\x06"),
        "image/jpeg" => prefix.starts_with(b"\xff\xd8\xff"),
        "image/png" => prefix.starts_with(b"\x89PNG\r\n\x1a\n"),
        "image/webp" => prefix.starts_with(b"RIFF") && prefix.get(8..12) == Some(b"WEBP"),
        "video/mp4" | "video/quicktime" => prefix.get(4..8) == Some(b"ftyp"),
        "video/webm" => prefix.starts_with(b"\x1a\x45\xdf\xa3"),
        _ => return None,
    })
}

pub(super) fn original_key(id: Uuid) -> String {
    format!("original-{id}")
}

fn verify(path: &Path, count: u64, hash: &[u8]) -> io::Result<bool> {
    let file = File::open(path)?;
    if file.metadata()?.len() != count {
        return Ok(false);
    }
    let mut digest = Sha256::new();
    let copied = io::copy(&mut file.take(count.saturating_add(1)), &mut digest)?;
    Ok(copied == count && &digest.finalize()[..] == hash)
}

fn remove(path: &Path) -> io::Result<()> {
    match fs::remove_file(path) {
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(()),
        result => result,
    }
}
