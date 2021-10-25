/* This Source Code Form is subject to the terms of the Mozilla Public
* License, v. 2.0. If a copy of the MPL was not distributed with this
* file, You can obtain one at http://mozilla.org/MPL/2.0/. */

use crate::{Config, GenerateStructCmd};

pub(crate) fn generate_struct(_config: Option<Config>, _cmd: GenerateStructCmd) {
    todo!("Kotlin backend achieved")
}

#[cfg(test)]
mod test {
    use crate::backends::kotlin;
    use crate::error::FMLError;
    use crate::error::Result;
    use crate::GenerateStructCmd;
    use std::path::PathBuf;
    use std::{env, process::Command};

    fn pkg_dir() -> String {
        env::var("CARGO_MANIFEST_DIR")
            .expect("Missing $CARGO_MANIFEST_DIR, cannot build tests for generated bindings")
    }

    fn join(base: String, suffix: &str) -> String {
        [base, suffix.to_string()]
            .iter()
            .collect::<PathBuf>()
            .to_string_lossy()
            .to_string()
    }

    // The Application Services directory
    fn as_dir() -> String {
        join(pkg_dir(), "../../..")
    }

    // The Nimbus SDK directory
    fn sdk_dir() -> String {
        join(as_dir(), "components/nimbus")
    }

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

    // We'll generate our Kotlin files in here.
    fn generated_dir() -> String {
        join(pkg_dir(), "fixtures/android/generated")
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

    fn classpath() -> Result<String> {
        Ok(format!("{}:{}", json_jar(), prepare_build_dir()?))
    }

    // Prepare the build directory, by compiling the runtime, and FeatureVariables.kt
    // If the directory already exists, then do nothing more; it takes time
    // to spin up the kotlinc.
    // If you need to change the runtime files, then you'll need to remove the build file.
    fn prepare_build_dir() -> Result<String> {
        let pkg_dir = pkg_dir();
        let file_path = "build/android/runtime";
        let build_dir: PathBuf = [&pkg_dir, file_path].iter().collect();
        let dir_str = build_dir.to_string_lossy().to_string();
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
            .arg(&runtime_dir())
            .spawn()?
            .wait()?;
        if status.success() {
            Ok(dir_str)
        } else {
            Err(FMLError::CLIError(
                "running `kotlinc` failed preparing a build directory".into(),
            ))
        }
    }

    // Compile a genertaed manifest file against the mocked out Android runtime.
    fn compile_manifest_kt(path: String) -> Result<()> {
        let build_dir = prepare_build_dir()?;
        let status = Command::new("kotlinc")
            // Our generated bindings should not produce any warnings; fail tests if they do.
            .arg("-Werror")
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
            Err(FMLError::CLIError(
                "running `kotlinc` failed compiling a generated manifest".into(),
            ))
        }
    }

    // Given a generated manifest, run a kts script against it.
    fn run_script_with_generated_code(manifest_kt: String, script: &str) -> Result<()> {
        compile_manifest_kt(manifest_kt)?;
        let script = join(tests_dir(), script);
        let build_dir = prepare_build_dir()?;
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
            Err(FMLError::CLIError(
                "running `kotlinc` failed running a script".into(),
            ))
        }
    }

    // Given a manifest.fml and script.kts in the tests directory generate
    // a manifest.kt and run the script against it.
    #[allow(dead_code)]
    fn generate_and_assert(script: &str, manifest: &str, is_ir: bool) -> Result<()> {
        let manifest_fml = join(tests_dir(), manifest);
        let manifest_kt = format!("{}.kt", join(generated_dir(), manifest));
        let cmd = GenerateStructCmd {
            manifest: manifest_fml.into(),
            output: manifest_kt.clone().into(),
            load_from_ir: is_ir,
            language: crate::TargetLanguage::Kotlin,
        };
        kotlin::generate_struct(None, cmd);

        run_script_with_generated_code(manifest_kt, script)?;
        Ok(())
    }

    #[test]
    fn smoke_test_runtime_dir() -> Result<()> {
        run_script_with_generated_code(join(tests_dir(), "SmokeTestFeature.kt"), "smoke_test.kts")?;
        Ok(())
    }
}
