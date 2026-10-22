# Keep release retries within one producer attempt

Lane: Spectrum launch-hardening / release plumbing. The current runtime fix is finished; this change only improves release recovery diagnostics.

1. Verify publication run39749624981 attempts1–3 without changing the run/tag. Attempt2 retains four successful attempt1 producers and correctly rejects their receipts under verifier attempt2. Attempt3 plan uploads succeed with overwrite=false; do not add a speculative artifact-conflict fix.
2. Keep exact run/attempt checks. Add actionable full-workflow retry guidance and tests proving retained/future/local wrong attempts remain rejected without mutating receipts. No mixed-attempt acceptance, API/schema/dependency change or metadata shortcut.
3. Document the full-run recovery command and its limits in the existing producer-source-lock decision. Run the source-lock rejection suite and Ruff; push one small PR. Remove the isolated workspace after verified remote retention.
