import { Link } from "fumapress/client";
import { canonicalUrl } from "../site-url";

export default function DownloadPage() {
  return (
    <>
      <title>Install and download — Oh My Herdr</title>
      <meta
        name="description"
        content="Install Oh My Herdr from source or Nix while public binary artifacts remain behind the release gate."
      />
      <meta property="og:title" content="Install and download — Oh My Herdr" />
      <meta
        property="og:description"
        content="Source and Nix installation paths, supported platforms, and verified binary release status."
      />
      <meta name="twitter:title" content="Install and download — Oh My Herdr" />
      <meta
        name="twitter:description"
        content="Source and Nix installation paths with verified binary release status."
      />
      <link rel="canonical" href={canonicalUrl("/download")} />
      <meta property="og:url" content={canonicalUrl("/download")} />

      <main className="omh-page omh-status-page">
        <section className="omh-shell omh-status-hero" aria-labelledby="page-title">
          <div className="omh-status-row">
            <p className="omh-eyebrow">Install Oh My Herdr</p>
            <p className="omh-status" data-tone="holding">
              Public binaries in verification
            </p>
          </div>
          <h1 id="page-title" className="omh-title">
            Start from source. Downloads stay gated.
          </h1>
          <p className="omh-copy omh-copy-large">
            The product is pre-public. Build the current source or use the Nix flake today; binary
            buttons will appear only after every advertised artifact passes the release gate.
          </p>
          <div className="omh-actions">
            <Link className="omh-action" data-primary="true" href="/docs/getting-started/install">
              Follow the install guide
            </Link>
            <a
              className="omh-action"
              href="https://github.com/masakirocorp/oh-my-herdr"
              rel="noreferrer"
            >
              View source on GitHub
            </a>
          </div>
        </section>

        <section className="omh-shell omh-section" aria-labelledby="install-paths-title">
          <div className="omh-section-intro">
            <p className="omh-eyebrow">Available now</p>
            <h2 id="install-paths-title" className="omh-section-title">
              Two source-backed paths
            </h2>
            <p className="omh-copy">
              Both paths follow the current repository source. Read the installation guide before
              pinning either command for automation.
            </p>
          </div>
          <div className="omh-card-grid omh-install-grid">
            <article className="omh-card">
              <p className="omh-card-index" aria-hidden="true">
                01
              </p>
              <h3>Build with Cargo</h3>
              <p>Clone the current repository and install the workspace binary from its package.</p>
              <pre className="omh-command" aria-label="Cargo source installation commands">
                <code>{`git clone https://github.com/masakirocorp/oh-my-herdr.git
cd oh-my-herdr
cargo install --path apps/omh`}</code>
              </pre>
            </article>
            <article className="omh-card">
              <p className="omh-card-index" aria-hidden="true">
                02
              </p>
              <h3>Install with Nix</h3>
              <p>Use the repository flake on x86_64 or aarch64 Linux and macOS.</p>
              <pre className="omh-command" aria-label="Nix source installation command">
                <code>
                  {`nix profile install \\
  "github:masakirocorp/oh-my-herdr#omh"`}
                </code>
              </pre>
            </article>
          </div>
        </section>

        <section className="omh-shell omh-section omh-gate" aria-labelledby="release-gate-title">
          <div>
            <p className="omh-eyebrow">Release gate</p>
            <h2 id="release-gate-title" className="omh-section-title">
              No button before its binary.
            </h2>
          </div>
          <div className="omh-gate-copy">
            <p>
              Public download controls remain intentionally absent. Platform, checksum, and release
              metadata must agree before a binary is advertised here.
            </p>
            <p>
              Local clients are supported on macOS, Linux, Windows, and WSL, with platform-specific
              boundaries documented separately. The remote bridge has a narrower Unix boundary.
            </p>
            <div className="omh-actions">
              <Link className="omh-action" href="/docs/reference/platforms">
                Check platform support
              </Link>
              <Link className="omh-action" href="/releases">
                Read release status
              </Link>
            </div>
          </div>
        </section>
      </main>
    </>
  );
}
