#!/bin/bash

# Run jazzy with the --clean option
jazzy --clean

# Check if jazzy command was successful
if jazzy --clean; then
    echo "Documentation generated successfully and cleaned up previous output."
else
    echo "Failed to generate documentation."
fi