import { canonicalUrl } from "../../site-url";
export const GET = async (): Promise<Response> => {
  const body = `User-agent: *\nAllow: /\n\nSitemap: ${canonicalUrl("/sitemap.xml")}\n`;

  return new Response(body, {
    headers: { "Content-Type": "text/plain; charset=utf-8" },
  });
};

export const getConfig = async () => ({ render: "static" }) as const;
