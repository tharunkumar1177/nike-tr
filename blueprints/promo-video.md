# Promo video

## Outcome or responsibility

A 30-second, 1920×1080, 30 fps motion-graphics advertisement for Tron, built with Remotion in `promo/` and rendered to MP4 by GitHub Actions. The video is graphics only: schematic lines that draw themselves, a simulated step response, PCB traces routing at 45 degrees, the AI verification pipeline, and kinetic typography. It shows no app screenshots or illustrated UI, because the app has no shipped screens to show and mocked UI is not allowed.

## Current verified status

**Status:** Planned

Code exists in `promo/` and `.github/workflows/promo.yml`. No CI run has verified it yet.

## Architecture dependencies

- [Build and dependency policy](../architecture.md#build-and-dependency-policy)
- [Verification boundaries](../architecture.md#verification-boundaries)

## Local rules and implications

### Build and dependency policy

`promo/` is a standalone npm project with its own `package.json`, not a workspace of the root package. Remotion downloads a headless Chromium and a large dependency tree, and keeping it separate keeps the app's `npm ci` fast. When `promo/package-lock.json` is missing, the workflow runs `npm install` and uploads the result as the `promo-lockfile` artifact to commit. After that it installs with `npm ci`. Versions are pinned exactly and match the root project where the two overlap. TypeScript stays on 6.0.x for the same `typescript-eslint` peer-range reason recorded in [build-and-release](build-and-release.md).

`@remotion/eslint-plugin` is not used: it depends on `@typescript-eslint/utils` 5.x, whose peer range stops at ESLint 8, and the project uses ESLint 10.

### Verification boundaries

`.github/workflows/promo.yml` runs eslint, Prettier, and `tsc --noEmit`, then renders the composition. After rendering, it checks the MP4 with Remotion's bundled ffprobe: width 1920, height 1080, 30/1 fps, and 900 frames. It triggers only on changes under `promo/` or to the workflow itself. The root eslint and Prettier configs ignore `promo/`, so the app's CI never lints or builds it.

### Content rules

- The copy makes no unverifiable claims: no download links, pricing, or user counts. The call to action is the repository URL.
- The AI scene shows the documented pipeline (prompt, compile, ERC, simulation check) as plain text. It does not show circuit DSL source, because the DSL syntax is not defined yet.
- Colors come from the app tokens in `ui/src/styles.css`. Fonts are Inter and JetBrains Mono, loaded from Google Fonts through `@remotion/google-fonts` at render time.
- The video has no audio.
- Remotion is free for individuals and for companies of up to three people. Larger organizations need a Remotion company license.

## Related blueprints

### Required

None

### Impact checks

- [build-and-release](build-and-release.md) — when the Node version or the shared TypeScript and ESLint pins change.

## Relevant implementation and tests

- `promo/src/Root.tsx` — the `TronPromo` composition (1920×1080, 30 fps, 900 frames)
- `promo/src/Promo.tsx` — five 180-frame scenes in sequence
- `promo/src/scenes/` — per-scene timing and transitions
- `promo/src/components/` — procedural SVG graphics and animated text
- `promo/src/theme.ts` — color tokens, fonts, type scale, easing, and spring config
- `.github/workflows/promo.yml` — install, lint, format check, type-check, render, verify, upload `promo-video`

## Acceptance or verification criteria

- [ ] The promo package lints, passes the Prettier check, and type-checks in CI.
- [ ] CI renders `TronPromo` to an MP4 that ffprobe reports as 1920×1080, 30 fps, 900 frames.
- [ ] The MP4 is uploaded as the `promo-video` artifact.
- [ ] A run without a committed lockfile uploads `promo-lockfile`.
- [ ] The app's `CI` workflow stays green and doesn't lint or install `promo/`.

## Remaining gaps and unknowns

- No vertical (1080×1920) cut exists yet.
- Rendering depends on Google Fonts being reachable from the CI runner.
