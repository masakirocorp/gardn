const configuredSiteUrl = process.env.SITE_URL ?? "https://oh-my-herdr.invalid";
const siteUrl = new URL(configuredSiteUrl);

if (siteUrl.protocol !== "https:") {
  throw new Error("SITE_URL must use HTTPS");
}

export const siteOrigin = siteUrl.origin;
export const canonicalUrl = (pathname: string) => new URL(pathname, siteOrigin).href;
