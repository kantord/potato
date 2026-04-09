use futures_util::StreamExt;
use spudkit_core::Spud;
use std::path::Path;

fn normalize_build_tag(tag: &str) -> anyhow::Result<String> {
    let spud = Spud::new(tag)?;
    Ok(spud.name().to_string())
}

pub async fn run(tag: &str, path: &Path) -> anyhow::Result<()> {
    let name = normalize_build_tag(tag)?;
    let tar_bytes = build_tar(path)?;
    let docker = bollard::Docker::connect_with_local_defaults()?;
    let image_tag = format!("spud-{name}");
    let options = bollard::query_parameters::BuildImageOptionsBuilder::default()
        .t(image_tag.as_str())
        .build();
    let mut stream = docker.build_image(options, None, Some(bollard::body_full(tar_bytes.into())));
    while let Some(event) = stream.next().await {
        let info = event?;
        if let Some(stream_text) = info.stream {
            print!("{stream_text}");
        }
        if let Some(detail) = info.error_detail {
            let msg = detail
                .message
                .unwrap_or_else(|| "unknown error".to_string());
            anyhow::bail!("docker build error: {msg}");
        }
    }
    Ok(())
}

pub fn build_tar(path: &Path) -> anyhow::Result<Vec<u8>> {
    let buf = Vec::new();
    let mut builder = tar::Builder::new(buf);

    for entry in walkdir::WalkDir::new(path) {
        let entry = entry?;
        if !entry.file_type().is_file() {
            continue;
        }
        let full_path = entry.path();
        let relative_path = full_path.strip_prefix(path)?;
        builder.append_path_with_name(full_path, relative_path)?;
    }

    Ok(builder.into_inner()?)
}

#[cfg(test)]
mod tests {
    use super::{build_tar, normalize_build_tag};
    use std::io::Cursor;

    #[test]
    fn normalize_build_tag_accepts_spud_prefix_as_part_of_name() {
        assert_eq!(
            normalize_build_tag("spud-launcher").unwrap(),
            "spud-launcher"
        );
    }

    #[test]
    fn normalize_build_tag_keeps_plain_name() {
        assert_eq!(normalize_build_tag("hello-world").unwrap(), "hello-world");
    }

    #[test]
    fn normalize_build_tag_rejects_invalid_name() {
        assert!(normalize_build_tag("../etc").is_err());
    }

    #[test]
    fn build_tar_includes_dockerfile() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("Dockerfile"), "FROM scratch\n").unwrap();
        let bytes = build_tar(dir.path()).unwrap();
        let mut archive = tar::Archive::new(Cursor::new(bytes));
        let found = archive
            .entries()
            .unwrap()
            .any(|e| e.unwrap().path().unwrap().to_str() == Some("Dockerfile"));
        assert!(found, "expected an entry named `Dockerfile` in the archive");
    }

    #[test]
    fn build_tar_uses_relative_paths() {
        let dir = tempfile::tempdir().unwrap();
        let subdir = dir.path().join("subdir");
        std::fs::create_dir_all(&subdir).unwrap();
        std::fs::write(subdir.join("file.txt"), "hello\n").unwrap();
        let bytes = build_tar(dir.path()).unwrap();
        let mut archive = tar::Archive::new(Cursor::new(bytes));
        let found = archive.entries().unwrap().any(|e| {
            let entry = e.unwrap();
            let p = entry.path().unwrap();
            p.to_str() == Some("subdir/file.txt")
        });
        assert!(
            found,
            "expected entry with relative path `subdir/file.txt`, got something else"
        );
    }

    #[test]
    fn build_tar_preserves_file_contents() {
        let dir = tempfile::tempdir().unwrap();
        let expected = "FROM ubuntu:22.04\n";
        std::fs::write(dir.path().join("Dockerfile"), expected).unwrap();
        let bytes = build_tar(dir.path()).unwrap();
        let mut archive = tar::Archive::new(Cursor::new(bytes));
        let mut actual = String::new();
        for entry in archive.entries().unwrap() {
            let mut entry = entry.unwrap();
            if entry.path().unwrap().to_str() == Some("Dockerfile") {
                std::io::Read::read_to_string(&mut entry, &mut actual).unwrap();
                break;
            }
        }
        assert_eq!(actual, expected);
    }
}
