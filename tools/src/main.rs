use dialoguer::{theme::ColorfulTheme, Select};
use std::process;
use std::process::Command;

fn main() {
    let cmds = [
        ("Build Application Services", "cargo build"),
        (
            "Run all tests for a pull request",
            "sh ./automation/all_tests.sh",
        ),
        ("Run all Rust tests", "sh ./automation/all_rust_tests.sh"),
        (
            "Verify Desktop Environment",
            "sh ./libs/verify-desktop-environment.sh",
        ),
        (
            "Verify Android Environment",
            "sh ./libs/verify-android-environment.sh",
        ),
        (
            "Verify iOS Environment",
            "sh ./libs/verify-ios-environment.sh",
        ),
        (
            "Smoke test Android Components",
            "python3 ./automation/smoke-test-android-components.py",
        ),
        (
            "Smoke test Fenix Components",
            "python3 ./automation/smoke-test-fenix.py",
        ),
        (
            "Smoke test Firefox iOS Components",
            "python3 ./automation/smoke-test-firefox-ios.py",
        ),
        (
            "Tag a new release minor release",
            "python3 ./automation/prepare-release.py minor",
        ),
        (
            "Tag a new release patch release",
            "python3 ./automation/prepare-release.py patch",
        ),
        (
            "Lint bash script changes",
            "sh ./automation/lint_bash_scripts.sh",
        ),
        (
            "Create a 'cargo update' PR",
            "python3 ./automation/cargo-update-pr.py",
        ),
    ];

    let selections = cmds.iter().map(|(title, _cmd)| title).collect::<Vec<_>>();

    let selection = Select::with_theme(&ColorfulTheme::default())
        .with_prompt("What would you like to do today?")
        .default(0)
        .items(&selections[..])
        .interact()
        .unwrap();

    spawn(cmds[selection].1);
}

fn spawn(cmd: &str) {
    println!("Executing: {}", cmd);
    let mut split = cmd.split_whitespace();

    Command::new(split.next().unwrap())
        .args(split)
        .stdout(process::Stdio::piped())
        .stderr(process::Stdio::inherit())
        .spawn()
        .unwrap()
        .wait_with_output()
        .unwrap();
}
