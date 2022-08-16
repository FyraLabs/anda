#!/bin/bash
# anda_build_rpm.sh
# Script to process rpmbuild and build RPMs inside a docker container for Andaman.

# m4_ignore(
echo "This is just a script template, not the script (yet) - pass it to 'argbash' to fix this." >&2
exit 11  #)Created by argbash-init v2.10.0


# ARG_POSITIONAL_SINGLE([mode], [Mode to run the script in. Valid values are 'cargo' and 'rpmbuild'])
# ARG_OPTIONAL_SINGLE([project],p,[Project to build. if using rpmbuild mode, must be path to spec file, otherwise it is an optional cargo workspace member])
# ARG_HELP([RPM builder scripts for Andaman. Not meant to be be run directly.])

# ARGBASH_GO

# [ <-- needed because of Argbash

# get the arguments from argbash
mode=$_arg_mode
project=$_arg_project

# vvv  PLACE YOUR CODE HERE  vvv

echo "Running in $mode mode"


# PUT YOUR CODE HERE

rpmdeps () {
    if [ -z "$project" ]; then
        >&2 echo "No project specified! Exiting."
        exit 1
    fi
    OUT=$(rpmbuild -br "$project" -D "_sourcedir $PWD" -D "_disable_source_fetch 0" -D "_srcrpmdir $PWD/anda-build/rpm/src" --pipe cat | grep -n "Wrote: ")

    #OUT=$(grep -hnr '^Wrote: ' <<<"$OUT" | sed 's/^Wrote: //')
    #OUT=$(echo "$OUT" | grep -v ".src.rpm")
    # Find a line that contains "Wrote: "
    # split by space and get the second field
    OUT=${OUT##* }
    echo "$OUT"
}



anda_rpmbuild () {

    SRPM=$(rpmdeps)

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

}


cargo_generate_rpm () {
    # Generate RPMs using cargo-generate-rpm
    if [ -z "$project" ]; then
        cargo build --release
        cargo generate-rpm
    else
        cargo build --release --package "$project"
        cargo generate-rpm -p "$project"

    fi
}


# match the mode
case $mode in
    cargo)
        cargo_generate_rpm
        ;;
    rpmbuild)
        anda_rpmbuild
        ;;
    *)
        echo "Invalid mode: $mode"
        exit 1
        ;;
esac


# ] <-- needed because of Argbash


