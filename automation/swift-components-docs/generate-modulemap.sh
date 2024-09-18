#!/usr/bin/env bash

# Define the module name and path to the include directory
MODULE_NAME="MozillaRustComponents"
INCLUDE_DIR="Sources/SwiftComponents/include"
MODULEMAP_FILE="$INCLUDE_DIR/module.modulemap"

# Start creating the module map
echo "module $MODULE_NAME {" > "$MODULEMAP_FILE"

# Find all .h files in the include directory and add them to the module map
for header in "$INCLUDE_DIR"/*.h; do
    echo "    header \"$(basename "$header")\"" >> "$MODULEMAP_FILE"
done

# Add export statement to the end of the module map
echo "    export *" >> "$MODULEMAP_FILE"
echo "}" >> "$MODULEMAP_FILE"

# Confirm module.modulemap was created
echo "module.modulemap generated at: $MODULEMAP_FILE"
