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
Usage: nimbus-cli [OPTIONS] <COMMAND>

Commands:
  apply-file    Send a complete JSON file to the Nimbus SDK and apply it immediately
  capture-logs  Capture the logs into a file
  defaults      Print the defaults for the manifest
  enroll        Enroll into an experiment or a rollout
  features      Print the feature configuration involved in the branch of an experiment
  fetch         Fetch one or more named experiments and rollouts and put them in a file
  fetch-list    Fetch a list of experiments and put it in a file
  info          Displays information about an experiment
  list          List the experiments from a server
  log-state     Print the state of the Nimbus database to logs
  open          Open the app without changing the state of experiment enrollments
  start-server  Start a server
  reset-app     Reset the app back to its just installed state
  tail-logs     Follow the logs for the given app
  test-feature  Configure an application feature with one or more feature config files
  unenroll      Unenroll from all experiments and rollouts
  validate      Validate an experiment against a feature manifest
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

Usage: nimbus-cli --app <APP> --channel <CHANNEL> enroll [OPTIONS] --branch <BRANCH> <EXPERIMENT_SLUG> [ROLLOUTS]... [-- <PASSTHROUGH_ARGS>...]

Arguments:
  <EXPERIMENT_SLUG>
          The experiment slug, including the server and collection

  [ROLLOUTS]...
          Optional rollout slugs, including the server and collection

  [PASSTHROUGH_ARGS]...
          Optionally, add platform specific arguments to the adb or xcrun command.

          By default, arguments are added to the end of the command, likely to be passed directly to the app.

          Arguments before a special placeholder `{}` are passed to `adb am start` or `xcrun simctl launch` commands directly.

Options:
      --file <EXPERIMENTS_FILE>
          An optional file from which to get the experiment.

          By default, the file is fetched from the server.

      --use-rs
          Use remote settings to fetch the experiment recipe.

          By default, the file is fetched from the v6 api of experimenter.

      --patch <PATCH_FILE>
          An optional patch file, used to patch feature configurations

          This is of the format that comes from the `features --multi` or `defaults` commands.

  -b, --branch <BRANCH>
          The branch slug

      --preserve-targeting
          Preserves the original experiment targeting

      --preserve-bucketing
          Preserves the original experiment bucketing

      --deeplink <DEEPLINK>
          Optional deeplink. If present, launch with this link

      --reset-app
          Resets the app back to its initial state before launching

      --preserve-nimbus-db
          Keeps existing enrollments and experiments before enrolling.

          This is unlikely what you want to do.

      --no-validate
          Don't validate the feature config files before enrolling

      --manifest <MANIFEST_FILE>
          An optional manifest file

      --version <APP_VERSION>
          An optional version of the app. If present, constructs the `ref` from an app specific template. Due to inconsistencies in branching names, this isn't always reliable

      --ref <APP_VERSION>
          The branch/tag/commit for the version of the manifest to get from Github

          [default: main]

  -h, --help
          Print help (see a summary with '-h')
```

### List

```sh
List the experiments from a server

Usage: nimbus-cli --app <APP> --channel <CHANNEL> list [OPTIONS] [SERVER]

Arguments:
  [SERVER]
          A server slug e.g. preview, release, stage, stage/preview

          [default: ]

Options:
  -f, --file <FILE>
          An optional file

      --use-api
          Use the v6 API to fetch the experiment recipes.

          By default, the file is fetched from the Remote Settings.

          The API contains *all* launched experiments, past and present, so this is considerably slower and longer than Remote Settings.

  -h, --help
          Print help (see a summary with '-h')
```

### Test Feature

```sh
Configure an application feature with one or more feature config files.

One file per branch. The branch slugs will correspond to the file names.

By default, the files are validated against the manifest; this can be overridden with `--no-validate`.

Usage: nimbus-cli --app <APP> --channel <CHANNEL> test-feature [OPTIONS] <FEATURE_ID> [FILES]... [-- <PASSTHROUGH_ARGS>...]

Arguments:
  <FEATURE_ID>
          The identifier of the feature to configure

  [FILES]...
          One or more files containing a feature config for the feature

  [PASSTHROUGH_ARGS]...
          Optionally, add platform specific arguments to the adb or xcrun command.

          By default, arguments are added to the end of the command, likely to be passed directly to the app.

          Arguments before a special placeholder `{}` are passed to `adb am start` or `xcrun simctl launch` commands directly.

Options:
      --patch <PATCH_FILE>
          An optional patch file, used to patch feature configurations

          This is of the format that comes from the `features --multi` or `defaults` commands.

      --deeplink <DEEPLINK>
          Optional deeplink. If present, launch with this link

      --reset-app
          Resets the app back to its initial state before launching

      --no-validate
          Don't validate the feature config files before enrolling

      --manifest <MANIFEST_FILE>
          An optional manifest file

      --version <APP_VERSION>
          An optional version of the app. If present, constructs the `ref` from an app specific template. Due to inconsistencies in branching names, this isn't always reliable

      --ref <APP_VERSION>
          The branch/tag/commit for the version of the manifest to get from Github

          [default: main]

  -h, --help
          Print help (see a summary with '-h')
```

## Environment Variables

- `XCRUN_PATH` the path to `xcrun`. This is only useful with macOS.
- `ADB_PATH` the path to `adb`.
- `NIMBUS_URL` the URL to the RemoteSettings server; a default is supplied.
- `NIMBUS_URL_STAGE` the URL to the staging RemoteSettings server; a default is supplied.
- `NIMBUS_V6_URL` the host for the Experimenter, used as a basis for the calls to `/api/v6`; a default is supplied.
- `NIMBUS_V6_URL_STAGE` the host for the staging Experimenter, used as a basis for the calls to `/api/v6`; a default is supplied.
- `NIMBUS_MANIFEST_CACHE` the directory where remote Feature Manifests are cached. A temp directory is used as default.
- `NIMBUS_CLI_SERVER_HOST` the IP address the server is on; defaults to the local IP address derived from the network interface.
- `NIMBUS_CLI_SERVER_PORT` the port the server is on; defaults to 8080.
