# README images

The download buttons are original text-only layouts: `Download` and the platform name on a dark rounded surface.
They contain no platform artwork or download-arrow icons.

Each transparent PNG is 780 × 240 pixels and appears at 260 × 80 pixels in the README.
The README uses relative image paths inside ordinary links, supported by GitHub's Markdown renderer.
Each link downloads that platform's default installer from the latest GitHub release (ADR-0009).

Validate image files, labels and links with `pnpm vitest run docs/readmeDownloads.test.ts`.
