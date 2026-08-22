import { Link } from "fumapress/client";
import { canonicalUrl } from "../site-url";

export default function ReleasesPage() {
  return (
    <>
      <title>Release status | Gardn</title>
      <meta
        name="description"
        content="Public release status and the verification contract for Gardn artifacts and release notes."
      />
      <meta property="og:title" content="Release status | Gardn" />
      <meta
        property="og:description"
        content="How Gardn will publish verified binaries, compatibility details, and release notes."
      />
      <meta name="twitter:title" content="Release status | Gardn" />
      <meta
        name="twitter:description"
        content="How verified binaries, compatibility details, and release notes will be published."
      />
      <link rel="canonical" href={canonicalUrl("/releases")} />
      <meta property="og:url" content={canonicalUrl("/releases")} />

      <main className="gardn-page gardn-status-page">
        <section className="gardn-shell gardn-status-hero" aria-labelledby="page-title">
          <div className="gardn-status-row">
            <p className="gardn-eyebrow">Release status</p>
            <p className="gardn-status" data-tone="holding">
              Pre-public
            </p>
          </div>
          <h1 id="page-title" className="gardn-title">
            Release history starts at the gate.
          </h1>
          <p className="gardn-copy gardn-copy-large">
            There is no public binary release to announce yet. This page will show only tagged,
            verified releases—never preview content promoted by accident.
          </p>
          <div className="gardn-actions">
            <Link className="gardn-action" data-primary="true" href="/download">
              See installation options
            </Link>
            <Link className="gardn-action" href="/docs/guides/updates-and-handoff">
              Understand updates and handoff
            </Link>
          </div>
        </section>

        <section className="gardn-shell gardn-section" aria-labelledby="release-contract-title">
          <div className="gardn-section-intro">
            <p className="gardn-eyebrow">Publication contract</p>
            <h2 id="release-contract-title" className="gardn-section-title">
              A useful release answers three questions.
            </h2>
          </div>
          <div className="gardn-card-grid">
            <article className="gardn-card">
              <p className="gardn-card-index" aria-hidden="true">
                01
              </p>
              <h3>What can I install?</h3>
              <p>
                Only platform artifacts that completed the release gate appear as download actions.
              </p>
            </article>
            <article className="gardn-card">
              <p className="gardn-card-index" aria-hidden="true">
                02
              </p>
              <h3>What changed?</h3>
              <p>
                Release-controlled notes describe user-visible behavior without exposing internal
                planning or preview state.
              </p>
            </article>
            <article className="gardn-card">
              <p className="gardn-card-index" aria-hidden="true">
                03
              </p>
              <h3>Will it attach safely?</h3>
              <p>
                Compatibility and handoff guidance distinguishes a live process transfer from a full
                session restart.
              </p>
            </article>
          </div>
        </section>

        <section
          className="gardn-shell gardn-section gardn-gate"
          aria-labelledby="release-now-title"
        >
          <div>
            <p className="gardn-eyebrow">Right now</p>
            <h2 id="release-now-title" className="gardn-section-title">
              Build first. Verify the boundary.
            </h2>
          </div>
          <div className="gardn-gate-copy">
            <p>
              The documented source checkout and Nix flake are the available installation paths. Old
              or private artifacts are not evidence of public availability.
            </p>
            <p>
              If preserving live panes matters, read the update guide before replacing a running
              server. A restart and a handoff do not preserve the same state.
            </p>
            <div className="gardn-actions">
              <Link className="gardn-action" href="/docs/getting-started/install">
                Open the install guide
              </Link>
              <a
                className="gardn-action"
                href="https://github.com/masakirocorp/gardn"
                rel="noreferrer"
              >
                Follow the repository
              </a>
            </div>
          </div>
        </section>
      </main>
    </>
  );
}
