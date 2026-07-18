const value = process.env.SITE_URL;

if (!value) throw new Error("SITE_URL is required for deployment");

const site = new URL(value);
if (site.protocol !== "https:" || site.hostname.endsWith(".invalid")) {
  throw new Error("SITE_URL must be a real HTTPS deployment origin");
}

console.log(`deployment canonical origin: ${site.origin}`);
