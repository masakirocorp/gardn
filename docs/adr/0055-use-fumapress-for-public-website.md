---
status: accepted
---

# Use FumaPress for the public website

ADR 0054 established the durable boundary: Oh My Herdr's public marketing and documentation remain one static, asset-only site, while a dynamic product application waits for real server-authoritative state. This decision supersedes only ADR 0054's Astro and Starlight framework selection. The ownership, source-of-truth, generated-content, deployment, domain, and dynamic-application boundaries remain accepted.

Oh My Herdr will build the site as one FumaPress package under `website/`. Custom FumaPress routes own `/`, `/download`, and `/releases`; FumaPress documentation routes own `/docs/**`. FumaPress composes Waku routing with Fumadocs content and UI primitives, allowing marketing and documentation to share one build and design system without making marketing pages look like documentation templates.

The package produces an asset-only static build in `website/dist/public` for Cloudflare Workers Static Assets. Production requests must not require a Waku server route, server action, middleware, API endpoint, database, session store, or long-lived Node process. A dynamic requirement needs its own approval rather than silently turning the static deployment into a Worker application.

## Build and content boundaries

Use FumaPress's static plugins for FlexSearch, sitemap generation, link validation, `llms.txt`, and Tegami integration when they satisfy the product contract without custom plumbing. Generated Local API and release reference remains disposable, versioned build content derived from authoritative Rust sources, tagged binaries, and release metadata under ADR 0054; FumaPress is a renderer and content system, not a new source of truth.

Authored website TypeScript, TSX, and other supported web assets use Oxlint and Oxfmt. The repository must pin compatible FumaPress, Waku, and Fumadocs versions and keep focused build, typecheck, lint, format, link, and content-validation tasks in the pnpm/Turbo/CI graph.

## Tradeoff

FumaPress covers the planned documentation, custom-page, static-search, sitemap, link-validation, machine-readable documentation, and release-timeline surfaces with less integration code than assembling raw Fumadocs primitives in Next.js. It also uses Oxlint and Oxfmt in its own repository. This removes plumbing that would otherwise become Oh My Herdr maintenance surface.

The cost is maturity: FumaPress is pre-1.0 and currently depends on Waku 1.0 beta. Before downstream content work begins, the scaffold must prove a deterministic clean build, Oxc checks, static search, and an asset-only Cloudflare preview from the pinned dependency set. Because FumaPress is composed from Waku and Fumadocs primitives, Oh My Herdr retains a migration path to a lower-level Fumadocs/Waku application if FumaPress's opinions become limiting.

## Rejected alternatives

- **Continue with Astro and Starlight.** Their static model is sound, but Astro lacks complete Oxfmt support and only has partial Oxlint support. Keeping them would either weaken the selected Oxc toolchain or create formatting and lint exceptions at the website boundary.
- **Build directly on Fumadocs and Next.js.** This has a more mature framework base and explicit Oxc support, but Oh My Herdr would own more routing, search, sitemap, link-validation, release, and static-export integration. That flexibility is not required by the current site.
- **Add a dynamic Waku or Cloudflare Worker runtime.** No current public-site requirement needs request-time authority. Adding one would weaken the static boundary without user value.
