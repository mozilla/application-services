/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

use std::{
    env::consts::{DLL_PREFIX, DLL_SUFFIX},
    fmt, process,
};

use anyhow::{bail, Result};
use camino::{Utf8Path, Utf8PathBuf};
use clap::{Args, Parser, Subcommand};
use uniffi_bindgen::bindings::{generate_swift_bindings, SwiftBindingsOptions};

#[derive(Parser)]
#[command(version, about, long_about = None)]
struct Cli {
    #[command(flatten)]
    megazord: MegazordArg,
    #[command(subcommand)]
    command: Command,
}

#[derive(Args)]
#[group(required = true, multiple = false)]
struct MegazordArg {
    /// Name of the megazord to use
    #[arg(short, long, value_parser=["megazord", "megazord_ios", "megazord_focus", "cirrus", "nimbus-experimenter"])]
    megazord: Option<String>,

    /// Path to a library file
    #[arg(short, long)]
    library: Option<Utf8PathBuf>,
}

#[derive(Subcommand)]
enum Command {
    Kotlin {
        out_dir: Utf8PathBuf,
    },
    Swift {
        out_dir: Utf8PathBuf,
        /// Generate swift files
        #[arg(long)]
        swift_sources: bool,
        /// Generate header files
        #[arg(long)]
        headers: bool,
        /// Generate modulemap
        #[arg(long)]
        modulemap: bool,
        // Generate an xcframework-compatible modulemap
        #[arg(long)]
        xcframework: bool,
        /// module name for the generated modulemap
        #[arg(long)]
        module_name: Option<String>,
        /// filename for the generate modulemap
        #[arg(long)]
        modulemap_filename: Option<String>,
    },
    Python {
        out_dir: Utf8PathBuf,
    },
}

enum Language {
    Kotlin,
    Swift,
    Python,
}

fn main() {
    if let Err(e) = run_uniffi_bindgen(Cli::parse()) {
        eprintln!("{e}");
        std::process::exit(1);
    }
}

fn run_uniffi_bindgen(cli: Cli) -> Result<()> {
    let metadata = cargo_metadata::MetadataCommand::new()
        .exec()
        .expect("error running cargo metadata");
    let megazord = Megazord::new(
        &cli.megazord,
        cli.command.language(),
        &metadata.workspace_root,
    )?;
    let config_supplier = uniffi::CargoMetadataConfigSupplier::from(metadata);

    match cli.command {
        Command::Kotlin { out_dir } => {
            uniffi::generate_bindings_library_mode(
                &megazord.library_path,
                None,
                &uniffi::KotlinBindingGenerator,
                &config_supplier,
                None,
                &out_dir,
                false,
            )?;
        }
        Command::Swift {
            out_dir,
            mut swift_sources,
            mut headers,
            mut modulemap,
            xcframework,
            module_name,
            modulemap_filename,
        } => {
            let module_name = module_name.unwrap_or_else(|| "MozillaRustComponents".to_owned());
            // If no generate kinds were specified, generate them all
            if !(swift_sources || headers || modulemap) {
                swift_sources = true;
                headers = true;
                modulemap = true;
            }

            generate_swift_bindings(SwiftBindingsOptions {
                out_dir,
                generate_swift_sources: swift_sources,
                generate_headers: headers,
                generate_modulemap: modulemap,
                library_path: megazord.library_path,
                xcframework,
                module_name: Some(module_name),
                modulemap_filename,
                metadata_no_deps: false,
            })?;
        }
        Command::Python { out_dir } => {
            uniffi::generate_bindings_library_mode(
                &megazord.library_path,
                None,
                &uniffi::PythonBindingGenerator,
                &config_supplier,
                None,
                &out_dir,
                false,
            )?;
        }
    };
    Ok(())
}

struct Megazord {
    library_path: Utf8PathBuf,
}

impl Megazord {
    fn new(arg: &MegazordArg, language: Language, workspace_root: &Utf8Path) -> Result<Self> {
        if let Some(crate_name) = &arg.megazord {
            // Build the megazord
            process::Command::new("cargo")
                .args(["build", "--release", "-p", crate_name])
                .spawn()?
                .wait()?;

            let filename = match language {
                // Swift uses static libs
                Language::Swift => format!("lib{}.a", crate_name.replace('-', "_")),
                // Everything else uses dynamic libraries
                _ => format!(
                    "{}{}{}",
                    DLL_PREFIX,
                    crate_name.replace('-', "_"),
                    DLL_SUFFIX
                ),
            };
            let library_path = workspace_root.join("target").join("release").join(filename);
            Ok(Self { library_path })
        } else if let Some(library_path) = &arg.library {
            Ok(Self {
                library_path: library_path.clone(),
            })
        } else {
            bail!("Neither megazord nor library specified")
        }
    }
}

impl fmt::Display for Language {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        let name = match self {
            Self::Swift => "swift",
            Self::Kotlin => "kotlin",
            Self::Python => "python",
        };
        write!(f, "{}", name)
    }
}

impl Command {
    fn language(&self) -> Language {
        match self {
            Self::Kotlin { .. } => Language::Kotlin,
            Self::Swift { .. } => Language::Swift,
            Self::Python { .. } => Language::Python,
        }
    }
}
