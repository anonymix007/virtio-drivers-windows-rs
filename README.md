# Windows VirtIO drivers

This repository contains different VirtIO kernel drivers for Windows, currently only one is available with more drivers yet to come:
- VirtIO-GPU full (render + display) WDDM 2.0 driver

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

Contributions entirely generated or suggested by LLMs are discouraged. I don't have enough motivation to review LLM-generated PRs with a lot of hallucinated and straigh up incorrect comments across the code, so these will be most likely ignored.
