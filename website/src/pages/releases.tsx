import { Link } from "fumapress/client";
import { canonicalUrl } from "../site-url";

export default function ReleasesPage() {
  return (
    <>
      <title>Releases — Oh My Herdr</title>
      <meta
        name="description"
        content="Release notes and verified artifact status for Oh My Herdr."
      />
      <link rel="canonical" href={canonicalUrl("/releases")} />
      <meta property="og:url" content={canonicalUrl("/releases")} />
      <main className="omh-page">
        <section className="omh-shell" aria-labelledby="page-title">
          <p className="omh-eyebrow">Releases</p>
          <h1 id="page-title" className="omh-title">
            Release history will come from tagged builds.
          </h1>
          <p className="omh-copy">
            This route is reserved for release-controlled notes and downloads generated from
            authoritative GitHub release metadata. Preview content cannot silently become the latest
            release.
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
