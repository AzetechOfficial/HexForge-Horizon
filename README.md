# HexForge

HexForge is a Rust command-line tool and library for bundling and installing mods for [Horizon](https://github.com/horizon-rust/horizon), the Rust Minecraft project. It does not replace Horizon's game engine or mod loader: it packages Horizon's supported content mods (`.hmod`) and native mods (`.hnmod`) into one portable `.hxforge` bundle.

## What it does

- Scaffolds a HexForge modpack project.
- Bundles existing Horizon `.hmod` content mods and `.hnmod` native mods.
- Checks the bundle structure before installation.
- Installs package files into Horizon's `horizon-mods/` and `horizon-native-mods/` folders without overwriting existing files.
- Exposes the same pack, check, and install operations as a Rust library.

HexForge does not compile native mods or load `.hxforge` files inside Horizon. Use Horizon's own tooling and launcher for creating, validating, and building the mods themselves. Native mods contain Rust source that changes the game during the launcher's build, so only install native mods you trust.

HexForge also has an experimental Java bytecode translator for `.jar` files. It translates only a small, self-contained subset of static integer methods; it is not a general Java-to-Rust compiler and does not translate Forge APIs, game hooks, or mod lifecycle behavior.

## Requirements

- Rust stable and Cargo
- Horizon, for playing the installed mods
- Horizon's mod tools for creating and validating Horizon packages

## Install HexForge

From a checkout of this repository:

```sh
cargo install --path .
```

## Create a bundle project

```sh
hxforge init my-pack --id my_pack --name "My Pack"
```

Put Horizon packages directly into the matching folder:

```text
my-pack/
├── hxforge.toml
└── mods/
    ├── content/
    │   └── example.hmod
    └── native/
        └── example.hnmod
```

Both folders are optional. A bundle can contain multiple packages of either kind.

## Build, check, and install

```sh
cd my-pack
hxforge pack
hxforge check target/my_pack.hxforge
hxforge install target/my_pack.hxforge --game-dir "C:\path\to\Horizon\game"
```

On macOS or Linux, pass the path to the Horizon game folder instead. The game folder is the one Horizon uses for `options.txt`. HexForge copies `.hmod` files to `horizon-mods/` and `.hnmod` files to `horizon-native-mods/`. It refuses to overwrite an installed file; remove or rename that file before reinstalling.

`hxforge check` verifies the `.hxforge` manifest and bundle layout. For Horizon's package-level checks, use Horizon's own tooling, such as `cargo horizon-mod check <file>`.

## Experimental Forge JAR translation

```sh
hxforge translate mod.jar translated.rs
```

See the [getting-started guide](docs/getting-started.md) for the exact supported subset and limitations. This is an experimental Forge-first input path, not a Forge API or mod-lifecycle translator; generated Rust is not an `.hnmod`.

## Bundle format

A `.hxforge` file is a ZIP archive containing `hxforge.toml` and one or more Horizon packages under `mods/content/` or `mods/native/`.

```toml
schema = 1
id = "my_pack"
name = "My Pack"
version = "0.1.0"
```

The archive carries Horizon package files unchanged; it does not introduce a new in-game mod API or make `.hxforge` directly loadable by Horizon.

## Rust library API

The `hexforge` crate exports `Manifest`, `pack_project`, `check_bundle`, and `install_bundle`:

```rust,no_run
use std::path::Path;

fn main() -> Result<(), String> {
    let bundle = pack_project(Path::new("my-pack"), None)?;
    let manifest = check_bundle(&bundle)?;
    println!("Built {} {}", manifest.id, manifest.version);
    install_bundle(&bundle, Path::new("path/to/horizon-game"))?;
    Ok(())
}
```

`pack_project` returns the output path. `check_bundle` returns its parsed manifest. `install_bundle` returns the installed package paths and never replaces an existing file.

## Development

```sh
cargo test
cargo run -- --help
```

## License

HexForge is licensed under the MIT License. See [LICENSE](LICENSE).
