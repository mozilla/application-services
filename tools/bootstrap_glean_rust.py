#!python3
# Python 3

import importlib
import subprocess
import sys
offline = sys.argv[1] == 'offline'
module_name = sys.argv[2]
expected_version = sys.argv[3]
try:
    module = importlib.import_module(module_name)
except ImportError:
    found_version = None
else:
    found_version = getattr(module, '__version__')
if found_version != expected_version:
    if not offline:
        subprocess.check_call([
            sys.executable,
            '-m',
            'pip',
            'install',
            '--upgrade',
            f'{module_name}=={expected_version}'
        ])
    else:
        print(f'Using Python environment at {sys.executable},')
        print(f'expected glean_parser version {expected_version}, found {found_version}.')
        sys.exit(1)
try:
    subprocess.check_call([
        sys.executable,
        '-m',
        module_name
    ] + sys.argv[4:])
except:
    # We don't need to show a traceback in this helper script.
    # Only the output of the subprocess is interesting.
    sys.exit(1)
