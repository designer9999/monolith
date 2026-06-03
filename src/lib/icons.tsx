/**
 * The MONOLITH custom line-icon set, ported 1:1 from the design's `ICONS` map.
 * Kept as-is (not swapped for lucide) so proportions match the mockup exactly.
 */

import type { CSSProperties } from "react";

const ICONS: Record<string, string> = {
  dash: "M3 3h7v7H3zM14 3h7v5h-7zM14 12h7v9h-7zM3 14h7v7H3z",
  vault: "M3 4h18v16H3zM3 9h18M9 9v11",
  folder: "M3 6h6l2 2h10v11H3z",
  gear: "M12 9a3 3 0 100 6 3 3 0 000-6zM12 2v3M12 19v3M2 12h3M19 12h3M5 5l2 2M17 17l2 2M19 5l-2 2M7 17l-2 2",
  search: "M11 4a7 7 0 100 14 7 7 0 000-14zM21 21l-5-5",
  grid: "M3 3h7v7H3zM14 3h7v7h-7zM14 14h7v7h-7zM3 14h7v7H3z",
  list: "M8 5h13M8 12h13M8 19h13M3 5h.01M3 12h.01M3 19h.01",
  copy: "M9 9h11v11H9zM5 15H4V4h11v1",
  eye: "M2 12s4-7 10-7 10 7 10 7-4 7-10 7S2 12 2 12zM12 9a3 3 0 100 6 3 3 0 000-6z",
  eyeoff: "M3 3l18 18M10.5 5.2A9.7 9.7 0 0112 5c6 0 10 7 10 7a17 17 0 01-3.1 3.6M6.5 6.8A17 17 0 002 12s4 7 10 7a9.6 9.6 0 003.6-.7",
  star: "M12 3l2.6 5.6 6.1.7-4.5 4.2 1.2 6L12 16.9 6.6 19.5l1.2-6L3.3 9.3l6.1-.7z",
  plus: "M12 5v14M5 12h14",
  shield: "M12 3l8 3v6c0 5-3.5 7.5-8 9-4.5-1.5-8-4-8-9V6z",
  clock: "M12 4a8 8 0 100 16 8 8 0 000-16zM12 8v4l3 2",
  chev: "M9 6l6 6-6 6",
  chevd: "M6 9l6 6 6-6",
  upload: "M12 16V4M7 9l5-5 5 5M4 18v2h16v-2",
  warn: "M12 3l9 16H3zM12 9v5M12 17h.01",
  check: "M4 12l5 5L20 6",
  x: "M5 5l14 14M19 5L5 19",
  arrow: "M5 12h14M13 6l6 6-6 6",
  key: "M14 7a4 4 0 11-5 5l-6 6v3h3l1-1h2v-2h2l1.5-1.5A4 4 0 0014 7z",
  sort: "M3 6h12M3 12h8M3 18h4M17 15l3 3 3-3M20 6v12",
  dl: "M12 4v10M8 11l4 4 4-4M5 19h14",
  pin: "M12 2v7M8 9h8l-1 6H9zM12 15v7",
  drag: "M9 5h.01M9 12h.01M9 19h.01M15 5h.01M15 12h.01M15 19h.01",
  back: "M15 6l-6 6 6 6",
  refresh: "M20 11a8 8 0 10-.5 4M20 5v6h-6",
  pencil: "M4 20h4L19 9l-4-4L4 16zM14 6l4 4",
  ext: "M14 4h6v6M20 4l-9 9M18 14v6H4V6h6",
  layers: "M12 3l9 5-9 5-9-5zM3 13l9 5 9-5",
  qr: "M3 3h7v7H3zM14 3h7v7h-7zM3 14h7v7H3zM14 14h3v3h-3zM20 14v7M17 20h4",
  globe: "M12 3a9 9 0 100 18 9 9 0 000-18zM3 12h18M12 3c3 3 3 15 0 18M12 3c-3 3-3 15 0 18",
  terminal: "M4 5h16v14H4zM7 9l3 3-3 3M12.5 15H16",
  card: "M3 6h18v12H3zM3 10h18M6 15h4",
  note: "M5 3h11l3 3v15H5zM14 3v4h4M8 12h8M8 16h6",
  img: "M3 5h18v14H3zM3 16l5-5 4 4 3-3 6 6M9 9a1.5 1.5 0 11-3 0 1.5 1.5 0 013 0",
  trash: "M5 7h14M9 7V4h6v3M6 7l1 14h10l1-14",
  more: "M5 12h.01M12 12h.01M19 12h.01",
  gem: "M6 4h12l4 6-10 12L2 10zM2 10h20M7 4l5 18M17 4l-5 18M7 4l-5 6M17 4l5 6",
};

type IconName = keyof typeof ICONS | string;

export interface IconProps {
  name: IconName;
  size?: number;
  stroke?: number;
  fill?: boolean;
  style?: CSSProperties;
  className?: string;
}

export function Icon({ name, size = 16, stroke = 1.6, fill = false, style, className }: IconProps) {
  return (
    <svg
      width={size}
      height={size}
      viewBox="0 0 24 24"
      fill="none"
      stroke="currentColor"
      strokeWidth={stroke}
      strokeLinecap="square"
      strokeLinejoin="miter"
      className={className}
      style={{ display: "block", ...style }}
    >
      <path d={ICONS[name] || ""} fill={fill ? "currentColor" : "none"} />
    </svg>
  );
}
