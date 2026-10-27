# Reject media the MTX runtime cannot load

Engineering-frontier bug fix for #261. The runtime advertises no media slots,
but `load_media` ignores every image and returns success.

1. Add `crates/runtime-memotech-mtx/tests/media.rs`: both MTX models reject all
   current nonempty media classes while preserving their serialized live state;
   an empty set remains a successful no-op. Run against the baseline and verify
   the intended assertion fails, not setup or compilation.
2. Change only `load_media` in `src/runtime.rs` to return UnsupportedMediaKind
   for the first supplied image. Keep APIs, schemas and empty setup unchanged.
3. Run the runtime's tests, targeted Clippy, formatting and diff checks. Push a
   separate PR and require its actual CI gates before rebase merge. Do not close
   the larger file-loading request merely because the silent success is fixed.
