# Extraction readiness

1. Add `scripts/extraction-readiness.py`: read the extraction manifest and Cargo metadata; show local versions, inter-repository dependencies, registry observations, and accessible GitHub repositories. Keep unknown distinct from absent and do not infer verification or consumer switches.
2. Add `scripts/test_extraction_readiness.py`: exercise dependency mapping, bundled crates, HTTP absence versus errors, and unknown states; run the tests and a real read-only report.
3. Reconcile the layer-two tracker with the 2026-10-25 workspace decision and link its manifest, preserving existing checklist history and gates. Verify the issue update.
