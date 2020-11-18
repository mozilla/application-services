## Miscellaneous Tooling Bits for Application Services

This directory contains miscellaneous tooling scripts that developers may need
to run locally while working on application-services. You'll need to read the
individual files to find out what they're for.

Some of the tools here are written in Python, and to run them you'll need
to install the Python dependencies listed in `requirements.txt`, like this:

```
pip3 install --require-hashes -r ./tools/requirements.txt
```

These dependencies are pinned to a specific hash for security.
To update the versions of a dependency you will need to:

* Visit https://pypi.org/ and search for the target package.
* Download the `.tar.gz` release bundle corresponding to the new version.
* (In theory you'd audit the downloaded package to check that it's trustworthy,
  but realisitically we don't have good advice on how to do that effectively).
* Run `pip hash ./path/to/package.tar.gz` to generate the hash string.
* Update `requirements.txt` with the new version number and hash string.
