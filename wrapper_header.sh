#!/bin/bash

# Generate the libsolv wrapper header

#use find to find all the files, then cut the path

WD=$(pwd)

pushd /usr/include
find solv -name "*.h" -exec echo "#include <{}>" \; > $WD/libsolv.h

popd