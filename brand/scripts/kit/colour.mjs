// OKLCH / hex → sRGB and WCAG 2.2 contrast, so the brand book prints ratios
// computed from the same strings contrast-pairs.json declares.

function oklchToLinear(L, C, hDeg) {
  const h = (hDeg * Math.PI) / 180;
  const a = C * Math.cos(h);
  const b = C * Math.sin(h);
  const l = (L + 0.3963377774 * a + 0.2158037573 * b) ** 3;
  const m = (L - 0.1055613458 * a - 0.0638541728 * b) ** 3;
  const s = (L - 0.0894841775 * a - 1.291485548 * b) ** 3;
  return [
    4.0767416621 * l - 3.3077115913 * m + 0.2309699292 * s,
    -1.2684380046 * l + 2.6097574011 * m - 0.3413193965 * s,
    -0.0041960863 * l - 0.7034186147 * m + 1.707614701 * s,
  ].map((x) => Math.max(0, Math.min(1, x)));
}

const toLinear = (c) => (c <= 0.04045 ? c / 12.92 : ((c + 0.055) / 1.055) ** 2.4);
const toGamma = (c) => (c <= 0.0031308 ? 12.92 * c : 1.055 * c ** (1 / 2.4) - 0.055);

/** Linear sRGB triple for `oklch(L C h)` or `#rrggbb`. */
export function linearRgb(css) {
  const s = css.trim();
  if (s.startsWith('#')) {
    const n = Number.parseInt(s.slice(1), 16);
    return [(n >> 16) & 255, (n >> 8) & 255, n & 255].map((x) => toLinear(x / 255));
  }
  const m = s.match(/oklch\(\s*([\d.]+)\s+([\d.]+)\s+([\d.]+)\s*\)/);
  if (!m) throw new Error(`unsupported colour ${css}`);
  return oklchToLinear(Number(m[1]), Number(m[2]), Number(m[3]));
}

export function hex(css) {
  return `#${linearRgb(css)
    .map((c) =>
      Math.round(toGamma(c) * 255)
        .toString(16)
        .padStart(2, '0'),
    )
    .join('')}`;
}

const luminance = (css) => {
  const [r, g, b] = linearRgb(css);
  return 0.2126 * r + 0.7152 * g + 0.0722 * b;
};

export function contrast(fg, bg) {
  const a = luminance(fg);
  const b = luminance(bg);
  return (Math.max(a, b) + 0.05) / (Math.min(a, b) + 0.05);
}

/** WCAG 2.2 AA threshold for the pair's declared use. */
export const threshold = (use) => (use === 'body' ? 4.5 : 3);
