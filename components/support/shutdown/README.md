Shutdown handling for application-services components

This crate allows us to enter shutdown mode causing any long-running operations
to be interrupted.  "Long-running" operations include both Rust code and
rusqlite queries.  Once we enter shutdown, any future long-running operations
will also be interrupted unless the `restart()` method is called.

In onder to support shutdown, components must do a few things:

  - Any potentially long-running function should regularly call
    `shutdown::err_if_shutdown()?` throughout the operation.  Usually this means
    checking at the start of the function and each loop.
  - The `Store` class that manages database connections should implement the
    `ShutdownInterrupt` trait and call `shutdown::register_interrupt` when
    it's created.
