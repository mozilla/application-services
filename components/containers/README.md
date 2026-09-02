# Containers

Storage for Firefox containers.

The component never touches the filesystem. `ContainersStore` hands the
serialized document to a callback, and the embedder decides where it goes and
how durably.

It owns the list, not the behaviour. Everything that gives a container its
meaning stays outside:

- the origin attributes that isolate its cookies and storage
- clearing that storage when a container is removed
- resolving the localized labels of the shipped containers
- closing the tabs that belong to one

## Tests

Tests are run with

```shell
cargo test -p containers
```

## Bugs

We use Bugzilla to track bugs and feature work. You can use [this link](https://bugzilla.mozilla.org/enter_bug.cgi?product=Firefox&component=Containers) to file bugs in the `Firefox :: Containers` bug component.
