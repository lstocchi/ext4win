# ext4win

`ext4win` is a Windows-native Rust tool for creating ext4 filesystem images
without requiring Linux, WSL, or `mke2fs` at image-creation time.

Its purpose is to make application files available to a Linux VM through a
real ext4 disk image instead of a shared-folder filesystem. The VM can mount
the image directly, retaining normal Linux filesystem semantics and avoiding
the performance overhead often associated with host/guest file sharing.

## Project goal

The intended end-to-end workflows are:

1. Extract a root filesystem from an OCI image and write its files into a
   valid ext4 image.
2. Copy or inject an application directory that lives on the Windows host
   into a valid ext4 image.
3. Attach that image to a Linux VM, mount it as ext4, and run the application
   from the VM at native filesystem speed.

For example, the eventual CLI should support a workflow conceptually like:

```text
ext4win from-oci ghcr.io/example/service:latest service.ext4
ext4win inject C:\work\my-app app-data.ext4 --destination /opt/my-app
```

The generated image can then be attached as a virtual disk and mounted by the
guest:

```sh
mount -t ext4 /dev/vdb /mnt/app
```

## Current status

The project currently creates an empty, valid ext4 image containing:

- a 4 KiB block size and 256-byte inodes;
- extents and directory-entry file types;
- sparse-superblock layout, including backup superblocks and group descriptor
  tables;
- initialized block and inode bitmaps, inode tables, the root directory, and
  `lost+found`.

The current prototype does **not yet** import OCI layers, traverse host
directories, or expose the planned `from-oci` / `inject` commands. It is the
filesystem formatter and metadata foundation those features will build on.

## Build and create an image

Install a current Rust toolchain, then run:

```powershell
cargo run -- output.img
```

This currently creates a 400 MiB ext4 image named `output.img` in the current
directory. An output path can be supplied as the first argument.

## Verify the image in Linux or WSL

Validation should always include a read-only filesystem check:

```sh
file -s output.img
blkid output.img
e2fsck -fn output.img
```

`blkid` should report `TYPE="ext4"`. `file` may use the legacy phrase “ext2
filesystem data” while also reporting extents; that description alone does
not mean the image is invalid. The authoritative check is `e2fsck -fn`, which
must complete without proposing repairs.

## Design constraints

- Image creation must run on Windows; Linux tooling is used only for optional
  verification.
- The filesystem layout is modeled on `e2fsprogs` and written directly to the
  image buffer using on-disk ext4 structures.
- The current implementation uses 32-bit block addresses and 32-byte group
  descriptors.
- Images are deliberately created without a journal at this stage. Journal
  creation, timestamps, UUID generation, permissions/ownership mapping, and
  robust error handling are planned before this is used for production data.

## Development

```powershell
cargo fmt --check
cargo test
```

When changing allocation or metadata code, regenerate an image and run
`e2fsck -fn` after every change. A clean Rust build is not enough to establish
on-disk ext4 correctness.
