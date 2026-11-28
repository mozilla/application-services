# Dashboard Generator

Use this tool to create Grafana dashboards for your team's components.

## Setup

Ensure you have a yardstick account by going to https://yardstick.mozilla.org/ and logging in using Mozilla SSO.
You should have “editor” access and can create, edit, and delete dashboards and alerts. If not, go
to https://mozilla-hub.atlassian.net/wiki/spaces/SRE/pages/886866077/Yardstick+Grafana+Service+User+Guide
for help.

## Configuration

Add a `src/teams/[name].rs` module for your team.
Feel free to copy and paste from the other modules to get started.

Edit `src/teams/mod.rs` and wire up your new team module.

## Running

Run `cargo generate-rust-dashboards [team-name] [output-directory]` and follow the instructions.
