"use client";

import { useEffect, useState } from "react";
import { platformRows, releasePreview, type RecentFeature } from "./responsive-design-data";

type PageName = "home" | "download" | "releases";
type ReleaseState = "prepublic" | "tagged" | "loading" | "error";
type ThemeName = "light" | "dark";

const pageNames: PageName[] = ["home", "download", "releases"];
const releaseStates: ReleaseState[] = ["prepublic", "tagged", "loading", "error"];
const themeNames: ThemeName[] = ["light", "dark"];

function isThemeName(value: string | null): value is ThemeName {
  return value !== null && themeNames.includes(value as ThemeName);
}

function BrandMark() {
  return (
    <svg className="rd-brand-mark" viewBox="0 0 256 256" aria-hidden="true">
      <g
        fill="none"
        stroke="currentColor"
        strokeWidth="14"
        strokeLinecap="round"
        strokeLinejoin="round"
      >
        <path d="M128 38 176 72 128 112 80 72Z" />
        <path d="M80 72v68l48 40 48-40V72" />
        <path d="M128 112v68" />
        <path d="M91 170 44 188l84 36 84-36-47-18" />
      </g>
    </svg>
  );
}

function isPageName(value: string | null): value is PageName {
  return value !== null && pageNames.some((page) => page === value);
}

function isReleaseState(value: string | null): value is ReleaseState {
  return value !== null && releaseStates.some((state) => state === value);
}


export function ResponsiveDesignPrototype() {
  const [page, setPage] = useState<PageName>("home");
  const [releaseState, setReleaseState] = useState<ReleaseState>("prepublic");
  const [theme, setTheme] = useState<ThemeName>("light");

  useEffect(() => {
    const search = new URLSearchParams(window.location.search);
    const requestedPage = search.get("page");
    const requestedState = search.get("state");
    const requestedTheme = search.get("theme");
    if (isPageName(requestedPage)) setPage(requestedPage);
    if (isReleaseState(requestedState)) setReleaseState(requestedState);
    if (isThemeName(requestedTheme)) setTheme(requestedTheme);
  }, []);

  const selectPage = (nextPage: PageName) => {
    setPage(nextPage);
    const url = new URL(window.location.href);
    url.searchParams.set("page", nextPage);
    url.searchParams.set("state", releaseState);
    url.searchParams.set("theme", theme);
    window.history.replaceState({}, "", url);
    window.scrollTo({ top: 0, behavior: "instant" });
  };

  const selectState = (nextState: ReleaseState) => {
    setReleaseState(nextState);
    const url = new URL(window.location.href);
    url.searchParams.set("page", page);
    url.searchParams.set("state", nextState);
    url.searchParams.set("theme", theme);
    window.history.replaceState({}, "", url);
  };

  return (
    <div className="rd-root" data-theme="light">
      <a className="rd-skip" href="#rd-main">
        Skip to content
      </a>
      <SiteHeader />
      <main id="rd-main">
        <HomePage />
      </main>
      <SiteFooter />
    </div>
  );
}


function PrototypeBar({
  theme,
  onTheme,
}: {
  theme: ThemeName;
  onTheme: (theme: ThemeName) => void;
}) {
  return (
    <aside className="rd-prototype-bar" aria-label="Design prototype controls">
      <div className="rd-prototype-note">
        <strong>ENG-128</strong>
        <span>Brand system · provisional copy</span>
      </div>
      <label className="rd-state-control">
        <span>Theme</span>
        <select
          value={theme}
          onChange={(event) => {
            const nextTheme = event.target.value;
            if (isThemeName(nextTheme)) onTheme(nextTheme);
          }}
        >
          <option value="light">Light</option>
          <option value="dark">Dark</option>
        </select>
      </label>
    </aside>
  );
}

function SiteHeader() {
  return (
    <header className="rd-header">
      <a className="rd-brand" href="/">
        <BrandMark />
        <span>Gardn</span>
      </a>
      <nav className="rd-nav" aria-label="Primary">
        <a href="/docs">Docs</a>
        <a href="https://github.com/masakirocorp/gardn">GitHub</a>
      </nav>
    </header>
  );
}

function HomePage() {
  return (
    <section className="rd-hero rd-shell" aria-labelledby="rd-home-title">
      <figure className="rd-shot">
        <img
          className="rd-shot-frame"
          src="/session.png"
          width={1284}
          height={820}
          alt="A Gardn session with product, ops, and commerce groups, a split checkout space, and agents in triage, working, and idle."
        />
      </figure>
      <h1 id="rd-home-title">All agents, all terminals, all machines, one session.</h1>
      <InstallCommand />
    </section>
  );
}


function RecentSection({
  releaseState,
  onState,
}: {
  releaseState: ReleaseState;
  onState: (state: ReleaseState) => void;
}) {
  if (releaseState === "loading") {
    return (
      <section className="rd-recent rd-shell" aria-busy="true">
        <SectionHeading
          eyebrow="Release proof"
          title="Checking the latest release…"
          id="rd-recent-title"
        >
          Release data is loading from the verified static source.
        </SectionHeading>
        <div className="rd-skeleton-grid">
          <i />
          <i />
          <i />
          <i />
        </div>
      </section>
    );
  }
  if (releaseState === "error") {
    return (
      <section className="rd-recent rd-shell">
        <SectionHeading
          eyebrow="Release proof"
          title="Release data is unavailable."
          id="rd-recent-title"
        >
          The page does not guess. Installation remains available from source while release metadata
          is checked.
        </SectionHeading>
        <div className="rd-inline-error" role="status">
          <span>Could not verify tagged release data.</span>
          <button type="button" onClick={() => onState("prepublic")}>
            Try again
          </button>
        </div>
      </section>
    );
  }

  const tagged = releaseState === "tagged";
  return (
    <section className="rd-recent rd-shell" aria-labelledby="rd-recent-title">
      <div className="rd-recent-heading">
        <SectionHeading
          eyebrow={tagged ? releasePreview.version : "Current source"}
          title={tagged ? "Shipped recently." : "What is taking shape."}
          id="rd-recent-title"
        >
          {tagged
            ? releasePreview.summary
            : "Pre-public behavior: recent work is described as current source, never promoted as shipped before a tagged release exists."}
        </SectionHeading>
        <p className="rd-release-date">
          {tagged ? releasePreview.date : "No tagged public release"}
        </p>
      </div>
      <div className="rd-feature-grid">
        {releasePreview.features.map((feature) => (
          <FeatureCard key={feature.title} feature={feature} />
        ))}
      </div>
      <a className="rd-text-link rd-recent-link" href="?page=releases">
        See the full release history <span>↗</span>
      </a>
    </section>
  );
}

function FeatureCard({ feature }: { feature: RecentFeature }) {
  return (
    <article className="rd-feature-card">
      <div>
        <span>{feature.category}</span>
        {feature.label && <code>{feature.label}</code>}
      </div>
      <h3>{feature.title}</h3>
      <p>{feature.summary}</p>
      {feature.href && (
        <a href={feature.href} aria-label={`Read about ${feature.title}`}>
          Read more <span>↗</span>
        </a>
      )}
    </article>
  );
}

const installCommand = "curl -fsSL https://gardn.dev/install | sh";

function InstallCommand() {
  const [copied, setCopied] = useState(false);

  const copy = async () => {
    try {
      await navigator.clipboard.writeText(installCommand);
      setCopied(true);
      window.setTimeout(() => setCopied(false), 1500);
    } catch {
      setCopied(false);
    }
  };

  return (
    <div className="rd-install-box">
      <div className="rd-install-row">
        <code>{installCommand}</code>
        <button type="button" onClick={() => void copy()}>
          {copied ? "Copied" : "Copy"}
        </button>
      </div>
    </div>
  );
}

function DownloadPage() {
  return (
    <section className="rd-hero rd-shell" aria-labelledby="rd-install-title">
      <h1 id="rd-install-title">Install</h1>
      <InstallCommand />
    </section>
  );
}

function InstallOption({
  index,
  title,
  copy,
  command,
}: {
  index: string;
  title: string;
  copy: string;
  command: string;
}) {
  return (
    <article className="rd-install-option">
      <span>{index}</span>
      <h3>{title}</h3>
      <p>{copy}</p>
      <div className="rd-code">
        <pre>
          <code>{command}</code>
        </pre>
        <button type="button" aria-label={`Copy ${title} command`}>
          copy
        </button>
      </div>
    </article>
  );
}

function ReleaseControl({
  state,
  onRetry,
}: {
  state: ReleaseState;
  onRetry: () => void;
}) {
  if (state === "loading")
    return (
      <div className="rd-release-control" aria-busy="true">
        <div className="rd-spinner" />
        <div>
          <b>Checking release artifacts</b>
          <p>Matching tags, checksums, architectures, and release metadata.</p>
        </div>
        <button type="button" disabled>
          Checking
        </button>
      </div>
    );
  if (state === "error")
    return (
      <div className="rd-release-control rd-release-control-error">
        <span aria-hidden="true">!</span>
        <div>
          <b>Release verification unavailable</b>
          <p>No download is offered until authoritative metadata can be checked.</p>
        </div>
        <button type="button" onClick={onRetry}>
          Try again
        </button>
      </div>
    );
  if (state === "tagged")
    return (
      <div className="rd-release-control rd-release-control-ready">
        <span aria-hidden="true">✓</span>
        <div>
          <b>Gardn {releasePreview.version}</b>
          <p>macOS · Apple silicon · design fixture</p>
        </div>
        <button type="button">Download</button>
      </div>
    );
  return (
    <div className="rd-release-control">
      <span aria-hidden="true">—</span>
      <div>
        <b>Public binaries unavailable</b>
        <p>Install from source or Nix while the first public release is verified.</p>
      </div>
      <button type="button" disabled>
        Not available
      </button>
    </div>
  );
}

function ReleasesPage({
  releaseState,
  onPage,
  onState,
}: {
  releaseState: ReleaseState;
  onPage: (page: PageName) => void;
  onState: (state: ReleaseState) => void;
}) {
  const tagged = releaseState === "tagged";
  return (
    <>
      <PageHero
        eyebrow="Release history"
        title={
          tagged
            ? "Verified changes, without the archaeology."
            : "Release history starts at the gate."
        }
        status={tagged ? releasePreview.version : "Pre-public"}
      >
        {tagged
          ? "Each release pairs an editorial summary with explicit artifacts, compatibility, and the details needed to update safely."
          : "No public binary release is announced yet. This page stays useful without inventing changelog history or promoting preview work."}
      </PageHero>

      {releaseState === "loading" && (
        <section className="rd-release-list rd-shell" aria-busy="true">
          <div className="rd-release-skeleton">
            <i />
            <i />
            <i />
          </div>
        </section>
      )}
      {releaseState === "error" && (
        <section className="rd-release-list rd-shell">
          <div className="rd-release-empty rd-release-empty-error">
            <span>!</span>
            <div>
              <h2>Release history could not be verified.</h2>
              <p>
                The page fails closed: no version, asset, or install action is inferred from stale
                data.
              </p>
            </div>
            <button className="rd-button" type="button" onClick={() => onState("prepublic")}>
              Try again
            </button>
          </div>
        </section>
      )}
      {releaseState === "prepublic" && (
        <section className="rd-release-list rd-shell">
          <div className="rd-release-empty">
            <span>00</span>
            <div>
              <p className="rd-eyebrow">Right now</p>
              <h2>No tagged public releases.</h2>
              <p>
                The source checkout and Nix flake are available. Tagged releases, notes, checksums,
                and assets will appear here after the release gate passes.
              </p>
            </div>
            <a
              className="rd-button rd-button-primary"
              href="?page=download"
              onClick={(event) => {
                event.preventDefault();
                onPage("download");
              }}
            >
              Install from source
            </a>
          </div>
        </section>
      )}
      {tagged && <TaggedRelease />}

      <section className="rd-release-contract rd-shell" aria-labelledby="rd-contract-title">
        <SectionHeading
          eyebrow="Publication contract"
          title="A useful release answers three questions."
          id="rd-contract-title"
        >
          The same contract shapes the empty, loading, unavailable, and released states.
        </SectionHeading>
        <ol>
          <li>
            <span>01</span>
            <h3>What can I install?</h3>
            <p>Only artifacts that completed the release gate become download actions.</p>
          </li>
          <li>
            <span>02</span>
            <h3>What changed?</h3>
            <p>
              Editorial notes describe user-visible behavior without exposing internal planning
              state.
            </p>
          </li>
          <li>
            <span>03</span>
            <h3>Will it attach safely?</h3>
            <p>Protocol and handoff guidance distinguish a live transfer from a restart.</p>
          </li>
        </ol>
      </section>
    </>
  );
}

function TaggedRelease() {
  return (
    <section className="rd-release-list rd-shell" aria-labelledby="rd-version-title">
      <article className="rd-release-entry">
        <header>
          <div>
            <p>{releasePreview.date}</p>
            <h2 id="rd-version-title">Gardn {releasePreview.version}</h2>
          </div>
          <span>latest</span>
        </header>
        <p className="rd-release-summary">{releasePreview.summary}</p>
        <div className="rd-release-notes">
          {releasePreview.features.map((feature) => (
            <div key={feature.title}>
              <span>{feature.category}</span>
              <h3>{feature.title}</h3>
              <p>{feature.summary}</p>
            </div>
          ))}
        </div>
        <div className="rd-assets">
          <h3>Assets</h3>
          <div>
            <span>gardn-macos-aarch64</span>
            <code>sha256 · design fixture</code>
            <button type="button">Download</button>
          </div>
          <div>
            <span>gardn-linux-x86_64</span>
            <code>sha256 · design fixture</code>
            <button type="button">Download</button>
          </div>
          <div>
            <span>View all 5 assets</span>
            <code>checksums.txt</code>
            <button type="button">Expand</button>
          </div>
        </div>
        <footer>
          <span>Protocol 7</span>
          <span>Server/client compatibility documented</span>
          <a href="/docs/guides/updates-and-handoff">Update safely ↗</a>
        </footer>
      </article>
    </section>
  );
}

function PageHero({
  eyebrow,
  title,
  status,
  children,
}: {
  eyebrow: string;
  title: string;
  status: string;
  children: string;
}) {
  return (
    <section className="rd-page-hero rd-shell">
      <div className="rd-page-status">
        <p className="rd-eyebrow">{eyebrow}</p>
        <span>
          <i />
          {status}
        </span>
      </div>
      <h1>{title}</h1>
      <p>{children}</p>
      <div className="rd-actions">
        <a className="rd-button rd-button-primary" href="/docs/getting-started/install">
          Follow the install guide
        </a>
        <a className="rd-button" href="https://github.com/masakirocorp/gardn">
          View source
        </a>
      </div>
    </section>
  );
}

function SectionHeading({
  eyebrow,
  title,
  id,
  children,
}: {
  eyebrow: string;
  title: string;
  id: string;
  children: string;
}) {
  return (
    <header className="rd-section-heading">
      <p className="rd-eyebrow">{eyebrow}</p>
      <h2 id={id}>{title}</h2>
      <p>{children}</p>
    </header>
  );
}

function SiteFooter() {
  return (
    <footer className="rd-footer">
      <a href="https://github.com/masakirocorp/gardn/blob/master/LICENSE">License</a>
      <a href="https://github.com/masakirocorp/gardn">GitHub</a>
      <span>A fork of Herdr</span>
    </footer>
  );
}
