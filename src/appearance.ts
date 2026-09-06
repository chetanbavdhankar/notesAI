import { useEffect } from "react";
import { defaultAppearance, type Appearance } from "./types";

export const palettes = [
  { id: "sage", name: "Sage", color: "#527a43" },
  { id: "blue", name: "Ocean", color: "#316ac3" },
  { id: "violet", name: "Violet", color: "#8052c7" },
  { id: "amber", name: "Amber", color: "#ae7021" },
  { id: "rose", name: "Rose", color: "#b34f75" },
  { id: "slate", name: "Slate", color: "#58677a" },
];
function hueSaturation(hex: string) {
  const [r, g, b] = [1, 3, 5].map(
    (i) => parseInt(hex.slice(i, i + 2), 16) / 255,
  );
  const max = Math.max(r, g, b),
    min = Math.min(r, g, b),
    d = max - min,
    l = (max + min) / 2;
  const hue =
    d === 0
      ? 0
      : max === r
        ? ((g - b) / d) % 6
        : max === g
          ? (b - r) / d + 2
          : (r - g) / d + 4;
  return {
    hue: (hue * 60 + 360) % 360,
    saturation: d === 0 ? 0 : (d / (1 - Math.abs(2 * l - 1))) * 100,
  };
}
export function useAppearance(appearance: Appearance = defaultAppearance) {
  useEffect(() => {
    const media = window.matchMedia("(prefers-color-scheme: dark)");
    const apply = () => {
      const chosen =
        appearance.palette === "custom"
          ? appearance.accent
          : palettes.find((p) => p.id === appearance.palette)?.color;
      const color =
        chosen && /^#[0-9a-f]{6}$/i.test(chosen)
          ? chosen
          : defaultAppearance.accent;
      const { hue, saturation } = hueSaturation(color);
      const root = document.documentElement;
      root.dataset.theme =
        appearance.mode === "system"
          ? media.matches
            ? "dark"
            : "light"
          : appearance.mode;
      root.dataset.palette = appearance.palette;
      root.style.setProperty("--hue", String(hue));
      root.style.setProperty("--saturation", `${Math.min(saturation, 70)}%`);
      root.style.setProperty(
        "--surface-saturation",
        `${Math.min(saturation / 3, 16)}%`,
      );
    };
    apply();
    media.addEventListener("change", apply);
    return () => media.removeEventListener("change", apply);
  }, [appearance.mode, appearance.palette, appearance.accent]);
}
