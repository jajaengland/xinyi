# First public release

Publish the first build as a GitHub prerelease with separate Installer and Live
downloads. Both should boot directly into their respective graphical interface.

| Image | Boot experience | Readiness |
| --- | --- | --- |
| Installer ISO and raw USB image | Open the graphical installer. Choose a destination, review the disk changes and capacity limits, and reboot into the installed desktop | Graphical wizard reuses the IDE and AHCI installer. The installer USB boots directly into setup. ISO packaging remains pending |
| Live ISO and raw USB image | Open the desktop with bundled apps. Session changes stay in RAM and disappear on reboot | The Live USB build uses a RAM-backed session and blocks physical disk writes. ISO packaging remains pending |

Build both images from the same source revision and app bundle. The Live kernel
enables RAM-backed storage at build time. The Installer image stays in the wizard until the user
reboots or shuts down. The Live image opens the regular desktop and leaves the
boot media and internal disks untouched during normal use. Use distinct names
for the ISO and raw USB builds so each download identifies its purpose and media
type. The existing QEMU ZIP can remain an optional preinstalled desktop download.

The installer should reuse the existing disk plan and confirmation checks. Show
the destination model and capacity, explain the changes to its partitions and
data, and require an explicit final confirmation. Show progress during the copy
and offer to reboot when installation succeeds. The installation source must
be protected from selection as a destination.

Test each ISO as a virtual CD with no hard disk attached. The installer should
show that no destination is available, and the live desktop should open bundled
apps. Verify that a Live session can save a file in RAM, loses that file on
reboot, and leaves the boot image and any attached internal disk byte-for-byte
unchanged. For installation, use disposable IDE and AHCI targets and check that
cancellation leaves the target unchanged. A completed installation must cold
boot with bundled apps after removing the installation media. Repeat the boot
checks with the raw USB builds, and complete an installation on a dedicated
physical SATA disk before advertising physical SATA compatibility.

Build public artifacts from the same clean tagged revision and attach SHA-256
checksums. Include boot requirements, known hardware limitations, and a short
list of machines tested. Keep personal hardware reports and firmware dumps
out of the release assets.
