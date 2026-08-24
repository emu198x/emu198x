---
title: Citations to the reference library are provenance records, not links
date: 2026-08-24
status: binding
scope: documentation / provenance
---

# Citations to the reference library are provenance records, not links

## Decision

Paths written as `reference/by-system/...` in this repository name documents in
the project's **primary source library**. That library is **private and stays
private**, so these citations are **records of where a fact came from — never
navigable links**.

Write them as plain backticked paths. **Do not turn them into markdown links.**

## Why

The library holds third-party material the project may keep for reference but
may not redistribute: manufacturer documentation, book and magazine scans, and
mirrored community sites. Privacy is the condition that makes holding it
legitimate — see `decisions/how-198x-licenses-its-own-work.md` in the umbrella
repository.

Before 2026-08-24 these were written as relative markdown links
(`[...](../../../../reference/by-system/...)`). This repository is **public**, so
every one of them resolved to nothing for every reader who is not Steve. Some
were written as absolute paths and leaked a local home directory into public
documentation.

Neither was a licence breach — a path is not the material it names — but both
made public documentation cite a tree its audience cannot reach, which is worse
than useless: it looks like a working reference.

## What this does not change

**Facts remain freely reusable.** Facts are not copyrightable, this repository is
openly licensed, and the citation exists so a claim stays traceable and a reader
can seek out the same source independently.

⚠ **Restricted-provenance sources are a stricter case.** Some documents may not
be named by filename, link, or archive identifier on any public surface at all —
title and date only. See `decisions/citing-restricted-provenance-sources.md` in
the umbrella repository. Audited 2026-08-24: no such document is currently named
in this repository.
