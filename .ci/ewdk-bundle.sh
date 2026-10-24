#!/bin/bash
set -xe

DIR=$(realpath $(dirname "$0")/..)

. $DIR/ewdk.env

EWDK_PRIVATE_REPO=/tmp/ewdk-private
CLANG_CL_LINUX_REPO=$DIR/../clang-cl-linux
KMDFVER=1.27

mkdir -p $EWDK_PRIVATE_REPO

rm -rf $EWDK_PRIVATE_REPO/Include
rm -rf $EWDK_PRIVATE_REPO/Lib

INC_SHARED="$WINSDKDIR/Include/$WINSDKVER/shared"
INC_KM_CRT="$WINSDKDIR/Include/$WINSDKVER/km/crt"
INC_KM="$WINSDKDIR/Include/$WINSDKVER/km"
INC_UM="$WINSDKDIR/Include/$WINSDKVER/um"
INC_WDF="$WINSDKDIR/Include/wdf/kmdf/$KMDFVER"
INC_MSVC="$VCTOOLSDIR/include"

mkdir -p $EWDK_PRIVATE_REPO/Include/$WINSDKVER/shared
mkdir -p $EWDK_PRIVATE_REPO/Include/$WINSDKVER/km/crt
mkdir -p $EWDK_PRIVATE_REPO/Include/$WINSDKVER/km/
mkdir -p $EWDK_PRIVATE_REPO/Include/$WINSDKVER/um/
mkdir -p $EWDK_PRIVATE_REPO/Include/wdf/kmdf/$KMDFVER

cp --no-preserve=mode -rv "$INC_SHARED"/*.h $EWDK_PRIVATE_REPO/Include/$WINSDKVER/shared/
cp --no-preserve=mode -rv "$INC_KM_CRT"/*.h $EWDK_PRIVATE_REPO/Include/$WINSDKVER/km/crt/
cp --no-preserve=mode -rv "$INC_KM"/*.h $EWDK_PRIVATE_REPO/Include/$WINSDKVER/km/
cp --no-preserve=mode -rv "$INC_WDF"/*.h $EWDK_PRIVATE_REPO/Include/wdf/kmdf/$KMDFVER/

UM_HEADERS=(d3d10TokenizedProgramFormat.hpp d3d10umddi.h d3d11TokenizedProgramFormat.hpp d3d12TokenizedProgramFormat.hpp d3dumddi.h dxgiddi.h dxmini.h)

for header in "${UM_HEADERS[@]}"; do
    cp -v "$INC_UM/$header" $EWDK_PRIVATE_REPO/Include/$WINSDKVER/um/
done

LIBS=(displib ntoskrnl ntstrsafe hal bufferoverflowfastfailk)
ARCHS=x64

LIB_KM="$WINSDKDIR/Lib/$WINSDKVER/km"

for arch in "${ARCHS[@]}"; do
    mkdir -p $EWDK_PRIVATE_REPO/Lib/$WINSDKVER/km/$arch

    for lib in "${LIBS[@]}"; do
        cp -rv "$LIB_KM/$arch/$lib.lib" $EWDK_PRIVATE_REPO/Lib/$WINSDKVER/km/$arch
    done
done

chmod -R +rw $EWDK_PRIVATE_REPO/*

python $CLANG_CL_LINUX_REPO/generate_vfs.py \
  --msvc $EWDK_PRIVATE_REPO/ \
  --sdk $EWDK_PRIVATE_REPO/ \
  --out $EWDK_PRIVATE_REPO/vfs-overlay.json

sed "s#$EWDK_PRIVATE_REPO#EWDK_PRIVATE_REPO#g" -i $EWDK_PRIVATE_REPO/vfs-overlay.json

echo 'export EWDK_ROOT="EWDK_PRIVATE_REPO"' > $EWDK_PRIVATE_REPO/ewdk.env
echo "export VCTOOLSVER=\"$VCTOOLSVER\"" >> $EWDK_PRIVATE_REPO/ewdk.env
echo 'export VCTOOLSDIR="$EWDK_ROOT"' >> $EWDK_PRIVATE_REPO/ewdk.env
echo "export WINSDKVER=\"$WINSDKVER\"" >> $EWDK_PRIVATE_REPO/ewdk.env
echo 'export WINSDKDIR="$EWDK_ROOT"' >> $EWDK_PRIVATE_REPO/ewdk.env
echo 'export VFSOVERLAY="/tmp/vfs-overlay.json"' >> $EWDK_PRIVATE_REPO/ewdk.env

rm -rf $DIR/../ewdk-private/Include
rm -rf $DIR/../ewdk-private/Lib
cp -r $EWDK_PRIVATE_REPO/* $DIR/../ewdk-private
rm -rf $EWDK_PRIVATE_REPO
