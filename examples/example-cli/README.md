CLI for the example component.

Creating a CLI can be useful for a couple reasons:

* It can assist development by providing a way to execute the code.
* It provides an example of how the component will be consumed (this is why the CLIs live in the `examples` directory).

If you create a CLI, add an alias in `/.cargo/config.toml`.
For example, this CLI has an alias which allows it to be run using `cargo example [args]`
