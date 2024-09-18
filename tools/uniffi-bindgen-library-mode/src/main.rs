/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

use std::{
    env::consts::{DLL_PREFIX, DLL_SUFFIX},
    fmt,
    process::Command,
};

use anyhow::{bail, Result};
use camino::Utf8PathBuf;
use cargo_metadata::Metadata;
use clap::{Parser, ValueEnum};
use uniffi_bindgen::{
    bindings::{KotlinBindingGenerator, PythonBindingGenerator, SwiftBindingGenerator},
    cargo_metadata::CrateConfigSupplier,
    library_mode::generate_bindings,
};

#[derive(Parser)]
#[command(version, about, long_about = None)]
struct Cli {
    #[arg(value_enum)]
    megazord: Megazord,
    #[arg(value_enum)]
    language: Language,
    out_dir: String,
}

#[derive(Clone, ValueEnum)]
enum Megazord {
    Android,
    Ios,
    IosFocus,
    Cirrus,
    NimbusExperimenter,
}

#[derive(Clone, ValueEnum)]
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
    check_args(&cli)?;
    // TODO: A lot of this code is copy-and-pasted from uniffi-bindgen.  The plan to fix this is to
    // make an 0.28.2 release that exposes a public interface for this.
    let metadata = cargo_metadata::MetadataCommand::new()
        .exec()
        .expect("error running cargo metadata");
    let library_path = build_library(&metadata, &cli.megazord, &cli.language);
    let config_supplier = CrateConfigSupplier::from(metadata);
    match cli.language {
        Language::Kotlin => {
            generate_bindings(
                &library_path,
                None,
                &KotlinBindingGenerator,
                &config_supplier,
                None,
                &Utf8PathBuf::from(cli.out_dir),
                false,
            )?;
        }
        Language::Swift => {
            generate_bindings(
                &library_path,
                None,
                &SwiftBindingGenerator,
                &config_supplier,
                None,
                &Utf8PathBuf::from(cli.out_dir),
                false,
            )?;
        }
        Language::Python => {
            generate_bindings(
                &library_path,
                None,
                &PythonBindingGenerator,
                &config_supplier,
                None,
                &Utf8PathBuf::from(cli.out_dir),
                false,
            )?;
        }
    };
    Ok(())
}

fn check_args(cli: &Cli) -> Result<()> {
    match &cli.megazord {
        Megazord::Ios | Megazord::IosFocus => match &cli.language {
            Language::Swift => Ok(()),
            _ => bail!("{} megazord is only compatible with swift", cli.megazord),
        },
        _ => match &cli.language {
            Language::Swift => bail!("{} megazord is not compatible with swift", cli.megazord),
            _ => Ok(()),
        },
    }
}

/// Build a megazord library and return the path to it
fn build_library(metadata: &Metadata, megazord: &Megazord, language: &Language) -> Utf8PathBuf {
    Command::new(env!("CARGO"))
        .arg("build")
        .arg("--release")
        .arg("--package")
        .arg(megazord.crate_name())
        .spawn()
        .expect("Error running cargo build")
        .wait()
        .expect("Error running cargo build");
    let filename = match language {
        // Swift uses static libs
        Language::Swift => format!("lib{}.a", megazord.crate_name().replace('-', "_")),
        // Everything else uses dynamic librarys
        _ => format!(
            "{}{}{}",
            DLL_PREFIX,
            megazord.crate_name().replace('-', "_"),
            DLL_SUFFIX
        ),
    };
    metadata
        .workspace_root
        .join("target")
        .join("release")
        .join(filename)
}

impl Megazord {
    fn crate_name(&self) -> &'static str {
        match self {
            Self::Android => "megazord",
            Self::Ios => "megazord_ios",
            Self::IosFocus => "megazord_focus",
            Self::Cirrus => "cirrus",
            Self::NimbusExperimenter => "nimbus-experimenter",
        }
    }
}

impl fmt::Display for Megazord {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        let name = match self {
            Self::Android => "android",
            Self::Ios => "ios",
            Self::IosFocus => "ios-focus",
            Self::Cirrus => "cirrus",
            Self::NimbusExperimenter => "nimbus-experimenter",
        };
        write!(f, "{}", name)
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

//     for language in languages {
//         // to help avoid mistakes we check the library is actually a cdylib, except
//         // for swift where static libs are often used to extract the metadata.
//         if !matches!(language, TargetLanguage::Swift) && !uniffi_bindgen::is_cdylib(library_path) {
//             anyhow::bail!(
//                 "Generate bindings for {language} requires a cdylib, but {library_path} was given"
//             );
//         }
//
//         // Type-bounds on trait implementations makes selecting between languages a bit tedious.
//         match language {
//             TargetLanguage::Kotlin => generate_bindings(
//                 library_path,
//                 crate_name.clone(),
//                 &KotlinBindingGenerator,
//                 &config_supplier,
//                 cfo,
//                 out_dir,
//                 fmt,
//             )?
//             .len(),
//             TargetLanguage::Python => generate_bindings(
//                 library_path,
//                 crate_name.clone(),
//                 &PythonBindingGenerator,
//                 &config_supplier,
//                 cfo,
//                 out_dir,
//                 fmt,
//             )?
//             .len(),
//             TargetLanguage::Ruby => generate_bindings(
//                 library_path,
//                 crate_name.clone(),
//                 &RubyBindingGenerator,
//                 &config_supplier,
//                 cfo,
//                 out_dir,
//                 fmt,
//             )?
//             .len(),
//             TargetLanguage::Swift => generate_bindings(
//                 library_path,
//                 crate_name.clone(),
//                 &SwiftBindingGenerator,
//                 &config_supplier,
//                 cfo,
//                 out_dir,
//                 fmt,
//             )?
//             .len(),
//         };
//     }
//     Ok(())
// }
//
