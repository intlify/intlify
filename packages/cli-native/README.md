# @intlify/cli-native

This package is the source package for the native `intlify` CLI binary consumed by `@intlify/cli`.

Users normally install and invoke `@intlify/cli` rather than this package directly. Release automation builds the native binary for each supported target and assembles the binaries under `bin/<rust-target>/` from this single source package.
