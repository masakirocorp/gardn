import "../atmosphere-prototype.css";
import { canonicalUrl } from "../site-url";
import { AtmospherePrototype } from "../atmosphere-prototype";

export default function AtmospherePrototypePage() {
  return (
    <>
      <title>Atmosphere prototype — Gardn</title>
      <meta name="robots" content="noindex, nofollow" />
      <link rel="canonical" href={canonicalUrl("/atmosphere-prototype")} />
      <AtmospherePrototype />
    </>
  );
}
