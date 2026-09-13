import { Link } from "fumapress/client";
import { canonicalUrl } from "../site-url";

export default function ReleasesPage() {
  return (
    <>
      <title>Release status | Gardn</title>
      <meta
        name="description"
        content="Read the latest Gardn release notes and download the verified macOS app."
      />
      <meta property="og:title" content="Release status | Gardn" />
      <meta
        property="og:description"
        content="Download Gardn 0.10.26 for macOS or review the latest release notes."
      />
      <meta name="twitter:title" content="Release status | Gardn" />
      <meta
        name="twitter:description"
        content="Download Gardn 0.10.26 and read the latest release notes."
      />
      <link rel="canonical" href={canonicalUrl("/releases")} />
      <meta property="og:url" content={canonicalUrl("/releases")} />

      <main className="gardn-page gardn-status-page">
        <section className="gardn-shell gardn-status-hero" aria-labelledby="page-title">
          <div className="gardn-status-row">
            <p className="gardn-eyebrow">Release status</p>
            <p className="gardn-status gardn-status--live">v0.10.26 available</p>
          </div>
          <h1 id="page-title" className="gardn-title">
            Gardn 0.10.26 is available.
          </h1>
          <p className="gardn-copy gardn-copy-large">
            Download the signed and notarized macOS app, or review every artifact in the GitHub
            release.
          </p>
          <div className="gardn-actions">
            <Link className="gardn-action" data-primary="true" href="/download">
              Download for macOS
            </Link>
            <a
              className="gardn-action"
              href="https://github.com/masakirocorp/gardn/releases/tag/v0.10.26"
            >
              Read the v0.10.26 release notes
            </a>
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
          aria-labelledby="latest-release-title"
        >
          <div>
            <p className="gardn-eyebrow">Latest release</p>
            <h2 id="latest-release-title" className="gardn-section-title">
              Verified binaries are ready.
            </h2>
          </div>
          <div className="gardn-gate-copy">
            <p>
              The release workflow builds Gardn for macOS, Linux, and Windows. It signs and
              notarizes the universal macOS app.
            </p>
            <div className="gardn-actions">
              <Link className="gardn-action" href="/docs/guides/updates-and-handoff">
                Read the update guide
              </Link>
              <a
                className="gardn-action"
                href="https://github.com/masakirocorp/gardn/releases/tag/v0.10.26"
              >
                View the GitHub release
              </a>
            </div>
          </div>
        </section>
      </main>
    </>
  );
}
