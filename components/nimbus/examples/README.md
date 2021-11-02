# Experiments CLI
In this directory we have a command line interface that helps us interact with the Nimbus SDK.


## How to use
In order to use the CLI, you would run it as an example:
```bash
cargo run --example experiment -- -c ./examples/config/config.json show-experiments
```
This would display all the valid experiments retrieved from the server

You can check out the details by running
```bash
cargo run --example experiment -- -h
```

You can set a config file using the `-c` option, which can include the following:

```text
{
    "context": {..},// App context elements
    "server_url": "...", // A remote settings url
    "bucket_name": "..." // Name of the bucket in the remote server (Defaults to `main`)
    "collection_name": "...", // Name of a collection in the remote server (Defaults to `messaging-experiment`)
    "uuid": ".." // A custom uuid to use
}
```

If you would like to generate a UUID for testing purposes, you can use the `gen-uuid` subcommand. This takes a number argument, and will attempt to generate a `uuid` that is able to enroll that the given number of experiments.

Note on the `gen-uuid` subcommand, the higher the number the longer it will take. It also depends on the bucket configuration of the buckets retrieved from the server.
