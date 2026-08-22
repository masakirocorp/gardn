import { defineConfig } from "fumapress";
import { createRootLayout } from "fumapress/layouts/root";
import { fumadocsMdx } from "fumapress/adapters/mdx";
import { flexsearchPlugin } from "fumapress/plugins/flexsearch";
import { linkValidationPlugin } from "fumapress/plugins/link-validation";
import { llmsPlugin } from "fumapress/plugins/llms.txt";
import { sitemapPlugin } from "fumapress/plugins/sitemap";
import { takumiPlugin } from "fumapress/plugins/takumi";
import { docs } from "./.source/server";
import NotFoundPage from "./src/not-found";
import { canonicalUrl, siteOrigin } from "./src/site-url";

const generatedApiReferencePrefix = "/docs/api/reference/generated/";

export default defineConfig({
  mode: "static",
  content: docs.toFumadocsSource(),
  site: {
    name: "Gardn",
    baseUrl: siteOrigin,
    git: {
      user: "masakirocorp",
      branch: "master",
      repo: "gardn",
    },
  },
  meta: {
    root() {
      return (
        <>
          <meta name="description" content="Terminal workspace management for AI coding agents." />
          <meta property="og:type" content="website" />
          <meta property="og:site_name" content="Gardn" />
          <meta property="og:title" content="Gardn" />
          <meta
            property="og:description"
            content="Terminal workspace management for AI coding agents."
          />
          <meta property="og:image" content={canonicalUrl("/social-card.png")} />
          <meta property="og:image:width" content="1200" />
          <meta property="og:image:height" content="630" />
          <meta
            property="og:image:alt"
            content="Gardn — terminal workspace management for AI coding agents"
          />
          <meta name="twitter:card" content="summary_large_image" />
          <meta name="twitter:title" content="Gardn" />
          <meta
            name="twitter:description"
            content="Terminal workspace management for AI coding agents."
          />
          <meta name="twitter:image" content={canonicalUrl("/social-card.png")} />
          <meta
            name="twitter:image:alt"
            content="Gardn — terminal workspace management for AI coding agents"
          />
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
  .layouts({
    root: createRootLayout({ providerProps: { theme: { defaultTheme: "dark" } } }),
    notFound: NotFoundPage,
  })
  .plugins(
    flexsearchPlugin({
      async buildIndex(page) {
        for (const adapter of this.adapters) {
          const structuredData = await adapter["core:get-structured-data"]?.call(this, page);
          if (structuredData === undefined) continue;

          return {
            id: page.url,
            title: page.data.title ?? page.path,
            description: page.data.description,
            url: page.url,
            structuredData: page.url.startsWith(generatedApiReferencePrefix)
              ? { headings: structuredData.headings, contents: [] }
              : structuredData,
          };
        }

        throw new Error("No adapter supplied structured search data");
      },
    }),
    llmsPlugin(),
    sitemapPlugin(),
    linkValidationPlugin(),
    takumiPlugin(),
  )
  .adapters(fumadocsMdx());
