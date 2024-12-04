# Naming Conventions

All names in this project should adhere to the guidelines outlined in this document.

## Rust Code

TL;DR: do what Rust's builtin warnings and clippy lints tell you
(and CI will fail if there are any unresolved warnings or clippy lints).

### Overview

- All variable names, function names, module names, and macros in Rust code should follow typical `snake_case` conventions.

- All Rust types, traits, structs, and enum variants must follow `UpperCamelCase`.

- Static and constant variables should be written in `SCREAMING_SNAKE_CASE`. s

For more in-depth Rust conventions, see the [Rust Style Guide](https://doc.rust-lang.org/1.0.0/style/style/naming/README.html).

### Examples:
```rust
fn sync15_passwords_get_all()
struct PushConfiguration{...}
const COMMON_SQL
```

## Swift Code

### Overview

- Names of types and protocols are `UpperCamelCase`.

- All other uses are `lowerCamelCase`.

For more in-depth Swift conventions, check out the [Swift API Design Guidelines](https://swift.org/documentation/api-design-guidelines/).

### Examples:
```swift
enum CheckChildren{...}
func checkTree()
public var syncKey: String
```

## Kotlin Code

If a source file contains only a top-level class, the source file should reflect the case-sensitive name of the class plus the *.kt* extension. Otherwise, if the source contains multiple top-level declarations, choose a name that describes the contents of the file, apply `UpperCamelCase` and append `.kt` extension.

### Overview

- Names of packages are always lower case and do not include underscores. Using multi-word names should be avoided. However, if used, they should be concatenated or use `lowerCamelCase`.

- Names of classes and objects use `UpperCamelCase`.

- Names of functions, properties, and local variables use `lowerCamelCase`.

For more in-depth Kotlin Conventions, see the [Kotlin Style Guide](https://kotlinlang.org/docs/reference/coding-conventions.html#naming-rules).

### Examples:

```kotlin
//FooBar.kt
class FooBar{...}
fun fromJSONString()
package mozilla.appservices.places
```
