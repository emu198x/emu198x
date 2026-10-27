# Evidence machine identities

The site joins the registry to the generated usability table using the identifier
inside its Markdown code span. The renderer adds a cohabitation warning inside
that span, so six real machine IDs no longer join even though their rows exist.

1. Add a focused renderer regression in `scripts/status/test_render_status.py`
   proving that code-span IDs equal registry IDs, while cohabiting machines keep
   their warning. Show it fails before the change.
2. Move the warning outside the code span in `scripts/status/render_status.py`
   and regenerate its status documents. Run the regression and generator --check.
3. Use the unchanged site's reader against the generated result, then push a PR
   and require hosted checks before merging. Rerun the blocked site updates once
   the producer correction is on main.
