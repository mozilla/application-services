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
    config: Option<Config>,
    cmd: GenerateStructCmd,
) -> Result<()> {
    let config = config.unwrap_or_default();
    let kt = gen_structs::FeatureManifestDeclaration::new(config, &manifest);

    let path = cmd.output;
    let contents = kt.render()?;

    std::fs::write(path, contents)?;

    Ok(())
}

#[cfg(test)]
pub mod test {
    use crate::util::{join, pkg_dir, sdk_dir};
    use anyhow::{bail, Result};
    use std::path::PathBuf;
    use std::process::Command;

    // The root of the Android kotlin package structure
    fn sdk_android_dir() -> String {
        join(sdk_dir(), "android/src/main/java")
    }

    // The directory with the mock implementations of Android
    // used for testing.
    fn runtime_dir() -> String {
        join(pkg_dir(), "fixtures/android/runtime")
    }

    // We'll put our test scripts in here.
    fn tests_dir() -> String {
        join(pkg_dir(), "fixtures/android/tests")
    }

    // The jar archive we need to do JSON with in Kotlin/Java.
    // This is the same library as bundled in Android.
    fn json_jar() -> String {
        join(runtime_dir(), "json.jar")
    }

    // The file with the kt implementation of FeatureVariables
    fn variables_kt() -> String {
        join(
            sdk_android_dir(),
            "org/mozilla/experiments/nimbus/FeatureVariables.kt",
        )
    }

    fn nimbus_internals_kt() -> String {
        join(sdk_android_dir(), "org/mozilla/experiments/nimbus/internal")
    }

    // The file with the kt implementation of FeatureVariables
    fn features_kt() -> String {
        join(
            sdk_android_dir(),
            "org/mozilla/experiments/nimbus/FeaturesInterface.kt",
        )
    }

    fn build_dir() -> String {
        use crate::util;
        join(util::build_dir(), "android")
    }

    fn manifest_build_dir() -> String {
        join(build_dir(), "manifests")
    }

    fn classpath() -> Result<String> {
        let cp = [
            json_jar(),
            prepare_runtime_build_dir()?,
            manifest_build_dir(),
        ];
        Ok(cp.join(":"))
    }

    // Prepare the build directory, by compiling the runtime, and FeatureVariables.kt
    // If the directory already exists, then do nothing more; it takes time
    // to spin up the kotlinc.
    // If you need to change the runtime files, then you'll need to remove the build file.
    fn prepare_runtime_build_dir() -> Result<String> {
        let dir_str = join(build_dir(), "runtime");
        let build_dir: PathBuf = PathBuf::from(&dir_str);
        if build_dir.is_dir() {
            return Ok(dir_str);
        }

        let status = Command::new("kotlinc")
            // Our generated bindings should not produce any warnings; fail tests if they do.
            .arg("-Werror")
            // Reflect $CLASSPATH from the environment, to help find `json.jar`.
            .arg("-classpath")
            .arg(json_jar())
            .arg("-d")
            .arg(&dir_str)
            .arg(&variables_kt())
            .arg(&features_kt())
            .arg(&runtime_dir())
            .arg(&nimbus_internals_kt())
            .spawn()?
            .wait()?;
        if status.success() {
            Ok(dir_str)
        } else {
            bail!("running `kotlinc` failed preparing a build directory",)
        }
    }

    // Compile a genertaed manifest file against the mocked out Android runtime.
    pub fn compile_manifest_kt(path: String) -> Result<()> {
        let build_dir = manifest_build_dir();

        // We need this to exist so we don't create a compile time warning for the first test.
        std::fs::create_dir_all(&build_dir)?;

        let status = Command::new("kotlinc")
            // Our generated bindings should not produce any warnings; fail tests if they do.
            .arg("-Werror")
            .arg("-J-ea")
            // Reflect $CLASSPATH from the environment, to help find `json.jar`.
            .arg("-classpath")
            .arg(classpath()?)
            .arg("-d")
            .arg(&build_dir)
            .arg(&path)
            .spawn()?
            .wait()?;
        if status.success() {
            Ok(())
        } else {
            bail!("running `kotlinc` failed compiling a generated manifest")
        }
    }

    // Given a generated manifest, run a kts script against it.
    pub fn run_script_with_generated_code(manifest_kt: String, script: &str) -> Result<()> {
        compile_manifest_kt(manifest_kt)?;
        let build_dir = prepare_runtime_build_dir()?;
        let status = Command::new("kotlinc")
            // Our generated bindings should not produce any warnings; fail tests if they do.
            .arg("-Werror")
            .arg("-J-ea")
            // Reflect $CLASSPATH from the environment, to help find `json.jar`.
            .arg("-classpath")
            .arg(&classpath()?)
            .arg("-d")
            .arg(&build_dir)
            .arg("-script")
            .arg(&script)
            .spawn()?
            .wait()?;
        if status.success() {
            Ok(())
        } else {
            bail!("running `kotlinc` failed running a script")
        }
    }

    #[test]
    fn smoke_test_runtime_dir() -> Result<()> {
        run_script_with_generated_code(
            join(tests_dir(), "SmokeTestFeature.kt"),
            "fixtures/android/tests/smoke_test.kts",
        )?;
        Ok(())
    }
}
