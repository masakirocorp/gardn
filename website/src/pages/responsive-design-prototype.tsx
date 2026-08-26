import "../responsive-design-prototype.css";
import "../visual-direction-prototype.css";
import { canonicalUrl } from "../site-url";
import { ResponsiveDesignPrototype } from "../responsive-design-prototype";

export default function ResponsiveDesignPrototypePage() {
  return (
    <>
      <title>Responsive marketing design | Gardn</title>
      <meta name="robots" content="noindex, nofollow" />
      <link rel="canonical" href={canonicalUrl("/responsive-design-prototype")} />
      <ResponsiveDesignPrototype />
    </>
  );
}
