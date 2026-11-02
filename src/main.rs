use std::{
    env, fs,
    path::{Path, PathBuf},
    process::ExitCode,
};

use hexforge::{Manifest, check_bundle, install_bundle, pack_project};

fn main() -> ExitCode {
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("error: {error}");
            ExitCode::FAILURE
        }
    }
}

fn run() -> Result<(), String> {
    let mut args = env::args().skip(1);
    match args.next().as_deref() {
        Some("init") => init(args.collect()),
        Some("pack") => pack(args.collect()),
        Some("check") => check(args.collect()),
        Some("install") => install(args.collect()),
        Some("--help") | Some("-h") | None => {
            print_help();
            Ok(())
        }
        Some(command) => Err(format!("unknown command {command:?}; run `hxforge --help`")),
    }
}

fn init(args: Vec<String>) -> Result<(), String> {
    let directory = args
        .first()
        .filter(|value| !value.starts_with("--"))
        .ok_or("usage: hxforge init <directory> [--id ID] [--name NAME] [--version VERSION]")?;
    let directory = PathBuf::from(directory);
    if directory.exists() {
        return Err(format!("{} already exists", directory.display()));
    }
    let base_name = directory
        .file_name()
        .and_then(|value| value.to_str())
        .ok_or("project path must have a valid final folder name")?;
    let id = flag(&args, "--id").map_or_else(|| slug(base_name), str::to_owned);
    let name =
        flag(&args, "--name").map_or_else(|| base_name.replace(['-', '_'], " "), str::to_owned);
    let version = flag(&args, "--version").unwrap_or("0.1.0").to_owned();
    let manifest = Manifest {
        schema: 1,
        id,
        name,
        version,
    };
    manifest
        .validate()
        .map_err(|error| format!("invalid project options: {error}"))?;

    fs::create_dir_all(directory.join("mods/content"))
        .map_err(|error| format!("{}: {error}", directory.display()))?;
    fs::create_dir_all(directory.join("mods/native"))
        .map_err(|error| format!("{}: {error}", directory.display()))?;
    let manifest_path = directory.join("hxforge.toml");
    fs::write(
        &manifest_path,
        toml::to_string_pretty(&manifest).map_err(|error| error.to_string())?,
    )
    .map_err(|error| format!("{}: {error}", manifest_path.display()))?;
    println!("Created {}", directory.display());
    Ok(())
}

fn pack(args: Vec<String>) -> Result<(), String> {
    if args.len() > 2 {
        return Err("usage: hxforge pack [project-directory] [output.hxforge]".into());
    }
    let project = args
        .first()
        .map_or_else(|| Path::new(".").to_path_buf(), PathBuf::from);
    let output = args.get(1).map(Path::new);
    let bundle = pack_project(&project, output)?;
    println!("Built {}", bundle.display());
    Ok(())
}

fn check(args: Vec<String>) -> Result<(), String> {
    if args.len() != 1 {
        return Err("usage: hxforge check <bundle.hxforge>".into());
    }
    let path = Path::new(&args[0]);
    let manifest = check_bundle(path)?;
    println!(
        "Valid HexForge bundle: {} ({}) version {}",
        manifest.name, manifest.id, manifest.version
    );
    Ok(())
}

fn install(args: Vec<String>) -> Result<(), String> {
    let bundle = args
        .first()
        .filter(|value| !value.starts_with("--"))
        .ok_or("usage: hxforge install <bundle.hxforge> --game-dir <Horizon game folder>")?;
    let game_dir = flag(&args, "--game-dir")
        .ok_or("missing --game-dir; specify the Horizon game folder that contains options.txt")?;
    let installed = install_bundle(Path::new(bundle), Path::new(game_dir))?;
    for path in installed {
        println!("Installed {}", path.display());
    }
    Ok(())
}

fn flag<'a>(args: &'a [String], name: &str) -> Option<&'a str> {
    args.windows(2)
        .find(|pair| pair[0] == name)
        .map(|pair| pair[1].as_str())
}

fn slug(value: &str) -> String {
    value
        .chars()
        .map(|character| {
            if character.is_ascii_alphanumeric() {
                character.to_ascii_lowercase()
            } else {
                '_'
            }
        })
        .collect()
}

fn print_help() {
    println!(
        "HexForge — bundle and install Horizon mods\n\
         \n\
         Usage:\n\
           hxforge init <directory> [--id ID] [--name NAME] [--version VERSION]\n\
           hxforge pack [project-directory] [output.hxforge]\n\
           hxforge check <bundle.hxforge>\n\
           hxforge install <bundle.hxforge> --game-dir <Horizon game folder>"
    );
}
