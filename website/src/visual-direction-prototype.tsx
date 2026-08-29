"use client";

import { useEffect, useState } from "react";

type Variant = "a" | "b" | "c";

const variants: Array<{ id: Variant; name: string; note: string }> = [
  {
    id: "a",
    name: "Dithered Operator",
    note: "Atmospheric, tactile, product-led",
  },
  {
    id: "b",
    name: "Control Surface",
    note: "Precise, dense, operational",
  },
  {
    id: "c",
    name: "Signal Print",
    note: "Editorial, assertive, human",
  },
];

const workflow = [
  [
    "01",
    "Prepare",
    "Open a project workspace with its profiles, commands, and context already in place.",
  ],
  ["02", "Launch", "Start agents, shells, and servers without rebuilding the setup by hand."],
  ["03", "Operate", "See what is working, blocked, listening, or complete across every project."],
  [
    "04",
    "Return",
    "Detach, reconnect, and keep the operation intact without taking over another view.",
  ],
] as const;

const capabilities = [
  ["Repeatable", "Profiles turn agent launch setup into something you can run again."],
  ["Legible", "Groups and live state keep many projects understandable at a glance."],
  ["Operational", "Commands, Git context, ports, and plugins stay beside the work."],
  ["Durable", "Persistent sessions keep running when the client disconnects."],
] as const;

export function ProductSurface({ mode }: { mode: Variant }) {
  return (
    <div
      className={`vdp-product vdp-product--${mode}`}
      aria-label="Provisional Gardn product composition"
    >
      <div className="vdp-product__topbar">
        <span className="vdp-product__brand">Gardn / atlas</span>
        <span>session: main</span>
        <span className="vdp-live">
          <i /> live
        </span>
      </div>
      <div className="vdp-product__body">
        <aside className="vdp-product__sidebar">
          <p className="vdp-product__label">Workspaces</p>
          <div className="vdp-project is-active">
            <span className="vdp-project__mark">A</span>
            <span>
              <b>atlas</b>
              <small>3 agents · 2 ports</small>
            </span>
          </div>
          <div className="vdp-project">
            <span className="vdp-project__mark">O</span>
            <span>
              <b>orbit</b>
              <small>1 agent · clean</small>
            </span>
          </div>
          <div className="vdp-project">
            <span className="vdp-project__mark">S</span>
            <span>
              <b>signal</b>
              <small>2 agents · 1 blocked</small>
            </span>
          </div>
          <p className="vdp-product__label vdp-product__label--agents">Agents</p>
          <div className="vdp-agent">
            <i className="is-working" />
            <span>codex</span>
            <em>working</em>
          </div>
          <div className="vdp-agent">
            <i className="is-done" />
            <span>claude</span>
            <em>done</em>
          </div>
          <div className="vdp-agent">
            <i className="is-blocked" />
            <span>pi</span>
            <em>blocked</em>
          </div>
        </aside>
        <div className="vdp-terminal-grid">
          <div className="vdp-terminal vdp-terminal--primary">
            <div className="vdp-terminal__title">
              <span>codex · api</span>
              <span>working</span>
            </div>
            <pre>
              <span className="dim">$</span> codex resume atlas-api{"\n"}
              <span className="accent">◆</span> reading workspace state{"\n"}
              <span className="accent">◆</span> updating release manifest{"\n"}
              <span className="dim"> src/release/manifest.rs +42 -8</span>
              {"\n"}
              <span className="cursor">▋</span>
            </pre>
          </div>
          <div className="vdp-terminal">
            <div className="vdp-terminal__title">
              <span>server · dev</span>
              <span className="ok">ready</span>
            </div>
            <pre>
              <span className="dim">$</span> pnpm dev{"\n"}
              <span className="ok">ready</span> http://localhost:3000{"\n"}
              <span className="dim">network</span> 192.168.1.12:3000
            </pre>
          </div>
          <div className="vdp-terminal">
            <div className="vdp-terminal__title">
              <span>git · atlas</span>
              <span>main</span>
            </div>
            <pre>
              <span className="accent">M</span> release/manifest.rs{"\n"}
              <span className="accent">M</span> website/download.tsx{"\n"}
              <span className="dim">branch</span> eng-127-visual
            </pre>
          </div>
        </div>
      </div>
      <div className="vdp-product__status">
        <span>3 projects</span>
        <span>6 agents</span>
        <span>2 ports</span>
        <span>client 01 / independent view</span>
      </div>
    </div>
  );
}

export function DitherOrb() {
  return (
    <svg className="vdp-orb" viewBox="0 0 640 640" aria-hidden="true">
      <defs>
        <radialGradient id="orb-color" cx="30%" cy="25%" r="85%">
          <stop offset="0" stopColor="#fff1c7" />
          <stop offset="0.28" stopColor="#ff7149" />
          <stop offset="0.68" stopColor="#d8ee57" />
          <stop offset="1" stopColor="#37b9de" />
        </radialGradient>
        <pattern id="orb-dots" width="9" height="9" patternUnits="userSpaceOnUse">
          <circle cx="2.5" cy="2.5" r="2.2" fill="white" />
        </pattern>
        <mask id="orb-mask">
          <circle cx="320" cy="320" r="288" fill="url(#orb-dots)" />
        </mask>
        <filter id="orb-warp">
          <feTurbulence
            type="fractalNoise"
            baseFrequency="0.008 0.02"
            numOctaves="2"
            seed="8"
            result="noise"
          />
          <feDisplacementMap in="SourceGraphic" in2="noise" scale="22" />
        </filter>
      </defs>
      <circle
        cx="320"
        cy="320"
        r="294"
        fill="url(#orb-color)"
        mask="url(#orb-mask)"
        filter="url(#orb-warp)"
      />
    </svg>
  );
}

export function MiniLifecycle({ mode }: { mode: Variant }) {
  return (
    <div className={`vdp-lifecycle vdp-lifecycle--${mode}`}>
      <span>launch</span>
      <i>→</i>
      <span>work</span>
      <i>→</i>
      <span>detach</span>
      <i>→</i>
      <span>reattach</span>
    </div>
  );
}

function SiteHeader({ label, mode }: { label: string; mode: Variant }) {
  return (
    <header className={`vdp-header vdp-header--${mode}`}>
      <a className="vdp-logo" href="#top" aria-label="Gardn prototype home">
        <img src="/logo.svg" alt="" />
        <span>Gardn</span>
      </a>
      <nav aria-label={`${label} concept navigation`}>
        <a href="#product">Product</a>
        <a href="#workflow">Workflow</a>
        <a href="#docs">Docs</a>
      </nav>
      <a className="vdp-header__install" href="#install">
        Install
      </a>
    </header>
  );
}

function ConceptA() {
  return (
    <article className="vdp-concept concept-a" id="top">
      <SiteHeader mode="a" label="Dithered Operator" />
      <main>
        <section className="a-hero">
          <div className="a-hero__copy">
            <p className="vdp-kicker">Terminal workspace management for AI coding agents</p>
            <h1>Run the whole coding operation.</h1>
            <p className="a-hero__promise">It is still just your terminal.</p>
            <p className="vdp-lede">
              Keep agents, shells, servers, and project context visible in persistent workspaces
              that survive disconnects.
            </p>
            <div className="vdp-actions">
              <a className="vdp-button is-primary" href="#install">
                Install Gardn
              </a>
              <a className="vdp-button" href="#product">
                Watch the workflow
              </a>
            </div>
            <div className="vdp-release">
              <span>
                <i /> v0.2.1 verified
              </span>
              <span>macOS · Linux · Windows</span>
            </div>
          </div>
          <div className="a-hero__art">
            <DitherOrb />
            <span className="a-hero__coordinate">36.7185° N / 4 active operations</span>
          </div>
          <div className="a-hero__product">
            <ProductSurface mode="a" />
          </div>
        </section>

        <section className="a-proof" id="product">
          <div className="vdp-section-heading">
            <p>01 / The product</p>
            <h2>One place to see the work move.</h2>
            <span>A real product capture will replace this provisional composition.</span>
          </div>
          <div className="a-proof__grid">
            <div className="a-proof__visual">
              <div className="a-proof__noise" />
              <ProductSurface mode="a" />
            </div>
            <ol>
              {workflow.map(([number, title, description]) => (
                <li key={number}>
                  <b>{number}</b>
                  <div>
                    <h3>{title}</h3>
                    <p>{description}</p>
                  </div>
                </li>
              ))}
            </ol>
          </div>
        </section>

        <section className="a-capabilities" id="workflow">
          <div className="vdp-section-heading">
            <p>02 / The operating layer</p>
            <h2>More than panes. Less than a platform that owns your tools.</h2>
          </div>
          <div className="a-capabilities__grid">
            {capabilities.map(([title, description], index) => (
              <article key={title}>
                <span>0{index + 1}</span>
                <h3>{title}</h3>
                <p>{description}</p>
              </article>
            ))}
          </div>
          <MiniLifecycle mode="a" />
        </section>

        <section className="a-install" id="install">
          <DitherOrb />
          <div>
            <p className="vdp-kicker">Current release / v0.2.1</p>
            <h2>Bring the whole operation with you.</h2>
            <p>Five verified builds. One durable workspace.</p>
            <div className="vdp-actions">
              <a className="vdp-button is-primary" href="/download">
                Install for macOS
              </a>
              <a className="vdp-button" href="/docs/getting-started/quick-start">
                Open quick start
              </a>
            </div>
          </div>
        </section>
      </main>
    </article>
  );
}

function ConceptB() {
  return (
    <article className="vdp-concept concept-b" id="top">
      <SiteHeader mode="b" label="Control Surface" />
      <main>
        <section className="b-hero">
          <div className="b-hero__index">
            <span>GARDN::01</span>
            <span>persistent operations</span>
          </div>
          <p className="vdp-kicker">Your terminal operation / observable and intact</p>
          <h1>
            Run everything.
            <br />
            <em>Lose nothing.</em>
          </h1>
          <div className="b-hero__bottom">
            <p>
              Agents, shells, servers, repositories, and state—organized in one terminal workspace
              without replacing the tools that do the work.
            </p>
            <div className="vdp-actions">
              <a className="vdp-button is-primary" href="#install">
                Install
              </a>
              <a className="vdp-button" href="#product">
                Inspect the system
              </a>
            </div>
          </div>
          <div className="b-telemetry" aria-label="Operational telemetry sample">
            <span>
              <b>06</b> agents
            </span>
            <span>
              <b>03</b> projects
            </span>
            <span>
              <b>02</b> listening
            </span>
            <span>
              <b>01</b> blocked
            </span>
            <span>
              <b>24h</b> session
            </span>
          </div>
          <div className="b-hero__product" id="product">
            <ProductSurface mode="b" />
          </div>
        </section>

        <section className="b-workflow" id="workflow">
          <header>
            <span>SYS / WORKFLOW</span>
            <h2>A complete loop, not a pile of terminals.</h2>
            <p>Every stage exposes the information needed for the next decision.</p>
          </header>
          <div className="b-workflow__rail">
            {workflow.map(([number, title, description]) => (
              <article key={number}>
                <div className="b-node">
                  <span>{number}</span>
                  <i />
                </div>
                <h3>{title}</h3>
                <p>{description}</p>
              </article>
            ))}
          </div>
        </section>

        <section className="b-capabilities">
          <div className="b-capabilities__intro">
            <span>SYS / CAPABILITIES</span>
            <h2>Operational context belongs beside the process.</h2>
            <MiniLifecycle mode="b" />
          </div>
          <div className="b-capabilities__list">
            {capabilities.map(([title, description], index) => (
              <article key={title}>
                <b>0{index + 1}</b>
                <h3>{title}</h3>
                <p>{description}</p>
                <span className="b-status">ACTIVE</span>
              </article>
            ))}
          </div>
        </section>

        <section className="b-install" id="install">
          <div className="b-install__status">
            <i />
            <span>RELEASE CHANNEL</span>
            <strong>STABLE</strong>
          </div>
          <div>
            <p>v0.2.1 / protocol 12 / schema 1</p>
            <h2>Attach to the operation.</h2>
          </div>
          <div className="vdp-actions">
            <a className="vdp-button is-primary" href="/download">
              Download verified build
            </a>
            <a className="vdp-button" href="/releases">
              Release evidence
            </a>
          </div>
        </section>
      </main>
    </article>
  );
}

function PrintField() {
  return (
    <div className="c-print-field" aria-hidden="true">
      <div className="c-print-field__sun" />
      <span>YOUR TERMINAL / YOUR AGENTS / YOUR OPERATION</span>
    </div>
  );
}

function ConceptC() {
  return (
    <article className="vdp-concept concept-c" id="top">
      <SiteHeader mode="c" label="Signal Print" />
      <main>
        <section className="c-hero">
          <p className="c-folio">
            ISSUE № 001 <span>MASAKIRO / 2026</span>
          </p>
          <div className="c-hero__headline">
            <p className="vdp-kicker">Terminal workspace manager for AI coding agents</p>
            <h1>
              The whole operation.
              <br />
              <em>Still your terminal.</em>
            </h1>
          </div>
          <div className="c-hero__copy">
            <p>
              Coordinate agents, projects, shells, and servers without surrendering the terminal
              tools you chose.
            </p>
            <div className="vdp-actions">
              <a className="vdp-button is-primary" href="#install">
                Install now
              </a>
              <a className="vdp-button" href="#product">
                See the field report
              </a>
            </div>
          </div>
          <div className="c-hero__art">
            <PrintField />
          </div>
          <div className="c-hero__product" id="product">
            <ProductSurface mode="c" />
            <span className="c-caption">
              Fig. 01 — Provisional multi-project session composition
            </span>
          </div>
        </section>

        <section className="c-manifesto">
          <p>Not another wrapper around your agents.</p>
          <h2>
            Gardn gives the work a durable shape while every process remains exactly what it is.
          </h2>
          <div>
            <span>Native terminal panes</span>
            <span>Independent client views</span>
            <span>Persistent sessions</span>
          </div>
        </section>

        <section className="c-workflow" id="workflow">
          <header>
            <p>FIELD NOTES / 01–04</p>
            <h2>A working day, held together.</h2>
          </header>
          <div className="c-workflow__grid">
            {workflow.map(([number, title, description]) => (
              <article key={number}>
                <b>{number}</b>
                <h3>{title}</h3>
                <p>{description}</p>
              </article>
            ))}
          </div>
          <PrintField />
        </section>

        <section className="c-capabilities">
          <header>
            <p>WHAT CHANGES</p>
            <h2>
              Structure where it helps.
              <br />
              Freedom where it matters.
            </h2>
          </header>
          <div>
            {capabilities.map(([title, description], index) => (
              <article key={title}>
                <span>{index + 1}</span>
                <h3>{title}</h3>
                <p>{description}</p>
              </article>
            ))}
          </div>
          <MiniLifecycle mode="c" />
        </section>

        <section className="c-install" id="install">
          <p className="c-folio">PUBLIC RELEASE / VERIFIED BUILDS</p>
          <h2>Take control of the terminal operation.</h2>
          <p>Available for macOS, Linux, Windows, and WSL.</p>
          <div className="vdp-actions">
            <a className="vdp-button is-primary" href="/download">
              Choose your build
            </a>
            <a className="vdp-button" href="/docs/getting-started/quick-start">
              Read the quick start
            </a>
          </div>
        </section>
      </main>
    </article>
  );
}

function PrototypeSwitcher({
  current,
  select,
}: {
  current: Variant;
  select: (variant: Variant) => void;
}) {
  const currentIndex = variants.findIndex(({ id }) => id === current);
  const move = (direction: -1 | 1) => {
    const next = (currentIndex + direction + variants.length) % variants.length;
    select(variants[next]!.id);
  };

  useEffect(() => {
    const handleKey = (event: KeyboardEvent) => {
      const target = event.target as HTMLElement | null;
      if (target?.matches("input, textarea, select, [contenteditable='true']")) return;
      if (event.key === "ArrowLeft") move(-1);
      if (event.key === "ArrowRight") move(1);
    };
    window.addEventListener("keydown", handleKey);
    return () => window.removeEventListener("keydown", handleKey);
  });

  const selected = variants[currentIndex]!;
  return (
    <aside className="vdp-switcher" aria-label="Visual direction switcher">
      <button type="button" onClick={() => move(-1)} aria-label="Previous direction">
        ←
      </button>
      <div>
        <span>Concept {selected.id.toUpperCase()} of 3</span>
        <strong>{selected.name}</strong>
        <small>{selected.note}</small>
      </div>
      <div className="vdp-switcher__dots">
        {variants.map(({ id, name }) => (
          <button
            key={id}
            type="button"
            className={id === current ? "is-active" : ""}
            onClick={() => select(id)}
            aria-label={`Show ${name}`}
          />
        ))}
      </div>
      <button type="button" onClick={() => move(1)} aria-label="Next direction">
        →
      </button>
    </aside>
  );
}

export function VisualDirectionPrototype() {
  const [variant, setVariant] = useState<Variant>("a");

  useEffect(() => {
    const requested = new URLSearchParams(window.location.search).get("variant");
    if (requested === "a" || requested === "b" || requested === "c") setVariant(requested);
  }, []);

  const select = (next: Variant) => {
    setVariant(next);
    const url = new URL(window.location.href);
    url.searchParams.set("variant", next);
    window.history.replaceState({}, "", url);
    window.scrollTo({ top: 0, behavior: "instant" });
  };

  return (
    <div className="vdp-root" data-variant={variant}>
      <div className="vdp-prototype-note">
        Prototype / visual direction only / content and product capture are provisional
      </div>
      {variant === "a" && <ConceptA />}
      {variant === "b" && <ConceptB />}
      {variant === "c" && <ConceptC />}
      <PrototypeSwitcher current={variant} select={select} />
    </div>
  );
}
