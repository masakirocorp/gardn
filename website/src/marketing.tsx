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
    <section className="gardn-shell gardn-hero" aria-labelledby="hero-title">
      <p className="gardn-eyebrow">{eyebrow}</p>
      <h1 id="hero-title" className="gardn-title">
        {title}
      </h1>
      <div className="gardn-copy">{children}</div>
      {actions && <div className="gardn-actions">{actions}</div>}
      {status && <div className="gardn-hero-status">{status}</div>}
    </section>
  );
}

export function Schematic({ title, caption }: { title: string; caption?: ReactNode }) {
  return (
    <section className="gardn-section" aria-labelledby="schematic-title">
      <h2 id="schematic-title" className="gardn-section-title">
        {title}
      </h2>
      <figure className="gardn-schematic" aria-label="Gardn session interface schematic">
        <div className="gardn-schematic-titlebar">
          <span className="gardn-schematic-dot" aria-hidden="true" />
          <span className="gardn-schematic-title">gardn · default session</span>
        </div>
        <div className="gardn-schematic-body">
          <div className="gardn-schematic-sidebar">
            <div className="gardn-schematic-group">Spaces</div>
            <ul className="gardn-schematic-list">
              <li className="is-active">web</li>
              <li>api</li>
              <li>agents</li>
            </ul>
            <div className="gardn-schematic-group">Agents</div>
            <ul className="gardn-schematic-list">
              <li>
                <span className="gardn-status gardn-status--working">working</span>
                <span>codex</span>
              </li>
              <li>
                <span className="gardn-status gardn-status--idle">idle</span>
                <span>omp</span>
              </li>
            </ul>
          </div>
          <div className="gardn-schematic-main">
            <div className="gardn-schematic-tabs">
              <div className="gardn-schematic-tab is-active">tab 1</div>
              <div className="gardn-schematic-tab">tab 2</div>
            </div>
            <div className="gardn-schematic-panes">
              <div className="gardn-schematic-pane">editor · shell</div>
              <div className="gardn-schematic-pane">agent · codex</div>
            </div>
            <div className="gardn-schematic-statusbar">
              <span className="gardn-command">ctrl+b</span>
              <span>space</span>
              <span>new agent</span>
            </div>
          </div>
        </div>
      </figure>
      {caption && <p className="gardn-schematic-caption">{caption}</p>}
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
    <section className="gardn-section" aria-labelledby="workflow-title">
      <h2 id="workflow-title" className="gardn-section-title">
        {title}
      </h2>
      <ol className="gardn-workflow">
        {steps.map((step, index) => (
          <li key={index} className="gardn-workflow-step">
            <h3 className="gardn-workflow-step-title">{step.title}</h3>
            <p className="gardn-workflow-step-copy">{step.copy}</p>
            <Link className="gardn-workflow-step-link" href={step.href}>
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
    <section className="gardn-section" aria-labelledby="features-title">
      <h2 id="features-title" className="gardn-section-title">
        {title}
      </h2>
      <ul className="gardn-card-grid">
        {features.map((feature, index) => (
          <li key={index} className="gardn-card">
            <h3 className="gardn-card-title">
              <Link href={feature.href}>{feature.title}</Link>
            </h3>
            <p className="gardn-card-copy">{feature.copy}</p>
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
    <section className="gardn-section" aria-labelledby="platform-title">
      <h2 id="platform-title" className="gardn-section-title">
        {title}
      </h2>
      <div className="gardn-platform">
        <div className="gardn-copy">{children}</div>
        <table className="gardn-platform-table">
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
        {actions && <div className="gardn-actions">{actions}</div>}
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
    <section className="gardn-section" aria-labelledby="cta-title">
      <h2 id="cta-title" className="gardn-section-title">
        {title}
      </h2>
      <div className="gardn-copy">{children}</div>
      {actions && <div className="gardn-actions">{actions}</div>}
    </section>
  );
}

export function Footer({ children }: { children: ReactNode }) {
  return <footer className="gardn-footer">{children}</footer>;
}
