/**
 * Local, offline brand marks for service templates.
 *
 * Uses the `simple-icons` package (CC0-1.0 licensed SVG path data) bundled into
 * the app — so we render the real brand logos with NO network request. Only the
 * specific icons we use are imported by name, so Vite tree-shakes the rest out of
 * the bundle. Keyed by the template's `slug`. Anything without an entry here
 * falls back to the named glyph or the monogram in `ServiceMark`.
 */

import {
  siApple,
  siClaude,
  siCloudflare,
  siGithub,
  siGoogle,
  siHuggingface,
  siInstagram,
  siMega,
  siPostgresql,
  siPrisma,
  siResend,
  siShopify,
  siStripe,
  siSupabase,
  siVercel,
} from "simple-icons";

interface BrandIcon {
  path: string;
  /** Official brand hex (without '#'). */
  hex: string;
}

/** template slug → Simple Icons data. */
const MARKS: Record<string, BrandIcon> = {
  apple: siApple,
  supabase: siSupabase,
  google: siGoogle,
  github: siGithub,
  huggingface: siHuggingface,
  instagram: siInstagram,
  mega: siMega,
  vercel: siVercel,
  stripe: siStripe,
  cloudflare: siCloudflare,
  postgresql: siPostgresql,
  shopify: siShopify,
  prisma: siPrisma,
  claude: siClaude,
  resend: siResend,
};

/** Whether a local brand mark exists for a slug. */
export function hasBrandMark(slug?: string | null): boolean {
  return !!slug && slug in MARKS;
}

/**
 * Render the real brand logo for a slug, tinted with `color` (the template's
 * brand color, matching the design). Returns `null` if we don't bundle that slug.
 */
export function BrandMark({
  slug,
  size,
  color,
}: {
  slug?: string | null;
  size: number;
  color: string;
}) {
  const icon = slug ? MARKS[slug] : undefined;
  if (!icon) return null;
  return (
    <svg
      role="img"
      width={size}
      height={size}
      viewBox="0 0 24 24"
      fill={color}
      style={{ display: "block" }}
    >
      <path d={icon.path} />
    </svg>
  );
}
