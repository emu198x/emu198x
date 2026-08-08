# Commodore 64 test data

This directory records the fixtures and fixture identities used to verify the
Commodore 64 implementation.

Its scope is evidence consumed by C64 tests and verification processes. It
does not contain C64 hardware specifications, general software archives or
claims inferred from the fixtures.

Neighbouring Commodore directories contain equivalent machine-family
evidence. C64 implementation and process documentation remains under
`knowledge/`; this directory supplies or identifies the bytes those documents
refer to.

Expected contents include redistributable synthetic fixtures, identity
manifests for external corpora and READMEs defining each fixture's assertion
boundary.

## Contents

- [`synthetic-kernal/`](synthetic-kernal/) contains the minimal KERNAL fixture
  used by focused tests.
- [`vicii-vice-survey/`](vicii-vice-survey/) pins the external programs,
  reference images and firmware used by the VIC-II breadth survey.
