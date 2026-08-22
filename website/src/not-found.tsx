import { Link } from "fumapress/client";

export default function NotFoundPage() {
  return (
    <>
      <title>Page not found — Gardn</title>
      <meta name="robots" content="noindex" />
      <main className="gardn-page">
        <section className="gardn-shell" aria-labelledby="page-title">
          <p className="gardn-eyebrow">404</p>
          <h1 id="page-title" className="gardn-title">
            That workspace does not exist.
          </h1>
          <p className="gardn-copy">The address may have moved, or the page may not be public yet.</p>
          <div className="gardn-actions">
            <Link className="gardn-action" data-primary="true" href="/">
              Return home
            </Link>
            <Link className="gardn-action" href="/docs">
              Search the documentation
            </Link>
          </div>
        </section>
      </main>
    </>
  );
}
