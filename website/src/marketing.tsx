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

export function SessionShot({ title, caption }: { title: string; caption?: ReactNode }) {
  return (
    <section className="gardn-section" aria-labelledby="session-shot-title">
      <h2 id="session-shot-title" className="gardn-section-title">
        {title}
      </h2>
      <figure className="gardn-session-shot">
        <img
          src="/session.png"
          width={1284}
          height={820}
          alt="A Gardn session with product, ops, and commerce groups, a split checkout space, and agents in triage, working, and idle."
        />
        {caption && <figcaption className="gardn-session-shot-caption">{caption}</figcaption>}
      </figure>
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
