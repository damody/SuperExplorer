## 1. Formatting implementation

- [x] 1.1 Change the built-in file-size formatter to emit exactly one decimal for every nonzero KB/MB/GB/TB value.
- [x] 1.2 Update formatter and Details Size tests for zero, sub-KB, exact units, fractional units, 250.5 GB, and TB.

## 2. Verification

- [x] 2.1 Run focused explorer-ui tests, formatting checks, diff checks, and strict OpenSpec validation.
- [x] 2.2 Build `build_test_install.bat --no-launch` and record the installer CRC and hashes.
- [x] 2.3 Install the package and verify the installed executable matches the release executable.
- [x] 2.4 Capture D:\ Details-view evidence proving built-in Size and Folder size both retain one decimal.
