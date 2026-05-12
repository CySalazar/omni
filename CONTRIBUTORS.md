# Contributors

OMNI OS is, today, a single-founder project in the Phase-0 foundation phase. This
document tracks individuals and organizations that have contributed to the project
in any verifiable capacity.

## Roles

| Role | Holder | Period | Notes |
|---|---|---|---|
| **Lead Architect / BDFL** | cySalazar (`cySalazar@cySalazar.com`) | 2026-05-09 → 2031-05-09 (BDFL veto window) | See [`docs/05-governance.md`](docs/05-governance.md) § "Founder role". |
| **OIP Editors** | *(TBD — appointment in Phase 0 closure)* | annual rotation | Two editors per term per `OIP-Process-001`. |
| **Cryptographer (peer review)** | *(TBD — see [`todo.md`](todo.md) P3.2)* | engagement TBD | Will be appointed via `docs/audits/cryptographer-engagement-template.md`. |
| **Stichting OMNI Trustees** | *(TBD — Phase 0 closure)* | 3-year rotating mandates | Five trustees including founder, ≥1 NL-resident. See [`docs/legal/bylaws-draft.md`](docs/legal/bylaws-draft.md). |

## Code contributors

All code contributors must:

1. Sign their commits cryptographically (SSH ed25519 or GPG; see
   [`CONTRIBUTING.md`](CONTRIBUTING.md) § "Signing").
2. Sign off on the DCO (`Signed-off-by:` trailer; enforced by
   `.github/workflows/dco.yml`).
3. Be listed below upon their first merged contribution.

Generated automatically from `git log` (script under `scripts/regen-contributors.sh`,
TBD); manual maintenance until that lands.

### Active maintainers

- **cySalazar** — Lead Architect, founder. First commit: `61426d5` (2026-05-09).

### Past contributors

*(none yet)*

## Acknowledgements

- The **RustCrypto** maintainers, whose crates (`chacha20poly1305`,
  `ed25519-dalek`, `x25519-dalek`, `sha2`, `sha3`, `blake3`, `hkdf`, `argon2`,
  `subtle`, `zeroize`) are the cryptographic base of `omni-crypto`.
- The **seL4** project, prior art for verified microkernels; cited as a
  long-term aspirational target in [`docs/02-architecture.md`](docs/02-architecture.md).
- The **Tamarin Prover** team for the symbolic protocol verifier used in
  [`protocol-proofs/handshake.spthy`](protocol-proofs/handshake.spthy).
- The **Contributor Covenant** project for the basis of our
  [`CODE_OF_CONDUCT.md`](CODE_OF_CONDUCT.md).

## How to be listed

Submit a pull request that lands a substantive change — code, documentation,
OIP, security finding, formal-method proof, or translation. Trivial fixes
(typos, lint adjustments) are welcome but do not result in listing.

Listing carries no legal weight. Stichting OMNI (once constituted) maintains
the authoritative list of trustees, employees, and contractors separately.
