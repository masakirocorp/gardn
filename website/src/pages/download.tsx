import { Link } from "fumapress/client";
import { canonicalUrl } from "../site-url";

const MACOS_DMG_URL =
  "https://github.com/masakirocorp/gardn/releases/download/v0.10.26/Gardn-0.10.26.dmg";
const RELEASE_URL = "https://github.com/masakirocorp/gardn/releases/tag/v0.10.26";

export default function DownloadPage() {
  return (
    <>
      <title>Install and download | Gardn</title>
      <meta
        name="description"
        content="Download the signed Gardn app for macOS, or install Gardn from source or Nix."
      />
      <meta property="og:title" content="Install and download | Gardn" />
      <meta
        property="og:description"
        content="Download Gardn for macOS, or use the source and Nix installation paths."
      />
      <meta name="twitter:title" content="Install and download | Gardn" />
      <meta
        name="twitter:description"
        content="Download Gardn for macOS, or install it from source or Nix."
      />
      <link rel="canonical" href={canonicalUrl("/download")} />
      <meta property="og:url" content={canonicalUrl("/download")} />

      <main className="gardn-page gardn-status-page">
        <section className="gardn-shell gardn-status-hero" aria-labelledby="page-title">
          <div className="gardn-status-row">
            <p className="gardn-eyebrow">Install Gardn</p>
            <p className="gardn-status gardn-status--live">v0.10.26 available</p>
          </div>
          <h1 id="page-title" className="gardn-title">
            Download Gardn for macOS.
          </h1>
          <p className="gardn-copy gardn-copy-large">
            Install the signed and notarized Gardn app on Apple silicon or Intel Macs.
          </p>
          <div className="gardn-actions">
            <a className="gardn-action" data-primary="true" href={MACOS_DMG_URL}>
              Download for macOS (.dmg)
            </a>
            <a className="gardn-action" href={RELEASE_URL}>
              Read the v0.10.26 release notes
            </a>
          </div>
        </section>

        <section className="gardn-shell gardn-section" aria-labelledby="install-paths-title">
          <div className="gardn-section-intro">
            <p className="gardn-eyebrow">Other options</p>
            <h2 id="install-paths-title" className="gardn-section-title">
              Build from source or use Nix
            </h2>
            <p className="gardn-copy">
              Use these paths when you need a source build or a declarative Nix installation.
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

        <section
          className="gardn-shell gardn-section gardn-gate"
          aria-labelledby="release-details-title"
        >
          <div>
            <p className="gardn-eyebrow">Verified release</p>
            <h2 id="release-details-title" className="gardn-section-title">
              One app for every supported Mac.
            </h2>
          </div>
          <div className="gardn-gate-copy">
            <p>
              The macOS disk image contains a universal app for Apple silicon and Intel. The release
              workflow signs and notarizes the app.
            </p>
            <p>
              The same release also provides command-line binaries for macOS, Linux, and Windows.
            </p>
            <div className="gardn-actions">
              <Link className="gardn-action" href="/docs/reference/platforms">
                Check platform support
              </Link>
              <a className="gardn-action" href={RELEASE_URL}>
                View all release downloads
              </a>
            </div>
          </div>
        </section>
      </main>
    </>
  );
}
