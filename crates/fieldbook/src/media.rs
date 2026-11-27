use std::{env, fs, io};

const MEDIA_ROOT: &str = "/var/data/fieldbook";

pub fn configured_media() -> io::Result<Option<fieldbook::storage::PrivateMedia>> {
    let invalid = |message| io::Error::new(io::ErrorKind::InvalidInput, message);
    match env::var("FIELD_STORAGE").as_deref() {
        Err(env::VarError::NotPresent) => Ok(None),
        Ok("gcs") if env::var("FIELD_GCS_BUCKET").is_ok_and(|name| !name.trim().is_empty()) => {
            Ok(None)
        }
        Ok("gcs") => Err(invalid("set FIELD_GCS_BUCKET for gcs storage")),
        Ok("filesystem") => {
            if env::var("FIELD_MEDIA_ROOT").as_deref() == Ok(MEDIA_ROOT)
                && fs::canonicalize(MEDIA_ROOT).is_ok_and(|path| path.as_os_str() == MEDIA_ROOT)
                && fs::read_to_string("/proc/self/mountinfo")
                    .is_ok_and(|mounts| has_durable_media_mount(&mounts))
            {
                Ok(Some(fieldbook::storage::PrivateMedia::new(
                    MEDIA_ROOT.into(),
                )))
            } else {
                Err(invalid(
                    "filesystem storage requires a read-write persistent mount at /var/data/fieldbook",
                ))
            }
        }
        _ => Err(invalid("FIELD_STORAGE must be filesystem or gcs")),
    }
}

fn has_durable_media_mount(mounts: &str) -> bool {
    // Linux mountinfo lists mount point and options before the separator, filesystem after.
    mounts.lines().any(|line| {
        let Some((mount, filesystem)) = line.split_once(" - ") else {
            return false;
        };
        let mut fields = mount.split_whitespace().skip(4);
        fields.next() == Some(MEDIA_ROOT)
            && fields
                .next()
                .is_some_and(|options| options.split(',').any(|option| option == "rw"))
            && filesystem
                .split_whitespace()
                .next()
                .is_some_and(|kind| !matches!(kind, "tmpfs" | "ramfs" | "overlay"))
    })
}

#[cfg(test)]
mod tests {
    use super::has_durable_media_mount;

    #[test]
    fn requires_a_separate_writable_non_ephemeral_media_mount() {
        assert!(has_durable_media_mount(
            "41 26 8:1 /volume /var/data/fieldbook rw,relatime - ext4 /dev/sda1 rw"
        ));
        for mounts in [
            "41 26 8:1 / / rw,relatime - ext4 /dev/sda1 rw",
            "41 26 0:44 / /var/data/fieldbook rw - tmpfs tmpfs rw",
            "41 26 0:44 / /var/data/fieldbook rw - overlay overlay rw",
            "41 26 8:1 /volume /var/data/fieldbook ro - ext4 /dev/sda1 rw",
        ] {
            assert!(!has_durable_media_mount(mounts));
        }
    }
}
