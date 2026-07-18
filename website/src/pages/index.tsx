import { Link } from "fumapress/client";
import { canonicalUrl } from "../site-url";

export default function HomePage() {
  return (
    <>
      <title>Oh My Herdr — Terminal workspace management for AI coding agents</title>
      <meta
        name="description"
        content="Run coding agents in durable terminal workspaces with local-first coordination."
      />
      <meta property="og:title" content="Oh My Herdr" />
      <meta
        property="og:description"
        content="Terminal workspace management for AI coding agents."
      />
      <link rel="canonical" href={canonicalUrl("/")} />
      <meta property="og:url" content={canonicalUrl("/")} />
      <main className="omh-page">
        <section className="omh-shell" aria-labelledby="page-title">
          <p className="omh-eyebrow">Local-first agent workspace</p>
          <h1 id="page-title" className="omh-title">
            Keep the terminal work. Lose the terminal sprawl.
          </h1>
          <p className="omh-copy">
            Oh My Herdr organizes coding agents, shells, and project context into durable spaces,
            tabs, and panes. This scaffold reserves the product story for verified public content.
          </p>
          <div className="omh-actions">
            <Link className="omh-action" data-primary="true" href="/docs">
              Read the documentation
            </Link>
            <Link className="omh-action" href="/download">
              Download status
            </Link>
          </div>
        </section>
      </main>
    </>
  );
}
