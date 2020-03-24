# Naming Conventions

All source files and variable names should adhere to the guidelines outlined in this document.

## Rust Code

All variable names, function names, module names, and macros in Rust code should follow typical *snake_case* conventions.

As an additional rule, the functions of all component ffi/src/lib.rs files should also follow *snake_case*, but with an additional prefix based on the library of that function. 

Examples:

	sync15_passwords_get_all()
	places_query_autocomplete()
	fxa_handle_push_message()


## Swift Code

All code written in Swift should follow [Swift API Design Guidelines](https://swift.org/documentation/api-design-guidelines/). Names of types and protocols are *UpperCamelCase*, while all other uses are *lowerCamelCase*.  

The only exception to this rule for this project is to use Rust convention for FFI binding files (e.g. RustFxAFFI.h).

## Kotlin Code

If a source file contains only a top-level class, the source file should reflect the case-sensitive name of the class plus the *.kt* extension. Otherwise, if the source contains multiple top-level declarations, choose a name that describes the contents of the file, apply *PascalCase* and append *.kt* extension.

Examples:

	//FooBar.kt
	class FooBar{}

### Naming Rules

- Names of packages are always lower case and do not include underscores. Using multi-word names should be avoided. However, if used, they should be concatenated or use *lowerCamelCase*.

- Names of classes and objects use *UpperCamelCase*.

- Names of functions, properties, and local variables use *lowerCamelCase*.

For more in-depth Kotlin Conventions, see the [Kotlin Style Guide](https://kotlinlang.org/docs/reference/coding-conventions.html#naming-rules).

