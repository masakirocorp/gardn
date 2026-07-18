import { Link } from "fumapress/client";

export default function NotFoundPage() {
  return (
    <>
      <title>Page not found — Oh My Herdr</title>
      <meta name="robots" content="noindex" />
      <main className="omh-page">
        <section className="omh-shell" aria-labelledby="page-title">
          <p className="omh-eyebrow">404</p>
          <h1 id="page-title" className="omh-title">
            That workspace does not exist.
          </h1>
          <p className="omh-copy">The address may have moved, or the page may not be public yet.</p>
          <div className="omh-actions">
            <Link className="omh-action" data-primary="true" href="/">
              Return home
            </Link>
            <Link className="omh-action" href="/docs">
              Search the documentation
            </Link>
          </div>
        </section>
      </main>
    </>
  );
}
