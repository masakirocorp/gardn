export const siteOrigin = "https://ohmyherdr.com";
export const canonicalUrl = (pathname: string) => new URL(pathname, siteOrigin).href;
