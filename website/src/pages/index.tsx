import "../atmosphere-prototype.css";
import { canonicalUrl } from "../site-url";
import { AtmospherePrototype } from "../atmosphere-prototype";

export default function HomePage() {
  return (
    <>
      <title>Gardn | Terminal workspace management for AI coding agents</title>
      <meta
        name="description"
        content="Run AI coding agents, shells, and project context in persistent terminal workspaces that survive disconnects."
      />
      <meta property="og:title" content="Gardn" />
      <meta
        property="og:description"
        content="Terminal workspace management for AI coding agents."
      />
      <meta name="twitter:title" content="Gardn" />
      <meta
        name="twitter:description"
        content="Run AI coding agents, shells, and project context in persistent terminal workspaces."
      />
      <link rel="canonical" href={canonicalUrl("/")} />
      <meta property="og:url" content={canonicalUrl("/")} />

      <AtmospherePrototype />
    </>
  );
}
