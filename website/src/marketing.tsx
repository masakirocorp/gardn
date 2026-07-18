import { Link } from "fumapress/client";
import type { ReactNode } from "react";

export function Hero({
  eyebrow,
  title,
  children,
  actions,
  status,
}: {
  eyebrow: string;
  title: string;
  children: ReactNode;
  actions?: ReactNode;
  status?: ReactNode;
}) {
  return (
    <section className="omh-shell omh-hero" aria-labelledby="hero-title">
      <p className="omh-eyebrow">{eyebrow}</p>
      <h1 id="hero-title" className="omh-title">
        {title}
      </h1>
      <div className="omh-copy">{children}</div>
      {actions && <div className="omh-actions">{actions}</div>}
      {status && <div className="omh-hero-status">{status}</div>}
    </section>
  );
}

export function Schematic({ title, caption }: { title: string; caption?: ReactNode }) {
  return (
    <section className="omh-section" aria-labelledby="schematic-title">
      <h2 id="schematic-title" className="omh-section-title">
        {title}
      </h2>
      <figure className="omh-schematic" aria-label="Oh My Herdr session interface schematic">
        <div className="omh-schematic-titlebar">
          <span className="omh-schematic-dot" aria-hidden="true" />
          <span className="omh-schematic-title">omh — default session</span>
        </div>
        <div className="omh-schematic-body">
          <div className="omh-schematic-sidebar">
            <div className="omh-schematic-group">Spaces</div>
            <ul className="omh-schematic-list">
              <li className="is-active">web</li>
              <li>api</li>
              <li>agents</li>
            </ul>
            <div className="omh-schematic-group">Agents</div>
            <ul className="omh-schematic-list">
              <li>
                <span className="omh-status omh-status--working">working</span>
                <span>codex</span>
              </li>
              <li>
                <span className="omh-status omh-status--idle">idle</span>
                <span>omp</span>
              </li>
            </ul>
          </div>
          <div className="omh-schematic-main">
            <div className="omh-schematic-tabs">
              <div className="omh-schematic-tab is-active">tab 1</div>
              <div className="omh-schematic-tab">tab 2</div>
            </div>
            <div className="omh-schematic-panes">
              <div className="omh-schematic-pane">editor · shell</div>
              <div className="omh-schematic-pane">agent · codex</div>
            </div>
            <div className="omh-schematic-statusbar">
              <span className="omh-command">ctrl+b</span>
              <span>space</span>
              <span>new agent</span>
            </div>
          </div>
        </div>
      </figure>
      {caption && <p className="omh-schematic-caption">{caption}</p>}
    </section>
  );
}

export function Workflow({
  title,
  steps,
}: {
  title: string;
  steps: Array<{
    title: string;
    copy: ReactNode;
    href: string;
    label: string;
  }>;
}) {
  return (
    <section className="omh-section" aria-labelledby="workflow-title">
      <h2 id="workflow-title" className="omh-section-title">
        {title}
      </h2>
      <ol className="omh-workflow">
        {steps.map((step, index) => (
          <li key={index} className="omh-workflow-step">
            <h3 className="omh-workflow-step-title">{step.title}</h3>
            <p className="omh-workflow-step-copy">{step.copy}</p>
            <Link className="omh-workflow-step-link" href={step.href}>
              {step.label}
            </Link>
          </li>
        ))}
      </ol>
    </section>
  );
}

export function FeatureGrid({
  title,
  features,
}: {
  title: string;
  features: Array<{
    title: string;
    copy: ReactNode;
    href: string;
  }>;
}) {
  return (
    <section className="omh-section" aria-labelledby="features-title">
      <h2 id="features-title" className="omh-section-title">
        {title}
      </h2>
      <ul className="omh-card-grid">
        {features.map((feature, index) => (
          <li key={index} className="omh-card">
            <h3 className="omh-card-title">
              <Link href={feature.href}>{feature.title}</Link>
            </h3>
            <p className="omh-card-copy">{feature.copy}</p>
          </li>
        ))}
      </ul>
    </section>
  );
}

export function PlatformCard({
  title,
  children,
  actions,
  rows,
}: {
  title: string;
  children: ReactNode;
  actions?: ReactNode;
  rows: Array<{ platform: string; architectures: string; role: string }>;
}) {
  return (
    <section className="omh-section" aria-labelledby="platform-title">
      <h2 id="platform-title" className="omh-section-title">
        {title}
      </h2>
      <div className="omh-platform">
        <div className="omh-copy">{children}</div>
        <table className="omh-platform-table">
          <thead>
            <tr>
              <th scope="col">Platform</th>
              <th scope="col">Architectures</th>
              <th scope="col">Remote role</th>
            </tr>
          </thead>
          <tbody>
            {rows.map((row, index) => (
              <tr key={index}>
                <td>{row.platform}</td>
                <td>{row.architectures}</td>
                <td>{row.role}</td>
              </tr>
            ))}
          </tbody>
        </table>
        {actions && <div className="omh-actions">{actions}</div>}
      </div>
    </section>
  );
}

export function CTASection({
  title,
  children,
  actions,
}: {
  title: string;
  children: ReactNode;
  actions?: ReactNode;
}) {
  return (
    <section className="omh-section" aria-labelledby="cta-title">
      <h2 id="cta-title" className="omh-section-title">
        {title}
      </h2>
      <div className="omh-copy">{children}</div>
      {actions && <div className="omh-actions">{actions}</div>}
    </section>
  );
}

export function Footer({ children }: { children: ReactNode }) {
  return <footer className="omh-footer">{children}</footer>;
}
