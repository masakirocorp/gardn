import { Link } from "fumapress/client";
import { canonicalUrl } from "../site-url";

export default function ReleasesPage() {
  return (
    <>
      <title>Release status — Oh My Herdr</title>
      <meta
        name="description"
        content="Public release status and the verification contract for Oh My Herdr artifacts and release notes."
      />
      <meta property="og:title" content="Release status — Oh My Herdr" />
      <meta
        property="og:description"
        content="How Oh My Herdr will publish verified binaries, compatibility details, and release notes."
      />
      <meta name="twitter:title" content="Release status — Oh My Herdr" />
      <meta
        name="twitter:description"
        content="How verified binaries, compatibility details, and release notes will be published."
      />
      <link rel="canonical" href={canonicalUrl("/releases")} />
      <meta property="og:url" content={canonicalUrl("/releases")} />

      <main className="omh-page omh-status-page">
        <section className="omh-shell omh-status-hero" aria-labelledby="page-title">
          <div className="omh-status-row">
            <p className="omh-eyebrow">Release status</p>
            <p className="omh-status" data-tone="holding">
              Pre-public
            </p>
          </div>
          <h1 id="page-title" className="omh-title">
            Release history starts at the gate.
          </h1>
          <p className="omh-copy omh-copy-large">
            There is no public binary release to announce yet. This page will show only tagged,
            verified releases—never preview content promoted by accident.
          </p>
          <div className="omh-actions">
            <Link className="omh-action" data-primary="true" href="/download">
              See installation options
            </Link>
            <Link className="omh-action" href="/docs/guides/updates-and-handoff">
              Understand updates and handoff
            </Link>
          </div>
        </section>

        <section className="omh-shell omh-section" aria-labelledby="release-contract-title">
          <div className="omh-section-intro">
            <p className="omh-eyebrow">Publication contract</p>
            <h2 id="release-contract-title" className="omh-section-title">
              A useful release answers three questions.
            </h2>
          </div>
          <div className="omh-card-grid">
            <article className="omh-card">
              <p className="omh-card-index" aria-hidden="true">
                01
              </p>
              <h3>What can I install?</h3>
              <p>
                Only platform artifacts that completed the release gate appear as download actions.
              </p>
            </article>
            <article className="omh-card">
              <p className="omh-card-index" aria-hidden="true">
                02
              </p>
              <h3>What changed?</h3>
              <p>
                Release-controlled notes describe user-visible behavior without exposing internal
                planning or preview state.
              </p>
            </article>
            <article className="omh-card">
              <p className="omh-card-index" aria-hidden="true">
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

        <section className="omh-shell omh-section omh-gate" aria-labelledby="release-now-title">
          <div>
            <p className="omh-eyebrow">Right now</p>
            <h2 id="release-now-title" className="omh-section-title">
              Build first. Verify the boundary.
            </h2>
          </div>
          <div className="omh-gate-copy">
            <p>
              The documented source checkout and Nix flake are the available installation paths. Old
              or private artifacts are not evidence of public availability.
            </p>
            <p>
              If preserving live panes matters, read the update guide before replacing a running
              server. A restart and a handoff do not preserve the same state.
            </p>
            <div className="omh-actions">
              <Link className="omh-action" href="/docs/getting-started/install">
                Open the install guide
              </Link>
              <a
                className="omh-action"
                href="https://github.com/masakirocorp/oh-my-herdr"
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
