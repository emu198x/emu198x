# 198x Project Constitution

**Purpose**
This document defines the guiding principles for every 198x project, whether written by a human or assisted by AI.

It is intentionally technology-agnostic.

Every repository within the 198x family should inherit these principles and add only project-specific guidance where necessary.

---

# Philosophy

198x keeps three documents at its root.

[`MANIFESTO.md`](MANIFESTO.md) says why the project exists.

This document settles disagreements.

`PRACTICE.md` describes the default method of working on a subject. It sits beside this file at the umbrella, and is published in each organisation's `.github` repository.

Motive, rules, method. Where the method and the rules disagree, the rules win.

The project teaches more than *how* to build software.

It continually asks:

> **Why does it work?**

That question is pursued through every layer of abstraction, from applications, to operating systems, to CPUs, to digital logic, to electronics, and ultimately to physics.

Where practical, explanations should continue until they reach the natural boundary of the discipline.

---

# Guiding Principles

## Curiosity first

Curiosity is the driving force behind the project.

Questions are more valuable than assumptions.

Keep asking "why?"

---

## Understanding over implementation

Working software does not settle the question.

Something can run correctly and still not be understood.

Software, documentation and tooling exist to deepen understanding.

Understanding is what this document checks for.

What it is ultimately for is in `MANIFESTO.md`.

---

## Capability, not specifications

No machine has a single property called "power".

Every machine is a set of capabilities, constraints and trade-offs.

"Which is more powerful?" is not a question until it becomes "more powerful for what?"

Answer the second one, name the constraint, and say what it cost.

---

## Reality over mythology

Document the world as it is.

Not as people remember it.

Not as people wish it had been.

---

## Evidence over repetition

Repeated claims are not evidence.

Prefer observation over tradition.

---

## Machines are historical artefacts

Computers exist within:

- engineering
- economics
- geography
- politics
- manufacturing
- culture

The wider context often explains why a machine exists in its current form.

Explain enough context to understand the machine.

Avoid turning technical documentation into political commentary.

---

## People built computers

Computers are deterministic.

People are not.

Design mistakes...

Manufacturing compromises...

Commercial pressure...

Budget limitations...

Changing requirements...

...all become part of computing history.

Preserve them.

Do not sanitise them.

---

# Universal Working Rules

## Preserve intent

Assume existing project decisions are deliberate.

Improve them before replacing them.

---

## Explain reasoning

Whenever making a recommendation:

- explain why
- explain the mechanism
- explain assumptions
- explain trade-offs

Avoid agreement without reasoning.

Avoid disagreement without reasoning.

---

## Locate the disagreement before settling it

Two people can contradict each other and both be right.

"Old computers felt faster" and "modern computers are enormously faster" describe different layers of the same system.

Before adjudicating a disagreement, establish which layer each side is talking about, and what each of them means by the words.

Many arguments about computing are not disputes about fact. They are two accurate observations taken from different heights.

---

## Distinguish certainty

Always distinguish between:

- fact
- observation
- inference
- hypothesis
- counterfactual
- opinion
- preference

Unknown is an acceptable answer.

A counterfactual is never presented as something that happened.

Exploring what a machine could have been is legitimate work; letting a reader mistake it for history is not.

---

## Don't invent decisions

Ideas discussed are not automatically adopted.

Only treat decisions as final once explicitly confirmed.

---

## Preserve optionality

Avoid unnecessarily narrowing future design space.

---

## Curiosity does not amend scope

An interesting thread is not, by itself, a reason to widen what the project covers.

Scope is settled in the decision records.

See `PRACTICE.md` for when to stop pulling a thread.

---

## Challenge execution more than ambition

Ambitious ideas are welcome.

Critique:

- sequencing
- implementation
- complexity
- evidence
- assumptions

Do not dismiss ideas because they are ambitious.

---

## Explain "best practices"

Never recommend a best practice without explaining:

- why it exists
- when it applies
- when it does not

---

## Make recommendations falsifiable

Every recommendation should answer:

- Why this?
- Under what conditions would this fail?
- What evidence would change the recommendation?

---

# Editorial Standards

## Corrections are edits

Published material should not expose the editing process.

Remove:

- correction summaries
- editorial notes
- revision histories
- TODOs
- prompt residue
- AI commentary

Git records edits.

Published documents present finished work.

---

## Separate evidence from provenance

Good:

> Sources disagree on this date.

Bad:

> This page previously contained an incorrect date.

Evidence belongs in documents.

Editorial history belongs elsewhere.

---

## Preserve voice

Improve:

- clarity
- consistency
- accuracy

Preserve:

- personality
- humour
- tone
- intent

---

## Remove production artefacts

Readers should never see:

- placeholders
- duplicated paragraphs
- editing commentary
- implementation notes

unless explicitly intended.

---

# Engineering Standards

## Respect architecture

Assume existing architectural boundaries exist for a reason.

Understand them before proposing alternatives.

---

## Prefer composition

Extract genuine commonality.

Model genuine differences.

Avoid abstraction that hides important behaviour.

---

## Build reusable knowledge

Every improvement should benefit all consumers where appropriate.

---

## Optimise for understanding

Readable software is preferable to clever software.

Explicitness is preferable to hidden behaviour.

---

## Historical accuracy over aesthetic consistency

If hardware behaved strangely:

Model the behaviour.

Do not "correct" history.

---

## Treat anomalies as research

Unexpected behaviour may reveal:

- undocumented hardware
- manufacturing revisions
- software assumptions
- historical mistakes

Investigate before changing.

---

# Research Standards

## Evidence hierarchy

When available, prefer evidence in approximately this order:

1. Original hardware
2. Original software (ROMs, tapes, disks, cartridges, executables)
3. Original source code
4. Original manuals
5. Contemporary technical documentation
6. Contemporary books
7. Contemporary magazines
8. Contemporary advertisements
9. Contemporary interviews
10. Developer recollections
11. Modern books
12. Modern articles
13. Community databases
14. AI-generated summaries

This is guidance, not absolute law.

When stronger evidence contradicts weaker evidence, investigate.

---

## Verify whenever practical

If software exists:

Run it.

If hardware exists:

Inspect it.

If source exists:

Read it.

If the emulator can answer the question:

Use it.

Prefer direct observation over secondary description.

---

## Trust artefacts over testimony

The artefact is the primary witness.

Documentation explains the artefact.

The artefact does not explain the documentation.

---

## Independent verification

Multiple independent observations outweigh repeated citation.

Ten websites quoting one incorrect source remain one incorrect source.

---

## Preserve uncertainty

Conflicting evidence should remain visible.

Do not manufacture certainty.

---

## Build narratives from evidence

Facts come first.

Narratives are constructed afterwards.

Never change evidence to preserve a narrative.

---

# Final Principle

The purpose of 198x is not to create emulators.

Nor websites.

Nor assemblers.

Nor books.

Nor documentation.

Those are all means.

Preservation serves understanding.

Understanding serves creation.

The purpose is to understand computing well enough to build something new with what was learned.

Every contribution should leave the project knowing something it did not know before.
