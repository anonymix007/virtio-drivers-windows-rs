#!/bin/bash
set -xe

ARCH=x86_64
FLAVOR=debug
NAME=virtio_wddm_rs
VERSION=0.1.0
HWID=''

TARGET=$ARCH-pc-windows-msvc

case $ARCH in
  x86_64)
    WINARCH=x64
    WINVER=_v100_X64_25H2
    ;;
  aarch64)
    WINARCH=arm64
    WINVER=_v100_ARM64_25H2
    ;;
  *)
    echo "Unsupported arch: $ARCH"
    exit 1
    ;;
esac

DIR=$(realpath $(dirname "$0"))

. $DIR/cert.env

DIST=$DIR/target/dist
DRIVER=$DIR/target/$TARGET/$FLAVOR/virtio_gpu_wddm_rs
MESA=$DIR/target/mesa

CARGO_FLAVOR=""
if [ "$FLAVOR" != "debug" ]; then
  CARGO_FLAVOR="--$FLAVOR"
fi

mkdir -p $MESA

cbindgen --config $DIR/gpu-wddm/cbindgen.toml $DIR/gpu-wddm/src/uapi.rs > $MESA/virtio_wddm_uapi.h 2>/dev/null
cbindgen --config $DIR/gpu-wddm/cbindgen.toml $DIR/gpu-wddm/src/uapi.rs --lang C++ > $MESA/virtio_wddm_uapi.hpp 2>/dev/null

if [ "$1" != "--no-force-rebuild" ]; then
  touch $DIR/gpu-wddm/build.rs
fi
cargo build $CARGO_FLAVOR --package virtio-gpu-wddm-rs

. $DIR/ewdk.env

BUILDDATE=$(TZ=UTC+0 date '+%m/%d/%Y')

rm -rf $DIST
mkdir -p $DIST
sed "s#%%BUILDDATE%%#${BUILDDATE}#" $DIR/$NAME.inx > $DIST/$NAME.inf
cp $MESA/{*.json,*.dll} $DIST/

for icd in $DIST/vulkan_*.dll; do
  symbols=${icd%.*}.debug
  $ARCH-w64-mingw32-objcopy --only-keep-debug $icd $symbols
  $ARCH-w64-mingw32-strip --strip-debug --strip-unneeded $icd
  $ARCH-w64-mingw32-objcopy --add-gnu-debuglink=$symbols $icd
done

osslsigncode sign -key $KEY -certs $CERT $DRIVER.dll $DIST/$NAME.sys
makecat --os-attr "2:10.0" --os "$WINVER" --hwid "$HWID" --output $DIST/$NAME.cat-unsigned $DIST/*
osslsigncode sign -key $KEY -certs $CERT $DIST/$NAME.cat-unsigned $DIST/$NAME.cat
rm $DIST/$NAME.cat-unsigned
cp $DRIVER.pdb $DIST/
cp $MESA/*.debug $DIST/
