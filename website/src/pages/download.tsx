import { Link } from "fumapress/client";
import { canonicalUrl } from "../site-url";

export default function DownloadPage() {
  return (
    <>
      <title>Download — Oh My Herdr</title>
      <meta
        name="description"
        content="Verified installation and download status for Oh My Herdr."
      />
      <link rel="canonical" href={canonicalUrl("/download")} />
      <meta property="og:url" content={canonicalUrl("/download")} />
      <main className="omh-page">
        <section className="omh-shell" aria-labelledby="page-title">
          <p className="omh-eyebrow">Download</p>
          <h1 id="page-title" className="omh-title">
            Release artifacts are still being verified.
          </h1>
          <p className="omh-copy">
            Public installation commands and download buttons will appear only after every supported
            artifact passes the release gate. This page deliberately makes no unreleased
            availability claim.
          </p>
          <div className="omh-actions">
            <Link className="omh-action" data-primary="true" href="/docs">
              Read the documentation
            </Link>
            <Link className="omh-action" href="/releases">
              Release status
            </Link>
          </div>
        </section>
      </main>
    </>
  );
}
