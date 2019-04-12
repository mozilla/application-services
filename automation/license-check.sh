#!/bin/bash
#
# Check declared licenses of cargo dependencies against list of acceptable licenses.
# It's not a foolproof test but it's a nice guard against human error.

IFS=$'\n'
UNACCEPTABLE=
for LINE in `cargo license -t | tail -n +2`; do
  NAME=`echo ${LINE} | cut -f 1`
  LICENSE=`echo ${LINE} | cut -f 5`
  if [ "x${LICENSE}" = "x" ]; then
    LICENSE="UNKNOWN"
  fi
  if grep "^${LICENSE}\$" >/dev/null < `dirname ${0}`/licenses.txt; then true; else
    if grep "^=${NAME}\$" > /dev/null < `dirname ${0}`/licenses.txt; then true; else
      echo "Unacceptable license: ${NAME} ${LICENSE}"
      UNACCEPTABLE=1
    fi
  fi
done
if [ "x${UNACCEPTABLE}" = "x1" ]; then
  exit 1
fi
