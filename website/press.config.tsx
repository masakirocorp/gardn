import { defineConfig } from "fumapress";
import { fumadocsMdx } from "fumapress/adapters/mdx";
import { flexsearchPlugin } from "fumapress/plugins/flexsearch";
import { linkValidationPlugin } from "fumapress/plugins/link-validation";
import { llmsPlugin } from "fumapress/plugins/llms.txt";
import { sitemapPlugin } from "fumapress/plugins/sitemap";
import { takumiPlugin } from "fumapress/plugins/takumi";
import { docs } from "./.source/server";
import NotFoundPage from "./src/not-found";
import { canonicalUrl, siteOrigin } from "./src/site-url";

export default defineConfig({
  mode: "static",
  content: docs.toFumadocsSource(),
  site: {
    name: "Oh My Herdr",
    baseUrl: siteOrigin,
    git: {
      user: "masakirocorp",
      branch: "master",
      repo: "oh-my-herdr",
    },
  },
  meta: {
    root() {
      return (
        <>
          <meta name="description" content="Terminal workspace management for AI coding agents." />
          <meta property="og:type" content="website" />
          <meta property="og:site_name" content="Oh My Herdr" />
          <meta property="og:title" content="Oh My Herdr" />
          <meta
            property="og:description"
            content="Terminal workspace management for AI coding agents."
          />
          <meta name="twitter:card" content="summary" />
          <meta name="theme-color" content="oklch(0.175 0.009 145)" />
          <link rel="icon" href="/favicon.svg" type="image/svg+xml" />
        </>
      );
    },
    page(page) {
      const url = canonicalUrl(page.url);
      return (
        <>
          <link rel="canonical" href={url} />
          <meta property="og:url" content={url} />
        </>
      );
    },
  },
})
  .layouts({ notFound: NotFoundPage })
  .plugins(
    flexsearchPlugin(),
    llmsPlugin(),
    sitemapPlugin(),
    linkValidationPlugin(),
    takumiPlugin(),
  )
  .adapters(fumadocsMdx());
