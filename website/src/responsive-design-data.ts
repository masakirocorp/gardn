export type RecentFeature = {
  category: string;
  label?: string;
  title: string;
  summary: string;
  href?: string;
};

export type OperatingModule = {
  number: string;
  title: string;
  evidence: string;
  summary: string;
};

export type ReleasePreview = {
  version: string;
  date: string;
  summary: string;
  features: RecentFeature[];
};

export const operatingModules: OperatingModule[] = [
  {
    number: "01",
    title: "Reproducible agents",
    evidence: "17 system profiles + custom",
    summary: "Launch a known agent setup again, then add the custom profile your project needs.",
  },
  {
    number: "02",
    title: "Intelligible projects",
    evidence: "Groups with policy, Triage, Follow Up",
    summary: "Give each project a visible home with policy, triage, and follow-up close at hand.",
  },
  {
    number: "03",
    title: "Surrounding work",
    evidence: "Discovered commands + managed runs",
    summary:
      "Run discovered commands from one palette with Git, Diff, IDE, and GitHub roles beside the work.",
  },
  {
    number: "04",
    title: "Attach without disrupting",
    evidence: "Mixed local/SSH + client-private views",
    summary:
      "Move between local and SSH sessions, keep each client private, and choose Take control explicitly.",
  },
];

export const releasePreview: ReleasePreview = {
  version: "0.3.0-preview",
  date: "Design fixture · not a public release",
  summary:
    "A representative release payload used to review the post-release layout. Nothing in this fixture is advertised as shipped.",
  features: [
    {
      category: "Agents",
      label: "profiles",
      title: "Repeat a known agent setup",
      summary:
        "System profiles make launches consistent while custom profiles keep project-specific workflows close.",
      href: "/docs/concepts",
    },
    {
      category: "Projects",
      label: "triage",
      title: "Keep project attention legible",
      summary:
        "Groups, policy, Triage, and Follow Up keep the next action visible without turning the terminal into a dashboard.",
      href: "/docs/concepts",
    },
    {
      category: "Sessions",
      label: "handoff",
      title: "Attach without rebuilding the work",
      summary:
        "Mixed local and SSH clients can reconnect to durable sessions while each client keeps its own view.",
      href: "/docs/concepts",
    },
    {
      category: "Operations",
      label: "palette",
      title: "Run the surrounding work",
      summary:
        "Discovered commands and managed runs stay available from a project palette with explicit control handoffs.",
      href: "/docs/api",
    },
  ],
};

export const platformRows = [
  { platform: "macOS", architectures: "x86_64 · aarch64", role: "Client + remote host" },
  { platform: "Linux", architectures: "x86_64 · aarch64", role: "Client + remote host" },
  { platform: "Windows", architectures: "x86_64", role: "Local client" },
  { platform: "WSL", architectures: "x86_64 · aarch64", role: "Linux path" },
] as const;
