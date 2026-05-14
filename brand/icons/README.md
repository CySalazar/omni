# OMNI OS — Iconography

**Direction:** C — Civic Tech / Generational
**Geometry:** 24×24 viewBox · 1.5 stroke-width · `stroke="currentColor"` · `fill="none"` · round caps/joins
**Color behavior:** Color is inherited from the consuming context. No multi-color icons. No fill states.
**License:** SIL Open Font License 1.1.

## Files in this directory

| Path | What it is | When to use |
|---|---|---|
| [`icons.svg`](./icons.svg) | **The sprite** — single SVG containing 16 `<symbol>` definitions, with `width="0" height="0"` so it renders invisibly. **Opening this file directly in a browser shows a blank page — that is by design.** | Production. Reference each symbol via `<use href="icons.svg#omni-mesh">`. |
| [`preview.html`](./preview.html) | Visual review page rendering all 16 icons at three sizes (16, 24, 48 px) and four palette colors. | Open in a browser to *see* every icon. Review-only — do not ship. |
| [`individual/`](./individual/) | 16 standalone SVG files, one per icon (`omni-mesh.svg`, `omni-tee.svg`, …). Each is a normal, openable, visible 24×24 SVG with charcoal default fill. | When you need a single icon file to drop into a slide, doc, or third-party tool that does not support sprite `<use>` references. |

> **Why the sprite looks empty when you open it.** `icons.svg` is intentionally a 0×0 SVG container holding 16 reusable `<symbol>` definitions. SVG symbols only render when referenced from somewhere else via `<use href="icons.svg#id">`. This is the standard pattern for icon libraries (Lucide, Heroicons, Feather, GitHub Octicons all work this way). If you want to *look at* the icons, open [`preview.html`](./preview.html) or any file in [`individual/`](./individual/).

## How to use

```html
<svg width="24" height="24" aria-hidden="true">
  <use href="brand/icons/icons.svg#omni-mesh" />
</svg>

<span style="color: var(--omni-petrol-500)">
  <svg width="20" height="20"><use href="brand/icons/icons.svg#omni-attestation" /></svg>
</span>
```

## Symbol catalog

The 16 symbols index the most-used concepts in OMNI communications.

| ID | Concept | When to use |
|---|---|---|
| `omni-mesh` | The federated mesh | Mesh protocol, peer-to-peer compute, network-level concepts |
| `omni-node` | A single attested peer | Single-machine view, "your computer is a node" |
| `omni-local-first` | Device with closed lock | Local-first principle, default-private compute |
| `omni-cloud-deny` | Cloud with strikethrough | Anti-cloud framing |
| `omni-attestation` | Shield with checkmark | TEE attestation, hardware verification |
| `omni-tee` | Chip with shield inset | Trusted Execution Environment (TDX, SEV-SNP, etc.) |
| `omni-kernel` | Three concentric rings | Microkernel architecture, OS-internal topics |
| `omni-agent` | Stylized head with internal process | AI agents, autonomous tasks |
| `omni-inference` | Input → transformation → output | Model inference, AI computation |
| `omni-encryption` | Envelope + key | Encrypted-by-default data types, cryptographic envelope |
| `omni-mesh-route` | Packet flowing through nodes | Routing, message-passing |
| `omni-governance` | Classical pillars + architrave | Governance documents, OIP process, board topics |
| `omni-fork` | Branching path | Forks-welcome posture, fork policy |
| `omni-oip` | Document with signature line | OMNI Improvement Proposals |
| `omni-zk` | Eye with strikethrough | Zero-knowledge proofs |
| `omni-anchor` | Anchor symbol | Mission Anchor, irrevocable principles |

## Design rules

1. **Single-color always.** Use `currentColor` only.
2. **No fill, only stroke.** Line-based icons.
3. **1.5 px stroke baseline.** At 24px viewBox.
4. **Round caps and joins.** Sharp corners read as "warning"; round corners read as "patient".
5. **Abstract over literal.** Icons index concepts, they do not illustrate.

## Anti-patterns

- ❌ Multi-color glyphs.
- ❌ Solid filled icons.
- ❌ Decorative iconography (sparkles, lightbulbs, rockets).
- ❌ Hand-drawn person-at-computer illustrations.
- ❌ Anthropomorphic AI mascots.
- ❌ Money/finance iconography (coins, chains).

## Adding a new icon

1. Confirm the concept appears in [`../STRATEGY.md`](../STRATEGY.md) §8 (Lexicon).
2. Draw on 24×24 grid, 1.5 stroke, round caps.
3. Append `<symbol id="omni-{concept}">` to [`icons.svg`](./icons.svg) alphabetically.
4. Document in the catalog table above.
5. Verify at 16 px and 64 px.
6. Open a Draft PR.
