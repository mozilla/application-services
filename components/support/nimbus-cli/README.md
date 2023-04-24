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
  enroll     Enroll into an experiment or a rollout
  list       List the experiments from a server
  reset-app  Reset the app back to its just installed state
  unenroll   Unenroll from all experiments and rollouts
  help       Print this message or the help of the given subcommand(s)

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

Usage: nimbus-cli --app <APP> --channel <CHANNEL> enroll [OPTIONS] --branch <BRANCH> <SLUG>

Arguments:
  <SLUG>
          The experiment slug, including the server and collection

Options:
  -b, --branch <BRANCH>
          The branch slug

      --preserve-targeting
          Preserves the original experiment targeting

      --preserve-bucketing
          Preserves the original experiment bucketing

      --reset-app
          Resets the app back to its initial state before launching

  -h, --help
          Print help (see a summary with '-h')
```

### List

```sh
List the experiments from a server

Usage: nimbus-cli --app <APP> --channel <CHANNEL> list [SERVER]

Arguments:
  [SERVER]  A server slug e.g. preview, release, stage, stage/preview

Options:
  -h, --help  Print help
```

## Environment Variables

- `XCRUN_PATH` the path to `xcrun`. This is only useful with macOS.
- `ADB_PATH` the path to `adb`.
- `NIMBUS_URL` the URL to the RemoteSettings server; a default is supplied.
- `NIMBUS_URL_STAGE` the URL to the staging RemoteSettings server; a default is supplied.
