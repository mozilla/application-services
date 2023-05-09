# `nimbus-cli`

Mozilla Nimbus' command line tool for mobile apps.

Currently supporting testing on device or emulator for Android, and on simulator only for iOS.

This provides a unified interface for QA and developers to enroll and unenroll into experiments,
including first run experiments.

## Supported apps

The apps currently supported are:

- Firefox for Android (`fenix`)
- Firefox for iOS (`firefox_ios`)
- Focus for Android (`focus_android`)
- Focus for iOS (`focus_ios`).

## Usage

```sh
Usage: nimbus-cli [OPTIONS] --app <APP> --channel <CHANNEL> <COMMAND>

Commands:
  apply-file    Send a complete JSON file to the Nimbus SDK and apply it immediately
  capture-logs  Capture the logs into a file
  enroll        Enroll into an experiment or a rollout
  fetch         Fetch one or more experiments and put it in a file
  list          List the experiments from a server
  log-state     Print the state of the Nimbus database to logs
  reset-app     Reset the app back to its just installed state
  tail-logs     Follow the logs for the given app
  test-feature  Configure an application feature with one or more feature config files
  unenroll      Unenroll from all experiments and rollouts
  help          Print this message or the help of the given subcommand(s)

Options:
  -a, --app <APP>              The app name according to Nimbus
  -c, --channel <CHANNEL>      The channel according to Nimbus. This determines which app to talk to
  -d, --device-id <DEVICE_ID>  The device id of the simulator, emulator or device
  -h, --help                   Print help (see more with '--help')
```

### Enroll

```sh
Enroll into an experiment or a rollout.

The experiment slug is a combination of the actual slug, and the server it came from.

* `release`/`stage` determines the server.

* `preview` selects the preview collection.

These can be further combined: e.g. $slug, preview/$slug, stage/$slug, stage/preview/$slug

Usage: nimbus-cli --app <APP> --channel <CHANNEL> enroll [OPTIONS] --branch <BRANCH> <SLUG> [ROLLOUTS]...

Arguments:
  <SLUG>
          The experiment slug, including the server and collection

  [ROLLOUTS]...
          Optional rollout slugs, including the server and collection

Options:
  -b, --branch <BRANCH>
          The branch slug

      --preserve-targeting
          Preserves the original experiment targeting

      --preserve-bucketing
          Preserves the original experiment bucketing

      --reset-app
          Resets the app back to its initial state before launching

      --preserve-nimbus-db
          Keeps existing enrollments and experiments before enrolling.

          This is unlikely what you want to do.

  -f, --file <FILE>
          Instead of fetching from the server, use a file instead

  -h, --help
          Print help (see a summary with '-h')
```

### List

```sh
List the experiments from a server

Usage: nimbus-cli --app <APP> --channel <CHANNEL> list [OPTIONS] [SERVER]

Arguments:
  [SERVER]  A server slug e.g. preview, release, stage, stage/preview

Options:
  -f, --file <FILE>  An optional file
  -h, --help         Print help
```

### Test Feature

```sh
Configure an application feature with one or more feature config files.

One file per branch. The branch slugs will correspond to the file names.

Usage: nimbus-cli --app <APP> --channel <CHANNEL> test-feature [OPTIONS] <FEATURE_ID> [FILES]...

Arguments:
  <FEATURE_ID>
          The identifier of the feature to configure

  [FILES]...
          One or more files containing a feature config for the feature

Options:
      --reset-app
          Resets the app back to its initial state before launching

  -h, --help
          Print help (see a summary with '-h')
```

## Environment Variables

- `XCRUN_PATH` the path to `xcrun`. This is only useful with macOS.
- `ADB_PATH` the path to `adb`.
- `NIMBUS_URL` the URL to the RemoteSettings server; a default is supplied.
- `NIMBUS_URL_STAGE` the URL to the staging RemoteSettings server; a default is supplied.
