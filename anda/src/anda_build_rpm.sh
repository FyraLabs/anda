#!/bin/bash
# anda_build_rpm.sh
# Script to process rpmbuild and build RPMs inside a docker container for Andaman.

set -x

echo "RPMSPEC: ${RPMSPEC}"

rpmdeps () {
    OUT=$(rpmbuild -br "$RPMSPEC" -D "_sourcedir $PWD" -D "_disable_source_fetch 0" -D "_srcrpmdir $PWD/anda-build/rpm/src" --pipe cat)

    #OUT=$(grep -hnr '^Wrote: ' <<<"$OUT" | sed 's/^Wrote: //')
    #OUT=$(echo "$OUT" | grep -v ".src.rpm")
    # Find a line that contains "Wrote: "
    OUT=$(echo "$OUT" | grep -n "Wrote: ")
    # split by space and get the second field
    OUT=${OUT##* }
    echo "$OUT"
}


SRPM=$(rpmdeps)

echo "OUT: $SRPM"

while [[ "$SRPM" == *".buildreqs."* ]]; do
    echo "SRPM: ${SRPM}"
    sudo dnf builddep -y "$SRPM"
    echo "SRPM contains .buildreqs. running again until no .buildreqs."
    SRPM=$(rpmdeps)
done

rpmbuild \
 --rebuild ${SRPM} \
 -ba \
 -D "_rpmdir $PWD/anda-build/rpm/" \
 -D "_sourcedir $PWD" \
 -D "_srcrpmdir $PWD/anda-build/rpm/src" \
 -D "_disable_source_fetch 0"


