# Offline application packages

A `.XAP` file carries one Xinyi application bundle: its manifest, executable, asset files and empty directories. It is an uncompressed archive; applications still install into `/APPS/ID`. Networking is not required or enabled.

Build a package from an ID-named folder:

```powershell
python tools/package_app.py target/my-app/HELLO --output target/HELLO.XAP
```

The source folder must contain `HELLO.APP` and exactly one of `HELLO.BIN` or `HELLO.ELF`. Asset names use uppercase-compatible DOS 8.3 names. The builder refuses links, junctions, case collisions and existing output files. Its manifest checks are preliminary; Applications and the kernel perform full metadata/image validation.

Copy the package onto the Xinyi drive. In **Applications**, choose **Install** (or **Update**), enter its absolute path, such as `/DOWNLOAD/HELLO.XAP`, and press Enter to review. Confirm after checking its title, version and access. Escape or Cancel leaves the drive untouched. Bundle-folder installation remains supported.

Review reads the entire package into a bounded private buffer and verifies its checksum and structure. Confirmation uses that reviewed snapshot, so subsequent edits to the source file do not change the installed bytes. An update must increase the version without expanding access; the kernel also refuses a running app. Restore and Discard backup use the existing single retained update backup for the drive.

## Limits and recovery

Version 1 allows at most **768 KiB including headers**, **63 entries**, and **six relative path components** per entry. Each parent directory must appear before its children. Executables retain the kernel's flat/ELF validation rules. The initial package cap fits within the manager's 1 MiB private heap; larger folder bundles can still use the existing folder installer within its limits.

Extraction reserves a new `/PKGIN` folder and refuses an existing one. It extracts into `/PKGIN/ID`, invokes the existing staged kernel installer, then removes its temporary files. Normal installation errors also trigger cleanup. The source `.XAP` is preserved. Failure to clean up is reported separately, because the app may already have installed successfully.

If Applications is terminated or power is lost during extraction, inspect `/PKGIN` in Files before removing the leftovers and retrying. Existing `/PKGIN` contents are never automatically replaced or deleted. Kernel publication retains its existing `/APPS/_STAGE`, `_BACKUP` and `_UPD.ID` recovery rules; this is not a power-loss-atomic transaction.

The CRC detects accidental corruption. It does **not** authenticate a publisher or establish that an application is safe. Packages grant no new privileges; executable validation, launch profiles and the existing installer remain authoritative. Further isolation review is still on the roadmap.

## Binary format: XAP v1

All integers are unsigned little-endian. There is no compression, padding between entries, or trailing data.

| Header offset | Bytes | Meaning |
| --- | --- | --- |
| 0 | 8 | ASCII `XINYIPK1` |
| 8 | 8 | Uppercase app ID, zero padded; first letter A-Z, remaining A-Z/0-9/underscore |
| 16 | 2 | Entry count (2 through 63) |
| 18 | 2 | Reserved, zero |
| 20 | 4 | IEEE CRC-32 of bytes 0..19 followed by bytes 24..EOF |

Each entry has an eight-byte header: path length (u8), kind (u8: 0 file, 1 directory), reserved (u16, zero), and data length (u32). Then come the path bytes and data bytes. Directories have zero data length. Paths have 1..77 ASCII bytes, slash separators and canonical uppercase 8.3 components using A-Z, digits, underscore and hyphen. Absolute paths, empty components, dot components, backslashes, duplicate paths and file parents are rejected.

The root manifest must be `ID.APP`; exactly one nonempty root `ID.BIN` or `ID.ELF` must match its `exec` field. The parser is shared with `tools/app_package_tests.rs`; Python packing and extraction are exercised through the manager QEMU test.
