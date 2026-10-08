# Businex web design system

Direction for the SolidJS business desktop (Phase 3). Written after reading the five required UI
skills in full: design-taste, frontend-design, frontend-ui-ux, impeccable, ui-ux-pro-max, plus
component-taste.md and the WCAG 2.2 AA checklist. Mode: Operate (impeccable) — the visitor
completes business tasks; scanability, consistency and native expectations outrank expression.

## Dials (design-taste: set these first)

| Dial | Setting | Why |
| --- | --- | --- |
| Product | Operational tool (business desktop) | CRM, projects, invoices, team ops for small businesses |
| Variance | 4 / 10 (balanced, modern) | Trust and scanability over novelty |
| Motion | 2 / 10 (subtle) | Daily repeated work; motion must never slow it down |
| Density | 8 / 10 (dense dashboard) | Operators scan many records per view |

## Tokens (single source of truth: packages/ui/src/styles/tokens.css)

- Color: canvas #f7f9fc, surface #ffffff, ink #16233b (body, 14:1 on surface), ink-2 #5a6b87
  (secondary, 4.9:1), line #dce3ee, brand #1e40af (interactive, 8.6:1 with white), brand-soft
  #e8eefc (selection), accent #b45309 (attention only; amber-700 so it clears 4.5:1 as text),
  success #15803d, danger #b91c1c, rail #101c30 (nav shell). Max three intentional colors per
  component; semantic color always pairs with text or an icon, never color alone.
- Type: Fira Sans (UI) and Fira Code (identifiers, codes, tabular figures only — never
  decorative labels). Scale 12 / 14 / 16 / 20 / 24 / 32; body 16px, dense table chrome 14px,
  captions 12px. Line-height 1.5 body, 1.25 headings.
- Space: 4px grid — 4 / 8 / 12 / 16 / 24 / 32 / 48. Internal padding smaller than the gap
  between cards.
- Radius: cards 10px, controls 6px, nested inner elements 4px (inner = outer minus padding).
- Elevation: data surfaces use a 1px line, not shadows (dense style). Shadows only for overlays
  (dialog, toast): a tight shadow layered with a soft one.
- Motion: 120ms micro, 240ms transitions, ease-out; opacity and transform only; never
  transition: all; prefers-reduced-motion collapses durations to near zero.

## Layout

Desktop (1200px+): dark command rail 240px wide (navigation; the active item is a fully inverted
pill — selection by inversion, one inverted element per view), top bar with company switcher and
user menu, content fills the rest with 24px gutters. Tables span the width and scroll inside
their own container; the page body never scrolls sideways.

    +--------+---------------------------------------------+
    | rail   | top bar: company switcher        user menu   |
    | Home   +---------------------------------------------+
    | CRM    | page title                        primary   |
    | Proj.  |                                             |
    | Docs   |  [KPI] [KPI] [KPI]                          |
    | Invoi. |                                             |
    | Team   |  table / form (full width)                  |
    | Agents |                                             |
    | Sett.  |                                             |
    +--------+---------------------------------------------+

Mobile (375px): the rail becomes a top drawer (hamburger), one column, forms stack at full
width, interactive targets are at least 44px, tables reflow into row cards.

## Principles

1. One focal point per view; one primary action per screen.
2. Active navigation uses full inversion (white pill on the dark rail).
3. Numbers are the product: KPI metrics use tabular figures and mixed-size number/unit pairs.
4. Errors are local, plain-language and state the fix; empty states explain what appears here
   and give one next action.
5. Semantic HTML first: real form labels, the native dialog element, table headers with scope,
   aria-live regions for asynchronous status.

## Review against the brief (frontend-design pass)

Rejected defaults: purple-blue gradient hero, the SaaS-card kit of identical rounded cards with
gradient washes, ALL-CAPS eyebrow labels, a monospace face for decorative labels, Inter. Kept the
data-backed Data-Dense Dashboard style and the Fira pairing returned by the ui-ux-pro-max search
("dashboard, data, technical, precise") because the subject is an operations desk. The dark
command rail is the single personality choice; everything else stays quiet.
