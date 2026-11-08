# intlify_config_memory_bench

The binary that measures the peak live memory of `intlify_config`'s resolver construction: design 015's `profile_resolve_peak_memory` phase, `construction_peak_live_memory` cost.

It is a separate crate because the observer it counts with, `allocation-counter`, installs its own global allocator in every binary that links it. In its own crate, the allocator reaches only this binary: `cargo test -p intlify_config --all-features` never builds this crate, and neither the duration smoke nor a product binary links the allocator. The `intlify_config` library holds only the observer seam and names no allocator. This crate hands the library the observer and the description the observer gives of itself.

## Verification

On Linux or macOS, from the repository root:

```sh
vp run bench:config:smoke
```

That runs the duration smoke and then this crate's memory smoke. To run only the memory smoke:

```sh
cargo bench -p intlify_config_memory_bench --bench profile_resolver_memory -- --profile smoke --validate
```

The memory smoke observes the 40 construction cases: the inputs, and the exact and first-over edges of the construction bounds. It saves the measurement records to a new temporary directory, rereads them, and validates them against the run that produced them. It prints the output location and a presentation summary of case counts.

The peaks are observational. The observer's counters are not checked for overflow, and while a buffer grows it counts the old and the new buffer together. The run records these limitations in its measurement method, and it makes no numeric decision.

## License

MIT
