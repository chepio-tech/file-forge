// Core
import type { SVGProps } from "react";

/**
 * Small stroke icon set drawn on a 16×16 grid with 1.5px strokes, so icons stay crisp at the sidebar size and
 * inherit `currentColor`. Keep additions in the same style.
 */
const PATHS = {
  compress: (
    <>
      <path d="M8 1.75v4.5M5.75 4 8 6.25 10.25 4" />
      <path d="M8 14.25v-4.5M5.75 12 8 9.75 10.25 12" />
      <path d="M2.75 8h10.5" />
    </>
  ),
  convert: (
    <>
      <path d="M2.75 5.25h9.5M9.75 2.75l2.5 2.5-2.5 2.5" />
      <path d="M13.25 10.75h-9.5M6.25 8.25l-2.5 2.5 2.5 2.5" />
    </>
  ),
  document: (
    <>
      <path d="M9.25 1.75h-4.5a1 1 0 0 0-1 1v10.5a1 1 0 0 0 1 1h6.5a1 1 0 0 0 1-1V4.75z" />
      <path d="M9.25 1.75v3h3" />
    </>
  ),
  plus: <path d="M8 3v10M3 8h10" />,
  check: <path d="M3.25 8.5 6.5 11.75l6.25-7.5" />,
  close: <path d="M4 4l8 8M12 4l-8 8" />,
  warning: (
    <>
      <path d="M8 2.25 1.75 13.25h12.5z" />
      <path d="M8 6.5v3M8 11.25v.01" />
    </>
  ),
  tray: (
    <>
      <path d="M8 1.75v7.5M5.25 6.5 8 9.25l2.75-2.75" />
      <path d="M1.75 9.75v2.5a1 1 0 0 0 1 1h10.5a1 1 0 0 0 1-1v-2.5" />
    </>
  ),
} as const;

export type IconName = keyof typeof PATHS;

interface IconProps extends Omit<SVGProps<SVGSVGElement>, "name"> {
  name: IconName;
  size?: number;
}

function Icon({ name, size = 16, ...rest }: IconProps) {
  return (
    <svg
      width={size}
      height={size}
      viewBox="0 0 16 16"
      fill="none"
      stroke="currentColor"
      strokeWidth={1.5}
      strokeLinecap="round"
      strokeLinejoin="round"
      aria-hidden="true"
      focusable="false"
      {...rest}
    >
      {PATHS[name]}
    </svg>
  );
}

export default Icon;
