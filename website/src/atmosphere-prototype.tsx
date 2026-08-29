"use client";

import { useEffect, useId, useState } from "react";

type Theme = "light" | "dark";

const PLATES = {
  hero: "/atmosphere/tiles/barnsley.png",
  field: "/atmosphere/fields/meadow.png",
  features: [
    "/atmosphere/tiles/leaves.png",
    "/atmosphere/tiles/lavender.png",
    "/atmosphere/tiles/bark.png",
  ],
  scatter: [
    "/atmosphere/tiles/lichen.png",
    "/atmosphere/tiles/hedge.png",
    "/atmosphere/tiles/seedhead.png",
  ],
} as const;

const FILMS = [
  {
    label: "Groups",
    title: "Filter by group",
    caption: "Open All, pick commerce, keep only that group's spaces.",
    layout: "spread",
    src: "/groups.png",
    srcDark: "/groups-night.png",
    video: "/groups.mp4",
    videoDark: "/groups-night.mp4",
  },
  {
    label: "Agents",
    title: "Filter agents to this group",
    caption: "Open All on Agents, pick Group, keep agents in the current group.",
    src: "/agents.png",
    srcDark: "/agents-night.png",
    video: "/agents.mp4",
    videoDark: "/agents-night.mp4",
  },
  {
    label: "Follow-up",
    title: "Manage follow-up from a row",
    caption: "Right-click an agent to add or remove Follow Up.",
    src: "/follow-up.png",
    srcDark: "/follow-up-night.png",
    video: "/follow-up.mp4",
    videoDark: "/follow-up-night.mp4",
  },
  {
    label: "Navigator",
    title: "Jump through the session",
    caption: "Search groups, spaces, tabs, and panes without leaving the view.",
    layout: "spread",
    flip: true,
    src: "/commands.png",
    srcDark: "/commands-night.png",
    video: "/commands.mp4",
    videoDark: "/commands-night.mp4",
  },
  {
    label: "Rail",
    title: "Read status from the rail",
    caption: "Collapse the sidebar. Hover a compact row to read the space name and state.",
    src: "/collapsed.png",
    srcDark: "/collapsed-night.png",
    video: "/collapsed.mp4",
    videoDark: "/collapsed-night.mp4",
  },
  {
    label: "Triage",
    title: "Jump from Triage",
    caption: "Click a blocked agent to focus that space, tab, and pane.",
    src: "/triage.png",
    srcDark: "/triage-night.png",
    video: "/triage.mp4",
    videoDark: "/triage-night.mp4",
  },
] as const;

const features = [
  {
    kicker: "#1 Spaces",
    title: "Work stays planted",
    body: "Agents, shells, and project context live in persistent spaces. Close the client. The session keeps growing.",
  },
  {
    kicker: "#2 State",
    title: "See the garden at a glance",
    body: "Triage, Follow Up, Working, and Idle sit beside the live panes. You read the row, then jump.",
  },
  {
    kicker: "#3 Return",
    title: "Detach. Reattach.",
    body: "The server owns the processes. Come back to the same spaces, tabs, and panes.",
  },
] as const;

function Logo({ className }: { className?: string }) {
  return (
    <svg className={className} viewBox="0 0 256 256" aria-hidden="true">
      <g fill="none" stroke="currentColor" strokeWidth="6" strokeLinecap="round" strokeLinejoin="round">
        <path d="M128 38 176 72 128 112 80 72Z" />
        <path d="M80 72v68l48 40 48-40V72" />
        <path d="M128 112v68" />
        <path d="M91 170 44 188l84 36 84-36-47-18" />
      </g>
    </svg>
  );
}

function Print({ src, className }: { src: string; className?: string }) {
  return (
    <div
      className={["atmo-print", className].filter(Boolean).join(" ")}
      style={{ ["--print-src" as string]: `url("${src}")` }}
      aria-hidden="true"
    />
  );
}

function Capture({ dark }: { dark: boolean }) {
  const poster = dark ? "/session-night.png" : "/session.png";
  const video = dark ? "/session-night.mp4" : "/session.mp4";
  return (
    <section className="atmo-field" aria-label="Session capture">
      <div
        className="atmo-field-print"
        style={{ ["--print-src" as string]: `url("${PLATES.field}")` }}
      />
      <div className="atmo-field-card">
        <video
          key={video}
          src={video}
          poster={poster}
          width={1440}
          height={912}
          autoPlay
          muted
          loop
          playsInline
          aria-label="Gardn session"
        />
      </div>
    </section>
  );
}

function FilmStage({ dark }: { dark: boolean }) {
  const baseId = useId();
  const [active, setActive] = useState(0);
  const [visible, setVisible] = useState(0);
  const [pending, setPending] = useState<number | null>(null);
  const [ready, setReady] = useState(false);
  const shown = FILMS[visible] ?? FILMS[0];
  const next = pending === null ? null : (FILMS[pending] ?? null);
  if (!shown) {
    return null;
  }

  const select = (index: number) => {
    setActive(index);
    if (index === visible) {
      setPending(null);
      setReady(false);
      return;
    }
    setPending(index);
    setReady(false);
  };

  const commit = () => {
    if (pending === null) {
      return;
    }
    setVisible(pending);
    setPending(null);
    setReady(false);
  };

  const shownPoster = dark ? shown.srcDark : shown.src;
  const shownVideo = dark ? shown.videoDark : shown.video;

  return (
    <section className="atmo-stage" aria-label="Session films">
      <div className="atmo-stage-media">
        <video
          key={shownVideo}
          src={shownVideo}
          poster={shownPoster}
          width={1440}
          height={912}
          autoPlay
          muted
          loop
          playsInline
          aria-label={shown.title}
        />
        {next ? (
          <video
            key={dark ? next.videoDark : next.video}
            className={ready ? "atmo-film-next is-ready" : "atmo-film-next"}
            src={dark ? next.videoDark : next.video}
            poster={dark ? next.srcDark : next.src}
            width={1440}
            height={912}
            autoPlay
            muted
            loop
            playsInline
            aria-hidden="true"
            onPlaying={() => {
              if (window.matchMedia("(prefers-reduced-motion: reduce)").matches) {
                commit();
                return;
              }
              setReady(true);
            }}
            onError={commit}
            onTransitionEnd={(event) => {
              if (event.propertyName === "opacity") {
                commit();
              }
            }}
          />
        ) : null}
      </div>
      <div className="atmo-stage-list">
        {FILMS.map((item, index) => {
          const open = index === active;
          const panelId = `${baseId}-panel-${index}`;
          const headerId = `${baseId}-header-${index}`;
          return (
            <div key={item.label} className="atmo-acc" data-open={open ? "true" : "false"}>
              <h3>
                <button
                  type="button"
                  id={headerId}
                  aria-expanded={open}
                  aria-controls={panelId}
                  onClick={() => select(index)}
                >
                  {item.label}
                </button>
              </h3>
              <div className="atmo-acc-panel" id={panelId} role="region" aria-labelledby={headerId}>
                <div>
                  <strong>{item.title}</strong>
                  <p>{item.caption}</p>
                </div>
              </div>
            </div>
          );
        })}
      </div>
    </section>
  );
}

function FeatureRow({
  feature,
  art,
  flip,
}: {
  feature: (typeof features)[number];
  art: string;
  flip: boolean;
}) {
  return (
    <article className="atmo-feature" data-flip={flip ? "true" : "false"}>
      <div className="atmo-feature-copy">
        <p className="atmo-kicker">{feature.kicker}</p>
        <h2>{feature.title}</h2>
        <p className="atmo-feature-body">{feature.body}</p>
      </div>
      <Print src={art} />
    </article>
  );
}

export function AtmospherePrototype() {
  const [theme, setTheme] = useState<Theme>("light");
  const dark = theme === "dark";

  useEffect(() => {
    const requested = new URLSearchParams(window.location.search).get("theme");
    if (requested === "dark" || requested === "light") {
      setTheme(requested);
    } else if (window.matchMedia("(prefers-color-scheme: dark)").matches) {
      setTheme("dark");
    }
  }, []);

  const selectTheme = (next: Theme) => {
    setTheme(next);
    const url = new URL(window.location.href);
    url.searchParams.set("theme", next);
    window.history.replaceState({}, "", url);
  };

  return (
    <div className="atmo-root" data-theme={theme}>
      <header className="atmo-nav">
        <a className="atmo-brand" href="/" aria-label="Gardn">
          <Logo className="atmo-mark" />
          GARDN
        </a>
        <nav className="atmo-nav-links" aria-label="Site">
          <a href="/docs">Docs</a>
          <a href="/docs/getting-started/install">Install</a>
          <button
            type="button"
            className="atmo-theme"
            aria-pressed={dark}
            aria-label={dark ? "Switch to light appearance" : "Switch to dark appearance"}
            onClick={() => selectTheme(dark ? "light" : "dark")}
          >
            {dark ? "Light" : "Dark"}
          </button>
        </nav>
      </header>
      <section className="atmo-hero">
        <Logo className="atmo-hero-wash" />
        <div className="atmo-hero-copy">
          <p className="atmo-kicker">Terminal workspace manager</p>
          <h1>Keep the terminal work. Lose the terminal sprawl.</h1>
          <p className="atmo-lede">
            Agents, shells, and project context live in a session that survives disconnects.
          </p>
          <div className="atmo-cta">
            <a className="atmo-btn" data-primary="true" href="/docs">
              Read the documentation
            </a>
          </div>
        </div>
        <div className="atmo-hero-plate">
          <Print className="atmo-hero-print" src={PLATES.hero} />
          <Logo className="atmo-seal" />
        </div>
      </section>
      <Capture dark={dark} />
      <FilmStage dark={dark} />
      <section className="atmo-features">
        {features.map((feature, index) => {
          const art = PLATES.features[index];
          return art ? (
            <FeatureRow key={feature.kicker} feature={feature} art={art} flip={index % 2 === 1} />
          ) : null;
        })}
      </section>
      <section className="atmo-scatter">
        {PLATES.scatter.map((src) => (
          <Print key={src} src={src} />
        ))}
      </section>
      <section className="atmo-end">
        <p className="atmo-wordmark atmo-wordmark-end">
          <Logo className="atmo-wordmark-mark" />
          GARDN
        </p>
        <h2>One session. Every agent.</h2>
        <div className="atmo-cta">
          <a className="atmo-btn" data-primary="true" href="/docs">
            Start in the docs
          </a>
        </div>
      </section>
    </div>
  );
}
