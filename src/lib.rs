use serde::{Deserialize, Serialize};
use std::{
    collections::HashSet,
    fs::{self, File, OpenOptions},
    io::{Read, Write},
    path::{Path, PathBuf},
};
use zip::{CompressionMethod, ZipArchive, ZipWriter, write::SimpleFileOptions};

const MANIFEST: &str = "hxforge.toml";
const SCHEMA: u32 = 1;

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Manifest {
    pub schema: u32,
    pub id: String,
    pub name: String,
    pub version: String,
}

impl Manifest {
    pub fn new(id: impl Into<String>, name: impl Into<String>) -> Self {
        Self {
            schema: SCHEMA,
            id: id.into(),
            name: name.into(),
            version: "0.1.0".into(),
        }
    }

    pub fn validate(&self) -> Result<(), String> {
        if self.schema != SCHEMA {
            return Err(format!(
                "unsupported HexForge schema {}; expected {SCHEMA}",
                self.schema
            ));
        }
        if self.id.is_empty()
            || !self.id.bytes().all(|byte| {
                byte.is_ascii_lowercase() || byte.is_ascii_digit() || b"_-".contains(&byte)
            })
            || !self.id.as_bytes()[0].is_ascii_lowercase()
            || !self.id.as_bytes()[self.id.len() - 1].is_ascii_lowercase()
                && !self.id.as_bytes()[self.id.len() - 1].is_ascii_digit()
        {
            return Err("package id must start with a lowercase letter and contain only lowercase letters, digits, '_' or '-'".into());
        }
        if self.name.trim().is_empty() {
            return Err("package name cannot be empty".into());
        }
        if self.version.trim().is_empty() {
            return Err("package version cannot be empty".into());
        }
        Ok(())
    }
}

#[derive(Clone, Debug)]
struct BundleEntry {
    archive_path: String,
    kind: PackageKind,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum PackageKind {
    Content,
    Native,
}

impl PackageKind {
    fn from_archive_path(path: &str) -> Result<Self, String> {
        let (prefix, extension, kind) = if let Some(name) = path.strip_prefix("mods/content/") {
            (name, ".hmod", Self::Content)
        } else if let Some(name) = path.strip_prefix("mods/native/") {
            (name, ".hnmod", Self::Native)
        } else {
            return Err(format!("unexpected bundle entry: {path}"));
        };

        if prefix.is_empty()
            || prefix.starts_with('.')
            || !prefix.ends_with(extension)
            || !prefix
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || b"._-".contains(&byte))
        {
            return Err(format!("invalid package path in bundle: {path}"));
        }

        Ok(kind)
    }

    fn destination_dir(self) -> &'static str {
        match self {
            Self::Content => "horizon-mods",
            Self::Native => "horizon-native-mods",
        }
    }
}

fn project_manifest(project: &Path) -> Result<Manifest, String> {
    let path = project.join(MANIFEST);
    let text = fs::read_to_string(&path).map_err(|error| format!("{}: {error}", path.display()))?;
    let manifest: Manifest =
        toml::from_str(&text).map_err(|error| format!("{}: {error}", path.display()))?;
    manifest.validate()?;
    Ok(manifest)
}

fn collect_project_entries(project: &Path) -> Result<Vec<(String, PathBuf, PackageKind)>, String> {
    let mut files = Vec::new();

    for (folder, kind, extension) in [
        ("content", PackageKind::Content, "hmod"),
        ("native", PackageKind::Native, "hnmod"),
    ] {
        let directory = project.join("mods").join(folder);
        if !directory.exists() {
            continue;
        }
        if !directory.is_dir() {
            return Err(format!("{} is not a directory", directory.display()));
        }

        let entries = fs::read_dir(&directory)
            .map_err(|error| format!("{}: {error}", directory.display()))?;
        for entry in entries {
            let entry = entry.map_err(|error| format!("{}: {error}", directory.display()))?;
            let path = entry.path();
            let metadata = fs::symlink_metadata(&path)
                .map_err(|error| format!("{}: {error}", path.display()))?;
            if !metadata.file_type().is_file() {
                return Err(format!("{} must be a regular package file", path.display()));
            }
            if path.extension().and_then(|value| value.to_str()) != Some(extension) {
                return Err(format!(
                    "{} must have the .{extension} extension",
                    path.display()
                ));
            }
            let file_name = path
                .file_name()
                .and_then(|value| value.to_str())
                .ok_or_else(|| format!("{} has an invalid file name", path.display()))?;
            if file_name.starts_with('.')
                || !file_name
                    .bytes()
                    .all(|byte| byte.is_ascii_alphanumeric() || b"._-".contains(&byte))
            {
                return Err(format!(
                    "{} has an invalid package file name",
                    path.display()
                ));
            }

            files.push((format!("mods/{folder}/{file_name}"), path, kind));
        }
    }

    files.sort_by(|left, right| left.0.cmp(&right.0));
    if files.is_empty() {
        return Err(
            "project has no .hmod or .hnmod files under mods/content or mods/native".into(),
        );
    }
    Ok(files)
}

pub fn pack_project(project: &Path, output: Option<&Path>) -> Result<PathBuf, String> {
    let manifest = project_manifest(project)?;
    let files = collect_project_entries(project)?;
    let output = output.map_or_else(
        || {
            project
                .join("target")
                .join(format!("{}.hxforge", manifest.id))
        },
        Path::to_path_buf,
    );
    if let Some(parent) = output.parent() {
        fs::create_dir_all(parent).map_err(|error| format!("{}: {error}", parent.display()))?;
    }

    let file = File::create(&output).map_err(|error| format!("{}: {error}", output.display()))?;
    let mut archive = ZipWriter::new(file);
    let options = SimpleFileOptions::default().compression_method(CompressionMethod::Deflated);
    archive
        .start_file(MANIFEST, options)
        .map_err(|error| error.to_string())?;
    archive
        .write_all(
            toml::to_string_pretty(&manifest)
                .map_err(|error| error.to_string())?
                .as_bytes(),
        )
        .map_err(|error| error.to_string())?;

    for (archive_path, source, _) in files {
        archive
            .start_file(archive_path, options)
            .map_err(|error| error.to_string())?;
        let mut package =
            File::open(&source).map_err(|error| format!("{}: {error}", source.display()))?;
        std::io::copy(&mut package, &mut archive).map_err(|error| error.to_string())?;
    }

    archive.finish().map_err(|error| error.to_string())?;
    Ok(output)
}

fn inspect_bundle(bundle: &Path) -> Result<(Manifest, Vec<BundleEntry>), String> {
    let file = File::open(bundle).map_err(|error| format!("{}: {error}", bundle.display()))?;
    let mut archive =
        ZipArchive::new(file).map_err(|error| format!("{}: {error}", bundle.display()))?;
    let mut manifest = None;
    let mut entries = Vec::new();
    let mut names = HashSet::new();

    for index in 0..archive.len() {
        let mut entry = archive.by_index(index).map_err(|error| error.to_string())?;
        let name = entry.name().to_owned();
        if entry.is_dir() || !names.insert(name.clone()) {
            return Err(format!("duplicate or directory entry in bundle: {name}"));
        }

        if name == MANIFEST {
            let mut text = String::new();
            entry
                .read_to_string(&mut text)
                .map_err(|error| format!("invalid {MANIFEST}: {error}"))?;
            let parsed: Manifest =
                toml::from_str(&text).map_err(|error| format!("invalid {MANIFEST}: {error}"))?;
            parsed.validate()?;
            manifest = Some(parsed);
        } else {
            let kind = PackageKind::from_archive_path(&name)?;
            entries.push(BundleEntry {
                archive_path: name,
                kind,
            });
        }
    }

    let manifest = manifest.ok_or_else(|| format!("bundle is missing {MANIFEST}"))?;
    if entries.is_empty() {
        return Err("bundle contains no Horizon packages".into());
    }
    entries.sort_by(|left, right| left.archive_path.cmp(&right.archive_path));
    Ok((manifest, entries))
}

pub fn check_bundle(bundle: &Path) -> Result<Manifest, String> {
    inspect_bundle(bundle).map(|(manifest, _)| manifest)
}

pub fn install_bundle(bundle: &Path, game_dir: &Path) -> Result<Vec<PathBuf>, String> {
    let (manifest, entries) = inspect_bundle(bundle)?;
    let destinations: Vec<_> = entries
        .iter()
        .map(|entry| {
            let file_name = entry
                .archive_path
                .rsplit('/')
                .next()
                .expect("validated archive paths have a file name");
            game_dir.join(entry.kind.destination_dir()).join(file_name)
        })
        .collect();

    for destination in &destinations {
        if destination.exists() {
            return Err(format!(
                "refusing to overwrite {}; remove it or choose another package file name",
                destination.display()
            ));
        }
    }

    let file = File::open(bundle).map_err(|error| format!("{}: {error}", bundle.display()))?;
    let mut archive = ZipArchive::new(file).map_err(|error| error.to_string())?;
    let mut installed = Vec::new();
    for (entry, destination) in entries.iter().zip(destinations) {
        let result = (|| {
            let parent = destination
                .parent()
                .expect("install destinations always have a parent");
            fs::create_dir_all(parent).map_err(|error| format!("{}: {error}", parent.display()))?;
            let mut source = archive
                .by_name(&entry.archive_path)
                .map_err(|error| error.to_string())?;
            let mut target = OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(&destination)
                .map_err(|error| format!("{}: {error}", destination.display()))?;
            std::io::copy(&mut source, &mut target)
                .map_err(|error| format!("{}: {error}", destination.display()))?;
            Ok::<(), String>(())
        })();

        if let Err(error) = result {
            for path in &installed {
                let _ = fs::remove_file(path);
            }
            return Err(error);
        }
        installed.push(destination);
    }

    if installed.is_empty() {
        return Err(format!(
            "bundle {} had no installable packages",
            manifest.id
        ));
    }
    Ok(installed)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{fs, time::SystemTime};

    fn temp_dir(label: &str) -> PathBuf {
        let unique = SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        std::env::temp_dir().join(format!("hexforge-{label}-{}-{unique}", std::process::id()))
    }

    fn project(root: &Path) {
        fs::create_dir_all(root.join("mods/content")).unwrap();
        fs::create_dir_all(root.join("mods/native")).unwrap();
        fs::write(
            root.join(MANIFEST),
            toml::to_string(&Manifest::new("test_pack", "Test Pack")).unwrap(),
        )
        .unwrap();
        fs::write(root.join("mods/content/content.hmod"), b"content package").unwrap();
        fs::write(root.join("mods/native/native.hnmod"), b"native package").unwrap();
    }

    #[test]
    fn packs_checks_and_installs_horizon_packages() {
        let root = temp_dir("roundtrip");
        project(&root);
        let bundle = pack_project(&root, Some(&root.join("test.hxforge"))).unwrap();
        assert_eq!(check_bundle(&bundle).unwrap().id, "test_pack");

        let game = root.join("game");
        let installed = install_bundle(&bundle, &game).unwrap();
        assert_eq!(installed.len(), 2);
        assert_eq!(
            fs::read(game.join("horizon-mods/content.hmod")).unwrap(),
            b"content package"
        );
        assert_eq!(
            fs::read(game.join("horizon-native-mods/native.hnmod")).unwrap(),
            b"native package"
        );

        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn install_never_overwrites_an_existing_package() {
        let root = temp_dir("no-overwrite");
        project(&root);
        let bundle = pack_project(&root, Some(&root.join("test.hxforge"))).unwrap();
        let game = root.join("game");
        fs::create_dir_all(game.join("horizon-mods")).unwrap();
        fs::write(game.join("horizon-mods/content.hmod"), b"keep me").unwrap();

        assert!(install_bundle(&bundle, &game).is_err());
        assert_eq!(
            fs::read(game.join("horizon-mods/content.hmod")).unwrap(),
            b"keep me"
        );
        assert!(!game.join("horizon-native-mods/native.hnmod").exists());
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn bundle_rejects_paths_outside_package_folders() {
        let root = temp_dir("invalid-path");
        fs::create_dir_all(&root).unwrap();
        let bundle = root.join("bad.hxforge");
        let file = File::create(&bundle).unwrap();
        let mut archive = ZipWriter::new(file);
        let options = SimpleFileOptions::default();
        archive.start_file(MANIFEST, options).unwrap();
        archive
            .write_all(
                toml::to_string(&Manifest::new("test_pack", "Test Pack"))
                    .unwrap()
                    .as_bytes(),
            )
            .unwrap();
        archive.start_file("../outside.hmod", options).unwrap();
        archive.write_all(b"no").unwrap();
        archive.finish().unwrap();

        assert!(check_bundle(&bundle).is_err());
        fs::remove_dir_all(root).unwrap();
    }
}
