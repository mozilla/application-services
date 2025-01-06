# Init Rust Components

This component is used to initialize the other Rust components globally.

Some services, such as logging and cryptography, are used in several components
and require explicit initialization. This component was created for this
purpose.

Currently, the Init Rust Componentes component only handles the initialization
of NSS.

If you encounter the error message 'NSS not initialized' in your code, it is
probably because you have not called the initialization routine of this
component before calling a method that uses NSS.

Therefore, call the initialization method as early as possible in your code:
```
init_rust_components::initialize();
```

When using the `logins/keydb` feature to use NSS for keymanagement, provide a path to the initialization function:
```
init_rust_components::initialize(profile_path.to_string());
```
