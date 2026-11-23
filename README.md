# Windows VirtIO drivers

This repository contains different VirtIO kernel drivers for Windows, currently only one is available with more drivers yet to come:
- VirtIO-GPU full (render + display) WDDM 2.0 kernel-mode driver (corresponding [UMD](https://github.com/anonymix007/virtio-d3d11))

## Quick start
- Download [EWDK](https://learn.microsoft.com/en-us/legal/windows/hardware/enterprise-wdk-license-2026) and mount the ISO somewhere.
- Create an environment file (`ewdk.env`) pointing to the EWDK dirs and versions:
```
#!/bin/bash

export EWDK_ROOT="/path/to/ewdk"
export VCTOOLSVER="14.44.35207"
export VCTOOLSDIR="$EWDK_ROOT/Program Files/Microsoft Visual Studio/2022/BuildTools/VC/Tools/MSVC/$VCTOOLSVER"
export WINSDKVER="10.0.26100.0"
export WINSDKDIR="$EWDK_ROOT/Program Files/Windows Kits/10"
export VFSOVERLAY="$HOME/projects/virtio-win32/vfs-overlay.json"
```
vfs-overlay.json is generated using generate_vfs.py script from [https://github.com/tmp64/clang-cl-linux/](https://github.com/tmp64/clang-cl-linux/), see the how-to there.
- Build Mesa and copy dlls to target/mesa
- Generate private and public signing keys and create an environment (`cert.env`) file with the paths:
```
#!/bin/bash

export CERT="/path/to/cert/localhost-km.der"
export KEY="/path/to/cert/localhost-km.pfx"
```
- Run the build script to compile and sign the driver
```console
./dist.sh
```

## Installation
Driver package will be available in the target/dist folder. At the very least it will contain the following files:
```console
$ tree target/dist
target/dist
├── dx11um_virtio.dll
├── virtio_icd.x86_64.json
├── virtio_wddm_rs.cat
├── virtio_wddm_rs.inf
├── virtio_wddm_rs.sys
└── vulkan_virtio.dll
```
Copy them to the VM. If you have an older version of the driver installed, it's recommended to uninstall it first:
```cmd
C:\Users\User>pnputil /enum-drivers
Microsoft PnP Utility

...
Published Name:     oem1.inf
Original Name:      virtio_wddm_rs.inf
Provider Name:      anonymix007
Class Name:         Display
Class GUID:         {4d36e968-e325-11ce-bfc1-08002be10318}
Driver Version:     11/22/2026 20.15.1.1
Signer Name:        Virto test certificate
Attributes:         Legacy
WHCP Version:       Unknown
...
C:\Users\User>pnputil /delete-driver oem1.inf /uninstall /force
Microsoft PnP Utility

Ignoring /force when used with /uninstall to delete driver package.
Driver package uninstalled.
Driver package deleted successfully.
```
Then, install the new driver:
```
pnputil /add-driver \path\to\virtio_wddm_rs.inf /install
```

## VM setup

Mainline QEMU and virglrenderer builds are required. Sample QEMU command line:
```bash
#!/bin/bash

set -xe

# Only single GPU per VM is supported, choose the appropriate one here
# Can be skipped there is only one on the host
export DRM_RENDER_NODE=/dev/dri/renderD128
export VK_DRIVER_FILES=/usr/share/vulkan/icd.d/radeon_icd.json
export DRI_PRIME=1002:164e
export MESA_LOADER_DRIVER_OVERRIDE=radeonsi

EFI_CODE="./OVMF_CODE.fd"
EFI_VARS="./win11_VARS.fd"
export VIRGL_LOG_FILE=virgl.log
export VIRGL_LOG_LEVEL=debug
export VIRGL_GBM_LAYOUT_FORCE_ENABLE=true
export VIRGL_GBM_LAYOUT_ENABLE=true
# Enable Vulkan Validation Layers for Venus
#export VKR_DEBUG=validate

qemu-system-x86_64 \
    -trace 'virtio*' \
    -trace 'virgl*' \
    -d guest_errors \
    -drive if=pflash,format=raw,unit=0,file=$EFI_CODE,readonly=on \
    -drive if=pflash,format=raw,unit=1,file=$EFI_VARS \
    -enable-kvm \
    -cpu host \
    -usb \
    -machine q35 \
    -serial file:driver.log \
    -device "virtio-vga-gl,id=video3,hostmem=4G,blob=on,venus=on,xres=1920,yres=1080"
    -device usb-tablet
    -chardev socket,id=kd,path=/tmp/kernel-debug.sock,server=on,wait=off -serial chardev:kd \
    -device usb-kbd \
    -device e1000,netdev=nic0 \
    -netdev user,id=nic0,hostfwd=tcp::10022-:22 \
    -m 16G \
    -object memory-backend-memfd,id=mem1,size=16G,share=on  \
    -machine memory-backend=mem1 \
    -smp 8 \
    -display gtk,gl=on \
    -hda 25h2.qcow2 2>&1 | tee qemu.log
```
You may try different `-display` backends, but note that at least `-display dbus` is broken on Nvidia in upstream QEMU as of now. Eventually I'll submit a patch.

## Gallery

<img width="3026" height="1702" alt="demo" src="https://github.com/user-attachments/assets/8979a1b8-92c6-47d6-abdd-969d2a473a9a" />

## License

This program is free software: you can redistribute it and/or modify it under the terms of the GNU Affero General Public License as published by the Free Software Foundation, either version 3 of the License, or (at your option) any later version.
This program is distributed in the hope that it will be useful, but WITHOUT ANY WARRANTY; without even the implied warranty of MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE. See the GNU General Public License for more details.
You should have received a copy of the GNU General Public License along with this program. If not, see <https://www.gnu.org/licenses/>.

If your this license does not fit your project, contact me privately. Other licensing options are negotiable.

## Contributing

There's a lot of small or not-so-small things to implement for the project. Grep for TODOs in the code for some contribution ideas. Drivers for other devices are also a great way to improve the project. However, there needs to be a real use-case for the driver for me to consider merging it.

By contributing any modifications you agree to grant the project perpetual, world-wide, non-exclusive, no-charge, royalty-free, irrevocable license to reproduce, prepare derivative works of, publiclt display, publicly perform, sublicense and distribute your modifications or any derivative works.
The intent here is to allow me later relicensing the project under a different license if deemed necessary.

In general, try to keep commit messages or comments short and concise. Any PRs MUST be thoroughly tested to ensure that the changes actually do what they are supposed to.

## LLM Policy

Code contributions or issue reports generated by LLMs are prohibited.
Low-level Windows development is rather complicated due to lack of documentation. Windows has so many of very weird undocumented quirks (to the point that these should be considered bugs and fixed ages ago) that writing code is not the bottleneck of development. Good patches require good understanding of how code works, what Windows expects from it and so on. LLMs will not hesitate before rewriting the whole project thousands of times to fix a small non-issue. The goal of this project is not to get the code, but rather to get the knowledge required to write the code.

It's OK to use LLMs to analyze code, review test logs, do some bug triage and even generate simple test cases. But since LLMs tend to produce unreadable walls of text, please write everything you submit here by yourself, be it code or bug reports. Please also do your own reserach before blindly believing what your LLM of choice has said. Once again, documentation is scarce, so every claim has to be independently verified.
