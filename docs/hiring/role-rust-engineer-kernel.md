# Senior Rust Engineer — Kernel and Embedded

**Status:** Job description draft (Status v0.1)
**Posting target:** post-Phase-0 closure (Q3-Q4 2026)
**Location:** EU / EEA remote, with quarterly on-site (Amsterdam) for
in-person sprints.
**Compensation:** see [`docs/hiring/salary-bands.md`](salary-bands.md).
**Type:** full-time employment with Stichting OMNI.

---

## About OMNI OS

OMNI OS is an AI-native, privacy-first, decentralized operating system. We
are building a Rust microkernel that treats AI as a first-class kernel
primitive while enforcing privacy cryptographically. The project is in
Phase 0 (foundation); foundational crates are landed and verified; Phase 1
(microkernel implementation) starts upon Stichting OMNI constitution and
funding closure.

This role is one of the **two senior engineers** the project hires to
execute Phase 1.

## What you will do

You will be responsible for the bare-metal Rust microkernel:

- Lead the `no_std` + `no_main` transition of `crates/omni-kernel`.
- Design and implement memory management (page tables, virtual memory, allocators).
- Implement the scheduler (capability-aware, thermal-aware, AI-workload-aware).
- Implement typed message-passing IPC with capability validation at every boundary.
- Implement the syscall surface and dispatch.
- Integrate with the UEFI bootloader (per `OIP-Kernel-003`).
- Implement the first userspace drivers (NVMe storage, networking, TEE).
- Work with the cryptographer (P3.2 engagement) on capability system correctness.
- Participate in the OIP process for kernel-impacting decisions.
- Contribute to the first external security audit at Phase 1 closure.

## What we expect

**Required:**

- 5+ years of professional Rust, including at least one production
  systems-programming context (kernel, embedded, hypervisor, browser engine,
  database internals).
- Comfortable working in `no_std + no_main` Rust with custom allocators and
  custom panic handlers.
- Experience with one of: `redox-os`, `Theseus`, `Tock`, `Hubris`, or
  another from-scratch Rust kernel. OR equivalent work on a non-Rust kernel
  (Linux, seL4, Zircon) plus willingness to learn `no_std` Rust patterns.
- Familiarity with: x86_64 paging, interrupt handling, UEFI boot,
  capability-based security models (Macaroons, seL4 capabilities).
- Strong test discipline: unit tests + property tests + fuzz harnesses.
- Comfortable with formal-methods literature even if not currently using TLA+
  or similar daily.
- English working language (Italian / Dutch nice-to-have).
- Mission alignment with the OMNI OS principles (local-first, privacy-first,
  anti-capture).

**Bonus:**

- Experience with TEE attestation (Intel TDX, AMD SEV-SNP, ARM CCA).
- Prior public contribution to one of: a Rust microkernel, a microkernel
  community, a public Rust crate at version >= 0.5 maintained for ≥1 year.
- Familiarity with Tamarin Prover or comparable protocol-verification tools.
- Background in operating-systems research (publications, university
  affiliation, etc.).

**Not required:**

- AI / ML expertise (this role is OS-focused; the AI Runtime Service is
  Phase 2, owned by the networking-focused engineer + founder).
- C/C++ expertise (we are Rust-only; legacy languages are not part of the
  daily work).

## What we offer

- **Salary**: EUR 95,000–135,000 annualized, depending on seniority and
  location (per [`docs/hiring/salary-bands.md`](salary-bands.md)).
- **Equity-equivalent**: not available (Stichting OMNI is a foundation, not
  a company; there is no equity). Compensated via salary band only.
- **Benefits**: standard NL employment benefits (health insurance,
  pension contribution, vacation accrual, sick leave). Remote workers get
  comparable benefits through an EOR (Employer of Record) arrangement.
- **Work environment**: small, mission-driven team. No CEO/CTO theatre. The
  Lead Architect (founder) is your direct collaborator. Decisions are made
  by the smallest competent group, with the OIP process as the appeal path.
- **Public credit**: all substantive work attributed in
  [`CONTRIBUTORS.md`](../../CONTRIBUTORS.md) and in release notes.
- **Apache-2.0 ownership**: your contributions are Apache-2.0; you retain copyright
  per the DCO; no CLA.

## How to apply

Send to `hello@omni-os.org` (or `cySalazar@cySalazar.com` until the
Foundation mailbox exists):

1. A CV or LinkedIn / GitHub equivalent.
2. A brief cover letter (≤500 words) explaining: why this role, what excites
   you about OMNI OS, and one specific concern you have about the project.
3. A code sample of your choice that demonstrates your Rust kernel /
   systems work. Public repositories preferred; non-public samples accepted
   under NDA.

**No coding interview circus.** The interview is a 90-minute conversation
where we walk through your code sample, your understanding of OMNI OS
architecture, and the candid mission-alignment check. A follow-up 60-minute
session covers compensation and start date.

**Total hiring process: ≤4 weeks from application to offer.**

## Diversity statement

Stichting OMNI is committed to a diverse, inclusive team. We actively encourage
applications from candidates underrepresented in systems software (women,
people of color, LGBTQ+, neurodivergent, people from outside Western Europe
and North America). The Foundation will fund reasonable accommodation costs
for any qualifying disability without requiring disclosure during the hiring
process.

## Conflict of interest

Candidates currently employed by, or holding board positions at, any
organization on the Excluded Sources list (per
[`docs/08-funding-policy.md`](../08-funding-policy.md)) must disclose this
during the interview. The Foundation evaluates each case; employment with
a regulated cloud provider is not automatically disqualifying but is
discussed openly.
