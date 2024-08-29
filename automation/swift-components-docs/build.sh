#!/bin/bash

# Set the script to exit if any command fails
set -e

# Run the generate-swift-project.sh script
echo "Running generate-swift-project.sh..."
./generate-swift-project.sh

# Run the build-static-website.sh script
echo "Running generate-static-docs-website.sh..."
./generate-static-docs-website.sh

echo "All scripts executed successfully."

