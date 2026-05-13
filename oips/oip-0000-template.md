---
oip: 0000
title: Reserved template (do not use as a real OIP)
track: Meta
status: Withdrawn
authors:
  - cySalazar <cySalazar@cySalazar.com>
created: 2026-05-10
updated: 2026-05-10
requires: []
supersedes: ~
superseded-by: ~
discussion: ~
license: CC0-1.0
---

<!--
  This file is the "OIP-0000" sentinel — number 0000 is permanently reserved.

  Do NOT copy this file to file a new OIP. Use `oip-template.md` instead.

  Why this file exists separately:
  - The lint (`scripts/lint-oips.py`) treats numbered files as OIPs proper. Reserving 0000 here
    means no real OIP can ever claim that number, so external references to "OIP-0000" remain
    unambiguous (e.g., when documentation cites the template).
  - It also gives the lint a fixed reference shape to validate against.

  Status `Withdrawn` is used (rather than `Draft`) so this file does not appear as an active
  proposal in the registry index.
-->

## Abstract

Reserved sentinel. This is not a real OIP. Use `oip-template.md` to start a new proposal.

---

## Motivation

Number `0000` is reserved to prevent collisions with the canonical template and to anchor any
historical reference to "OIP-0000" to a stable, discoverable file.

---

## Specification

The number `0000` MUST NOT be assigned to any new OIP. The OIP editors MUST reject any
submission claiming this number.

---

## Rationale

BIP-0000, EIP-0, and PEP-0 follow analogous conventions. Reserving the number in a real file
(rather than leaving it implicit) prevents accidental reuse and gives static linters a concrete
target.

---

## Backwards Compatibility

N/A — first introduction, no prior behavior.

---

## Test Cases

N/A — sentinel file, no testable invariant beyond "the lint passes against it".

---

## Reference Implementation

The lint at `scripts/lint-oips.py` treats this file as a valid OIP for structural purposes
while keeping its `status: Withdrawn` to exclude it from the active index.

---

## Security Considerations

None. This file does not change any runtime behavior, trust relationship, or authority.

---

## Privacy Considerations

None.

---

## Copyright

This OIP is released into the public domain under
[CC0-1.0](https://creativecommons.org/publicdomain/zero/1.0/).
