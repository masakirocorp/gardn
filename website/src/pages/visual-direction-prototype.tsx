import { canonicalUrl } from "../site-url";
import { VisualDirectionPrototype } from "../visual-direction-prototype";

export default function VisualDirectionPrototypePage() {
  return (
    <>
      <title>Visual direction prototype — Gardn</title>
      <meta name="robots" content="noindex, nofollow" />
      <link rel="canonical" href={canonicalUrl("/visual-direction-prototype")} />
      <VisualDirectionPrototype />
    </>
  );
}
