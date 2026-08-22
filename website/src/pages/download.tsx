import { Link } from "fumapress/client";
import { canonicalUrl } from "../site-url";

export default function DownloadPage() {
  return (
    <>
      <title>Install and download — Gardn</title>
      <meta
        name="description"
        content="Install Gardn from source or Nix while public binary artifacts remain behind the release gate."
      />
      <meta property="og:title" content="Install and download — Gardn" />
      <meta
        property="og:description"
        content="Source and Nix installation paths, supported platforms, and verified binary release status."
      />
      <meta name="twitter:title" content="Install and download — Gardn" />
      <meta
        name="twitter:description"
        content="Source and Nix installation paths with verified binary release status."
      />
      <link rel="canonical" href={canonicalUrl("/download")} />
      <meta property="og:url" content={canonicalUrl("/download")} />

      <main className="gardn-page gardn-status-page">
        <section className="gardn-shell gardn-status-hero" aria-labelledby="page-title">
          <div className="gardn-status-row">
            <p className="gardn-eyebrow">Install Gardn</p>
            <p className="gardn-status" data-tone="holding">
              Public binaries in verification
            </p>
          </div>
          <h1 id="page-title" className="gardn-title">
            Start from source. Downloads stay gated.
          </h1>
          <p className="gardn-copy gardn-copy-large">
            The product is pre-public. Build the current source or use the Nix flake today; binary
            buttons will appear only after every advertised artifact passes the release gate.
          </p>
          <div className="gardn-actions">
            <Link className="gardn-action" data-primary="true" href="/docs/getting-started/install">
              Follow the install guide
            </Link>
            <a
              className="gardn-action"
              href="https://github.com/masakirocorp/gardn"
              rel="noreferrer"
            >
              View source on GitHub
            </a>
          </div>
        </section>

        <section className="gardn-shell gardn-section" aria-labelledby="install-paths-title">
          <div className="gardn-section-intro">
            <p className="gardn-eyebrow">Available now</p>
            <h2 id="install-paths-title" className="gardn-section-title">
              Two source-backed paths
            </h2>
            <p className="gardn-copy">
              Both paths follow the current repository source. Read the installation guide before
              pinning either command for automation.
            </p>
          </div>
          <div className="gardn-card-grid gardn-install-grid">
            <article className="gardn-card">
              <p className="gardn-card-index" aria-hidden="true">
                01
              </p>
              <h3>Build with Cargo</h3>
              <p>Clone the current repository and install the workspace binary from its package.</p>
              <pre className="gardn-command" aria-label="Cargo source installation commands">
                <code>{`git clone https://github.com/masakirocorp/gardn.git
cd gardn
cargo install --path apps/gardn`}</code>
              </pre>
            </article>
            <article className="gardn-card">
              <p className="gardn-card-index" aria-hidden="true">
                02
              </p>
              <h3>Install with Nix</h3>
              <p>Use the repository flake on x86_64 or aarch64 Linux and macOS.</p>
              <pre className="gardn-command" aria-label="Nix source installation command">
                <code>
                  {`nix profile install \\
  "github:masakirocorp/gardn#gardn"`}
                </code>
              </pre>
            </article>
          </div>
        </section>

        <section className="gardn-shell gardn-section gardn-gate" aria-labelledby="release-gate-title">
          <div>
            <p className="gardn-eyebrow">Release gate</p>
            <h2 id="release-gate-title" className="gardn-section-title">
              No button before its binary.
            </h2>
          </div>
          <div className="gardn-gate-copy">
            <p>
              Public download controls remain intentionally absent. Platform, checksum, and release
              metadata must agree before a binary is advertised here.
            </p>
            <p>
              Local clients are supported on macOS, Linux, Windows, and WSL, with platform-specific
              boundaries documented separately. The remote bridge has a narrower Unix boundary.
            </p>
            <div className="gardn-actions">
              <Link className="gardn-action" href="/docs/reference/platforms">
                Check platform support
              </Link>
              <Link className="gardn-action" href="/releases">
                Read release status
              </Link>
            </div>
          </div>
        </section>
      </main>
    </>
  );
}
