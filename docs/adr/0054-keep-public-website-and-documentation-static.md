---
status: accepted
---

# Keep the public website and documentation static

Oh My Herdr will keep its public marketing site and product documentation in one Astro 5 package under `website/`. Astro custom pages own `/`, `/download`, and `/releases`; Starlight owns `/docs/**`. The package builds one static asset tree with no application server, database, session store, or server-rendered route.

The canonical product origin serves that static tree. `www` redirects to the canonical origin. The hostname is deployment configuration rather than a source-ownership boundary. `app.<product-domain>` is reserved and must not point at the static site or imply that a hosted product application exists.

## Content ownership

Public marketing pages, user guides, and public reference prose live under `website/**`. Root `docs/**` remains maintainer-facing: ADRs, release procedure, manual QA, and internal engineering explanations are not published wholesale as the product manual. Useful facts may be rewritten for public readers, but the public site does not import root documentation as an undifferentiated second source.

Rust source and tagged Oh My Herdr binaries remain authoritative for CLI commands, configuration, plugin contracts, Local API shapes, and runtime behavior. Tagged GitHub Releases remain authoritative for released versions, release notes, and downloadable assets under ADR 0012. Authored website prose explains those contracts; it does not redefine them.

Generated CLI, configuration, Local API, and release reference content is disposable build output. It must be reproducible from an explicit release candidate or tag, kept separate from authored prose, and never edited as the source of truth. The published site identifies the product version its generated reference describes. An untagged `master` build may be used for preview, but it must not silently replace latest-release documentation.

The ENG-110 public-contract audit found that current public coverage is sparse, raw Local API JSON Schema lacks method semantics, and install claims depend on release assets. This architecture assigns those gaps to two layers: generators preserve exact machine shape, while authored Starlight pages provide lifecycle, error, trust, security, and task guidance. Internal client/server wire and handoff protocols remain maintainer contracts unless a later decision deliberately makes them public.

## Local API reference

The Local API is newline-delimited JSON over a local socket under ADR 0013. It is not an HTTP API and does not have an OpenAPI contract. `omh api schema --json` produces JSON Schema for requests, responses, operational events, and subscriptions, but generated shapes alone do not explain method-to-result mappings, errors, ordering, connection lifetime, or the same-user security boundary.

Public API documentation therefore combines versioned generated shape reference with authored Starlight pages. The site must not label the Local API as REST, expose an invented HTTP endpoint, or imply that OpenAPI tooling is authoritative. If Oh My Herdr later adds an actual HTTP API, that API needs its own contract and architecture decision.

## Deployment

Cloudflare Workers Static Assets hosts the generated site as one deployment and one origin. The deploy input is Astro's static build output. Cloudflare owns TLS, custom-domain routing, preview deployments, redirects, and static response headers; the site does not add Worker code merely to serve files.

Build-time fetches may materialize public release metadata, but production requests do not depend on GitHub, Laravel, a database, or a long-lived Node process. A narrow hosted endpoint or third-party service for analytics, a contact form, or a download counter does not by itself justify converting the site into a dynamic application.

## Dynamic application boundary

A separate Laravel/Inertia application is justified only when Oh My Herdr has real server-authoritative product state, such as:

- authenticated user or organization accounts;
- billing, subscriptions, licenses, or entitlements;
- mutable customer-owned cloud data or synchronized settings;
- team roles, permissions, or administrative workflows;
- hosted control of remote resources that requires protected credentials or server-side execution.

If that boundary becomes real, the application belongs at `app.<product-domain>` as a separate package and deployment. It may share design tokens and links with the static site, but it does not absorb the marketing/docs package by default. The public site remains static until its own requirements demand otherwise.

## Considered alternatives

- **Laravel/Inertia plus a separate documentation deployment.** Rejected for the current product because it creates two frameworks, deployments, navigation systems, and operational surfaces before any server-owned product state exists.
- **Fumadocs embedded in Inertia.** Rejected because it would couple documentation to a React integration inside a Laravel application and still would not make the local JSON socket API an OpenAPI service. Its strengths do not justify the application runtime or framework seam here.
- **A custom Laravel Markdown documentation engine.** Rejected because Oh My Herdr would own content loading, routing, navigation, search, code rendering, and static optimization that Starlight already provides.
- **Publish root `docs/**` directly.** Rejected because maintainer procedure and architecture records have a different audience, structure, and stability contract from public product documentation.

## Consequences

`website/**` is an Oh My Herdr-owned product surface and remains protected from blind upstream replacement under ADR 0008. One visual system, build, search index, deployment, and domain hierarchy serve marketing and docs. Starlight can own documentation navigation and search without constraining custom Astro marketing pages.

Site builds and launch review must verify internal links, generated-reference freshness, release download targets, and source-version metadata. Generated content can be deleted and rebuilt without losing authored documentation. Product behavior changes flow from code and release contracts into regenerated reference and then into any affected authored guidance; website prose never becomes a compatibility shim for stale behavior.
