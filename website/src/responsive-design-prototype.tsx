"use client";

import { useEffect, useState } from "react";
import {
  operatingModules,
  platformRows,
  releasePreview,
  type OperatingModule,
  type RecentFeature,
} from "./responsive-design-data";
import { MiniLifecycle } from "./visual-direction-prototype";

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
    <div className="rd-root" data-page={page} data-release-state={releaseState} data-theme={theme}>
      <a className="rd-skip" href="#rd-main">
        Skip to content
      </a>
      <PrototypeBar
        releaseState={releaseState}
        theme={theme}
        onState={selectState}
        onTheme={(nextTheme) => {
          setTheme(nextTheme);
          const url = new URL(window.location.href);
          url.searchParams.set("page", page);
          url.searchParams.set("state", releaseState);
          url.searchParams.set("theme", nextTheme);
          window.history.replaceState({}, "", url);
        }}
      />
      <SiteHeader page={page} onPage={selectPage} />
      <main id="rd-main">
        {page === "home" && (
          <HomePage releaseState={releaseState} onPage={selectPage} onState={selectState} />
        )}
        {page === "download" && (
          <DownloadPage releaseState={releaseState} onPage={selectPage} onState={selectState} />
        )}
        {page === "releases" && (
          <ReleasesPage releaseState={releaseState} onPage={selectPage} onState={selectState} />
        )}
      </main>
      <SiteFooter onPage={selectPage} />
    </div>
  );
}

function GardnProductSurface() {
  return (
    <div className="rd-surface" aria-label="Gardn product workspace proof">
      <div className="rd-surface-topbar">
        <span className="rd-surface-brand">GARDN / atlas</span>
        <span>workspace: atlas</span>
        <span className="rd-surface-live">
          <i /> live
        </span>
      </div>
      <div className="rd-surface-body">
        <aside className="rd-surface-sidebar" aria-label="Workspace and agent state">
          <p className="rd-surface-label">Projects</p>
          <div className="rd-surface-project is-active">
            <span className="rd-surface-project-mark">A</span>
            <span>
              <b>atlas</b>
              <small>3 agents · active</small>
            </span>
          </div>
          <div className="rd-surface-project">
            <span className="rd-surface-project-mark">O</span>
            <span>
              <b>orbit</b>
              <small>1 agent · ready</small>
            </span>
          </div>
          <p className="rd-surface-label rd-surface-label-agents">Agent state</p>
          <div className="rd-surface-agent">
            <i className="is-working" />
            <span>codex</span>
            <em>working</em>
          </div>
          <div className="rd-surface-agent">
            <i className="is-blocked" />
            <span>claude</span>
            <em>blocked</em>
          </div>
          <div className="rd-surface-agent">
            <i className="is-idle" />
            <span>pi</span>
            <em>idle</em>
          </div>
        </aside>
        <div className="rd-surface-workspace">
          <div className="rd-surface-palette">
            <span>⌘K</span>
            <b>Project palette</b>
            <small>run managed action</small>
          </div>
          <div className="rd-surface-terminal rd-surface-terminal-primary">
            <div className="rd-surface-terminal-title">
              <span>codex · api</span>
              <span>working</span>
            </div>
            <pre>
              <span className="dim">$</span> gardn project triage{"\n"}
              <span className="accent">◆</span> loading atlas policy{"\n"}
              <span className="accent">◆</span> follow-up queue ready{"\n"}
              <span className="dim"> 3 agents · 2 awaiting input</span>
              {"\n"}
              <span className="cursor">▋</span>
            </pre>
          </div>
          <div className="rd-surface-terminal">
            <div className="rd-surface-terminal-title">
              <span>shell · local</span>
              <span className="ok">ready</span>
            </div>
            <pre>
              <span className="dim">$</span> git diff --stat{"\n"}
              <span className="ok">ready</span> 4 files changed
            </pre>
          </div>
          <div className="rd-surface-terminal">
            <div className="rd-surface-terminal-title">
              <span>agent · review</span>
              <span className="warn">blocked</span>
            </div>
            <pre>
              <span className="warn">!</span> permission needed{"\n"}
              <span className="dim">awaiting explicit approval</span>
            </pre>
          </div>
        </div>
      </div>
      <div className="rd-surface-status">
        <span>3 projects</span>
        <span>17 profiles</span>
        <span>local + SSH</span>
        <span>client-private view</span>
      </div>
    </div>
  );
}

function PrototypeBar({
  releaseState,
  theme,
  onState,
  onTheme,
}: {
  releaseState: ReleaseState;
  theme: ThemeName;
  onState: (state: ReleaseState) => void;
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
      <label className="rd-state-control">
        <span>Release state</span>
        <select
          value={releaseState}
          onChange={(event) => {
            const nextState = event.target.value;
            if (isReleaseState(nextState)) onState(nextState);
          }}
        >
          <option value="prepublic">Pre-public</option>
          <option value="tagged">Tagged preview</option>
          <option value="loading">Loading</option>
          <option value="error">Error</option>
        </select>
      </label>
    </aside>
  );
}

function SiteHeader({ page, onPage }: { page: PageName; onPage: (page: PageName) => void }) {
  const [menuOpen, setMenuOpen] = useState(false);

  const openHomeSection = (event: React.MouseEvent<HTMLAnchorElement>, sectionId: string) => {
    event.preventDefault();
    onPage("home");
    setMenuOpen(false);
    const url = new URL(window.location.href);
    url.hash = sectionId;
    window.history.replaceState({}, "", url);
    window.requestAnimationFrame(() => {
      document.getElementById(sectionId)?.scrollIntoView({ behavior: "smooth" });
    });
  };

  const openPage = (event: React.MouseEvent<HTMLAnchorElement>, nextPage: PageName) => {
    event.preventDefault();
    onPage(nextPage);
    setMenuOpen(false);
  };

  return (
    <header className="rd-header">
      <a className="rd-brand" href="?page=home" onClick={(event) => openPage(event, "home")}>
        <BrandMark />
        <span>Gardn</span>
      </a>
      <button
        className="rd-nav-toggle"
        type="button"
        aria-controls="rd-primary-navigation"
        aria-expanded={menuOpen}
        onClick={() => setMenuOpen((open) => !open)}
      >
        Menu
      </button>
      <nav
        id="rd-primary-navigation"
        className={`rd-nav${menuOpen ? " is-open" : ""}`}
        aria-label="Primary navigation"
      >
        <a href="?page=home#product" onClick={(event) => openHomeSection(event, "product")}>
          Product
        </a>
        <a href="/docs" onClick={() => setMenuOpen(false)}>
          Documentation
        </a>
        <a
          href="?page=releases"
          aria-current={page === "releases" ? "page" : undefined}
          onClick={(event) => openPage(event, "releases")}
        >
          Releases
        </a>
        <a href="https://github.com/masakirocorp/gardn" onClick={() => setMenuOpen(false)}>
          GitHub
        </a>
      </nav>
      <a
        className="rd-header-action"
        href="?page=download"
        onClick={(event) => openPage(event, "download")}
      >
        Install <span aria-hidden="true">↗</span>
      </a>
    </header>
  );
}

function HomePage({
  releaseState,
  onPage,
  onState,
}: {
  releaseState: ReleaseState;
  onPage: (page: PageName) => void;
  onState: (state: ReleaseState) => void;
}) {
  return (
    <>
      <section className="rd-hero rd-shell" aria-labelledby="rd-home-title">
        <div className="rd-hero-copy">
          <p className="rd-eyebrow">
            <span>Terminal workspace manager</span>
            <span className="rd-rule" />
          </p>
          <h1 id="rd-home-title">Run the whole coding operation.</h1>
          <p className="rd-hero-promise">It is still just your terminal.</p>
          <p className="rd-lede">
            Keep agents, shells, servers, repositories, and project commands in one durable
            workspace, without replacing the terminal tools you already trust.
          </p>
          <div className="rd-actions">
            <a
              className="rd-button rd-button-primary"
              href="?page=download"
              onClick={(event) => {
                event.preventDefault();
                onPage("download");
              }}
            >
              Install Gardn
            </a>
            <a className="rd-button" href="#product">
              Watch the workflow
            </a>
          </div>
          <a className="rd-text-link rd-hero-docs" href="/docs">
            Read the documentation <span>↗</span>
          </a>
          <p className="rd-availability">
            <span className="rd-signal" /> Source install available · public binaries gated
          </p>
          <p className="rd-status-sentence">
            More accurate agent status than Herdr. Gardn stays Working through compaction and the
            first prompt. Permission prompts show Blocked. Live hooks update Working, Blocked, and
            Idle. A resume report keeps the session instead of dropping the agent.
          </p>
        </div>
        <div className="rd-hero-art" aria-label="Gardn mark">
          <BrandMark />
          <span className="rd-coordinate">workspace atlas · 4 live operations</span>
        </div>
        <div className="rd-hero-product">
          <GardnProductSurface />
        </div>
      </section>

      <section id="product" className="rd-product rd-shell" aria-labelledby="rd-product-title">
        <SectionHeading
          eyebrow="Product demonstration"
          title="Launch, operate, return."
          id="rd-product-title"
        >
          Launch a workspace, start agents, run a project action from the palette, and detach
          without losing the work. Every pane remains a real terminal.
        </SectionHeading>
        <div className="rd-product-stage">
          <GardnProductSurface />
        </div>
        <MiniLifecycle mode="a" />
        <div className="rd-product-meta" aria-label="Product operation details">
          <span>
            <b>17</b> profiles
          </span>
          <span>
            <b>04</b> project roles
          </span>
          <span>
            <b>SSH</b> + local
          </span>
          <span>
            <b>01</b> private view
          </span>
        </div>
      </section>

      <section className="rd-freedom rd-shell" aria-labelledby="rd-freedom-title">
        <p className="rd-vertical-label" aria-hidden="true">
          TOOLS STAY YOURS
        </p>
        <div>
          <p className="rd-eyebrow">Still your terminal</p>
          <h2 id="rd-freedom-title">Native panes stay the work surface.</h2>
          <p>
            Run Codex, Claude, a shell, a server, an editor, or the next tool you adopt. Gardn adds
            status and restore context without gating what can run.
          </p>
        </div>
        <ul className="rd-tool-list" aria-label="Example terminal tools">
          <li>
            <span>agents</span>
            <b>Codex · Claude · any CLI</b>
          </li>
          <li>
            <span>shells</span>
            <b>zsh · bash · fish · PowerShell</b>
          </li>
          <li>
            <span>services</span>
            <b>dev servers · databases · tunnels</b>
          </li>
          <li>
            <span>editors</span>
            <b>vim · helix · your terminal editor</b>
          </li>
        </ul>
      </section>

      <section className="rd-operating rd-shell" aria-labelledby="rd-operating-title">
        <SectionHeading
          eyebrow="Operating layer"
          title="A clearer loop for every project."
          id="rd-operating-title"
        >
          Four modules make the surrounding work legible while the terminal remains the place where
          work happens.
        </SectionHeading>
        <div className="rd-operating-grid">
          {operatingModules.map((module) => (
            <OperatingModuleCard key={module.number} module={module} />
          ))}
        </div>
      </section>

      <section className="rd-persistence rd-shell" aria-labelledby="rd-persistence-title">
        <div className="rd-persistence-copy">
          <p className="rd-eyebrow">Durable session lifecycle</p>
          <h2 id="rd-persistence-title">The session owns the work. Clients are views.</h2>
          <p>
            Disconnecting a terminal does not dismantle the workspace. The session retains panes,
            process runtimes, layouts, and agent state while each attached client keeps its own
            navigation and viewport.
          </p>
          <a className="rd-text-link" href="/docs/concepts">
            Read the session model <span>↗</span>
          </a>
        </div>
        <div
          className="rd-session-diagram"
          role="img"
          aria-label="One persistent session serving three independent clients"
        >
          <div className="rd-session-core">
            <span>persistent session</span>
            <b>workspaces · tabs · panes · runtimes</b>
          </div>
          <div className="rd-session-line" />
          <div className="rd-client-row">
            <span>local client</span>
            <span>SSH client</span>
            <span>API client</span>
          </div>
        </div>
      </section>

      <section id="compare" className="rd-migration rd-shell" aria-labelledby="rd-migration-title">
        <div>
          <p className="rd-eyebrow">Herdr migration</p>
          <h2 id="rd-migration-title">Familiar workflow. Independent roadmap.</h2>
        </div>
        <div className="rd-migration-body">
          <p>
            Gardn credits Herdr upstream and keeps its own release process, documentation, and
            product direction. Review the differences, then install Gardn alongside an existing
            setup. Nothing silently changes.
          </p>
          <div className="rd-migration-path" aria-label="Herdr migration path">
            <span>Herdr workspace</span>
            <i>→</i>
            <span>review differences</span>
            <i>→</i>
            <span>Gardn workspace</span>
          </div>
          <a className="rd-text-link" href="/docs/guides/migrate-from-herdr">
            Open the migration guide <span>↗</span>
          </a>
        </div>
      </section>

      <RecentSection releaseState={releaseState} onState={onState} />

      <section className="rd-install rd-shell" aria-labelledby="rd-install-title">
        <div>
          <p className="rd-eyebrow">Ready when you are</p>
          <h2 id="rd-install-title">Keep the operation together.</h2>
        </div>
        <div className="rd-install-command">
          <code>cargo install --path apps/gardn</code>
          <button type="button" aria-label="Copy install command">
            copy
          </button>
        </div>
        <div className="rd-actions">
          <a
            className="rd-button rd-button-primary"
            href="?page=download"
            onClick={(event) => {
              event.preventDefault();
              onPage("download");
            }}
          >
            Install Gardn
          </a>
          <a className="rd-button" href="/docs">
            Read the documentation
          </a>
        </div>
      </section>
    </>
  );
}

function OperatingModuleCard({ module }: { module: OperatingModule }) {
  return (
    <article className="rd-operating-card">
      <span>{module.number}</span>
      <h3>{module.title}</h3>
      <strong>{module.evidence}</strong>
      <p>{module.summary}</p>
    </article>
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

function DownloadPage({
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
        eyebrow="Install Gardn"
        title={
          tagged
            ? "Choose the binary for your machine."
            : "Start from source. Downloads stay gated."
        }
        status={tagged ? "Release verified" : "Public binaries in verification"}
      >
        {tagged
          ? "Tagged-preview state: verified artifacts appear only after platform, checksum, and release metadata agree."
          : "Build the current source or use the Nix flake today. Binary controls remain unavailable until the release gate passes."}
      </PageHero>

      <section className="rd-download-options rd-shell" aria-labelledby="rd-install-options-title">
        <SectionHeading
          eyebrow="Available now"
          title="Two source-backed paths."
          id="rd-install-options-title"
        >
          Install snippets remain legible at narrow widths and scroll inside their own boundary instead
          of overflowing the page.
        </SectionHeading>
        <div className="rd-option-grid">
          <InstallOption
            index="01"
            title="Build with Cargo"
            copy="Clone the repository and install the workspace binary from its package."
            command={
              "git clone https://github.com/masakirocorp/gardn.git\ncd gardn\ncargo install --path apps/gardn"
            }
          />
          <InstallOption
            index="02"
            title="Install with Nix"
            copy="Use the repository flake on x86_64 or aarch64 Linux and macOS."
            command={'nix profile install "github:masakirocorp/gardn#gardn"'}
          />
        </div>
      </section>

      <section className="rd-binary-gate rd-shell" aria-labelledby="rd-binary-title">
        <div>
          <p className="rd-eyebrow">Release gate</p>
          <h2 id="rd-binary-title">No button before its binary.</h2>
        </div>
        <ReleaseControl state={releaseState} onRetry={() => onState("prepublic")} />
      </section>

      <section className="rd-platforms rd-shell" aria-labelledby="rd-platform-title">
        <SectionHeading
          eyebrow="Compatibility"
          title="Know the boundary before install."
          id="rd-platform-title"
        >
          The desktop table becomes labeled records on narrow screens; no horizontal page scroll is
          required.
        </SectionHeading>
        <div className="rd-platform-table" role="table" aria-label="Supported platforms">
          <div className="rd-platform-row rd-platform-head" role="row">
            <span role="columnheader">Platform</span>
            <span role="columnheader">Architectures</span>
            <span role="columnheader">Role</span>
            <span role="columnheader">Status</span>
          </div>
          {platformRows.map((row) => (
            <div className="rd-platform-row" role="row" key={row.platform}>
              <span role="cell" data-label="Platform">
                <b>{row.platform}</b>
              </span>
              <span role="cell" data-label="Architectures">
                {row.architectures}
              </span>
              <span role="cell" data-label="Role">
                {row.role}
              </span>
              <span role="cell" data-label="Status">
                <i className="rd-check" /> supported
              </span>
            </div>
          ))}
        </div>
        <div className="rd-actions">
          <a className="rd-button rd-button-primary" href="/docs/getting-started/install">
            Open install guide
          </a>
          <a
            className="rd-button"
            href="?page=releases"
            onClick={(event) => {
              event.preventDefault();
              onPage("releases");
            }}
          >
            Release status
          </a>
        </div>
      </section>
    </>
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

function SiteFooter({ onPage }: { onPage: (page: PageName) => void }) {
  const openMigration = (event: React.MouseEvent<HTMLAnchorElement>) => {
    event.preventDefault();
    onPage("home");
    const url = new URL(window.location.href);
    url.hash = "compare";
    window.history.replaceState({}, "", url);
    window.requestAnimationFrame(() => {
      document.getElementById("compare")?.scrollIntoView({ behavior: "smooth" });
    });
  };

  return (
    <footer className="rd-footer">
      <div>
        <a
          className="rd-brand"
          href="?page=home"
          onClick={(event) => {
            event.preventDefault();
            onPage("home");
          }}
        >
          <BrandMark />
          <span>Gardn</span>
        </a>
        <p>Terminal workspace management for AI coding agents.</p>
      </div>
      <nav aria-label="Footer navigation">
        <div>
          <b>Product</b>
          <a
            href="?page=download"
            onClick={(event) => {
              event.preventDefault();
              onPage("download");
            }}
          >
            Download
          </a>
          <a
            href="?page=releases"
            onClick={(event) => {
              event.preventDefault();
              onPage("releases");
            }}
          >
            Releases
          </a>
          <a href="?page=home#compare" onClick={openMigration}>
            Herdr migration
          </a>
        </div>
        <div>
          <b>Learn</b>
          <a href="/docs/getting-started/quick-start">Quick start</a>
          <a href="/docs">Documentation</a>
          <a href="/docs/reference/platforms">Platform support</a>
          <a href="/docs/api">Local API</a>
        </div>
        <div>
          <b>Project</b>
          <a href="https://github.com/masakirocorp/gardn">GitHub</a>
          <a href="https://github.com/masakirocorp/gardn/blob/master/LICENSE">License</a>
        </div>
      </nav>
      <p className="rd-footer-meta">AGPL-3.0-or-later · credited fork of Herdr</p>
    </footer>
  );
}
