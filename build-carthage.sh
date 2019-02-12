if [ $# -eq 0 ]; then
    echo "Missing arg: name of output zip file"
    exit 1
fi

## When https://github.com/Carthage/Carthage/issues/2623 is fixed, 
## carthage build --archive should work to produce a zip 
carthage build --no-skip-current
zip -r $1 Carthage/Build/iOS

