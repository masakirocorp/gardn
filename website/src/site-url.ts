export const siteOrigin = "https://gardn.dev";
export const canonicalUrl = (pathname: string) => new URL(pathname, siteOrigin).href;
