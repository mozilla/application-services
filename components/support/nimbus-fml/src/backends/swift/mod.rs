/* This Source Code Form is subject to the terms of the Mozilla Public
* License, v. 2.0. If a copy of the MPL was not distributed with this
* file, You can obtain one at http://mozilla.org/MPL/2.0/. */

use crate::error::Result;
use askama::Template;

use crate::intermediate_representation::FeatureManifest;
use crate::{Config, GenerateStructCmd};

mod gen_structs;

pub(crate) fn generate_struct(
    manifest: FeatureManifest,
    config: Config,
    cmd: GenerateStructCmd,
) -> Result<()> {
    let kt = gen_structs::FeatureManifestDeclaration::new(config, &manifest);

    let path = cmd.output;
    let contents = kt.render()?;

    std::fs::write(path, contents)?;

    Ok(())
}



#[cfg(test)]
pub mod test {
    use std::{process::Command, path::{Path, PathBuf}, ffi::OsString};
    use anyhow::{Result, Context, bail};
    use tempdir::TempDir;

    use crate::{Config, error::FMLError};

    pub fn compile_manifest_swift(manifest_file: &Path, out_dir: &Path) -> Result<()> {
        let out_path = PathBuf::from(out_dir);
        let manifest_file = PathBuf::from(manifest_file);
        let mut dylib_file = out_path.clone();
        dylib_file.push(format!("lib{}.dylib", "FeatureManifest"));

        // `-emit-library -o <path>` generates a `.dylib`, so that we can use the
        // Swift module from the REPL. Otherwise, we'll get "Couldn't lookup
        // symbols" when we try to import the module.
        // See https://bugs.swift.org/browse/SR-1191.
    
        let status = Command::new("swiftc")
            .arg("-module-name")
            .arg("FeatureManifest")
            .arg("-emit-library")
            .arg("-o")
            .arg(&dylib_file)
            .arg("-emit-module")
            .arg("-emit-module-path")
            .arg(&out_path)
            .arg("-parse-as-library")
            .arg("-L")
            .arg(&out_path)
            .arg(manifest_file)
            .spawn()
            .context("Failed to spawn `swiftc` when compiling bindings")?
            .wait()
            .context("Failed to wait for `swiftc` when compiling bindings")?;
        if !status.success() {
            bail!("running `swiftc` failed")
        }
        Ok(())
    }
    
    pub fn run_script(out_dir: &Path, script_file: &Path) -> Result<()> {
        let mut cmd = Command::new("swift");
    
        // Find any module maps and/or dylibs in the target directory, and tell swift to use them.
        // Listing the directory like this is a little bit hacky - it would be nicer if we could tell
        // Swift to load only the module(s) for the component under test, but the way we're calling
        // this test function doesn't allow us to pass that name in to the call.
    
        cmd.arg("-I").arg(out_dir).arg("-L").arg(out_dir);
        for entry in PathBuf::from(out_dir)
            .read_dir()
            .context("Failed to list target directory when running script")?
        {
            let entry = entry.context("Failed to list target directory when running script")?;
            if let Some(ext) = entry.path().extension() {
                 if ext == "dylib" || ext == "so" {
                    let mut option = OsString::from("-l");
                    option.push(entry.path());
                    cmd.arg(option);
                }
            }
        }
        cmd.arg(script_file);
    
        let status = cmd
            .spawn()
            .context("Failed to spawn `swift` when running script")?
            .wait()
            .context("Failed to wait for `swift` when running script")?;
        if !status.success() {
            bail!("running `swift` failed")
        }
        Ok(())
    }

    pub fn run_script_with_generated_code(manifest_file: &Path, script: &Path) -> Result<()> {
        let path = PathBuf::from(&manifest_file);
        let prefix = path
            .file_stem()
            .ok_or_else(|| FMLError::InvalidPath(manifest_file.to_string_lossy().into_owned()))?;
        let prefix = prefix
            .to_str()
            .ok_or_else(|| FMLError::InvalidPath(manifest_file.to_string_lossy().into_owned()))?;
        let temp = TempDir::new(prefix)?;
        let build_dir = temp.path();
        compile_manifest_swift(manifest_file, build_dir)?;
        run_script(build_dir, script)
    }

    #[test]
    fn smoke_test_script() -> Result<()> {
        run_script_with_generated_code("fixtures/ios/FeatureManifest.swift".as_ref(), "fixtures/ios/script.swift".as_ref())
    }    
}
