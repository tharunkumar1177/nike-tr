# Promo video

## Outcome or responsibility

A 30-second, 1920×1080, 30 fps motion-graphics advertisement for Tron, built with Remotion in `promo/` and rendered to MP4 by GitHub Actions. The video is graphics only: schematic lines that draw themselves, a simulated step response, PCB traces routing at 45 degrees, the AI verification pipeline, and kinetic typography. It shows no app screenshots or illustrated UI, because the app has no shipped screens to show and mocked UI is not allowed.

The soundtrack is a Kokoro text-to-speech voiceover over a procedurally generated synth pad, both generated in CI.

## Current verified status

**Status:** Implemented. The loudness target is still open (see gaps).

The soundtrack was verified on 2026-10-13 by [run 38367970439](https://github.com/tharunkumar1177/tron/actions/runs/38367970439). Every line ended inside its scene: Schematic at frame 115, Simulation at 291, Layout at 495, AI at 681, and Outro at 860. The mix peaked at -2.7 dBFS. ffprobe found an AAC audio track, and ebur128 measured -21.6 LUFS integrated with a 9.6 LU loudness range. [Run 38366231020](https://github.com/tharunkumar1177/tron/actions/runs/38366231020) had failed the line-length check as intended, on a longer Outro line that ended at frame 900.

The video was verified on 2026-10-12 by these CI runs:

- [Run 38292312905](https://github.com/tharunkumar1177/tron/actions/runs/38292312905): bootstrap run. It generated and uploaded `promo-lockfile`, and a Prettier violation failed the run as intended.
- [Run 38293101023](https://github.com/tharunkumar1177/tron/actions/runs/38293101023): the format check failed and the diff step printed the exact Prettier changes.
- [Run 38293438957](https://github.com/tharunkumar1177/tron/actions/runs/38293438957): every step green on the committed lockfile. ffprobe reported `1920,1080,30/1,900`, and the `promo-video` artifact (about 618 KB) was uploaded.

## Architecture dependencies

- [Build and dependency policy](../architecture.md#build-and-dependency-policy)
- [Verification boundaries](../architecture.md#verification-boundaries)

## Local rules and implications

### Build and dependency policy

`promo/` is a standalone npm project with its own `package.json`, not a workspace of the root package. Remotion downloads a headless Chromium and a large dependency tree, and keeping it separate keeps the app's `npm ci` fast. When `promo/package-lock.json` is missing, the workflow runs `npm install` and uploads the result as the `promo-lockfile` artifact to commit. After that it installs with `npm ci`. Versions are pinned exactly and match the root project where the two overlap. TypeScript stays on 6.0.x for the same `typescript-eslint` peer-range reason recorded in [build-and-release](build-and-release.md).

The speech model is `onnx-community/Kokoro-82M-v1.0-ONNX` (Apache-2.0), run on the CPU through `kokoro-js` 1.2.1. `kokoro-js` has no revision option, so `audio/generate.js` sets `@huggingface/transformers` `env.remotePathTemplate` to the commit in `audio/model.json`. `@huggingface/transformers` is pinned to 3.8.1 so that it dedupes with the copy `kokoro-js` uses, which means one `env` controls both. The voice files ship inside the `kokoro-js` package, so the lockfile pins them. The model downloads into `promo/.cache/huggingface`, which CI caches with a key based on `audio/model.json`. The generated `public/soundtrack.wav` is not committed. Every run regenerates it.

`@remotion/eslint-plugin` is not used: it depends on `@typescript-eslint/utils` 5.x, whose peer range stops at ESLint 8, and the project uses ESLint 10.

### Verification boundaries

`.github/workflows/promo.yml` runs eslint, Prettier, and `tsc --noEmit`, then renders the composition. When the Prettier check fails, the next step runs `prettier --write` and prints the `git diff`, because no formatter runs locally. Before rendering, `npm run generate:audio` builds the soundtrack. It fails if a spoken line runs past its `endByFrame`, if a line comes out silent, or if the mix peaks above -1 dBFS. After rendering, the workflow checks the MP4 with Remotion's bundled ffprobe: width 1920, height 1080, 30/1 fps, 900 frames, and an AAC audio track. It then logs the EBU R128 loudness without enforcing a target. That measurement uses Ubuntu's `ffmpeg` package, because Remotion's bundled ffmpeg doesn't include the `ebur128` filter. It triggers only on changes under `promo/` or to the workflow itself. The root eslint and Prettier configs ignore `promo/`, so the app's CI never lints or builds it.

### Content rules

- The copy makes no unverifiable claims: no download links, pricing, or user counts. The call to action is the repository URL.
- The AI scene shows the documented pipeline (prompt, compile, ERC, simulation check) as plain text. It does not show circuit DSL source, because the DSL syntax is not defined yet.
- Colors come from the app tokens in `ui/src/styles.css`. Fonts are Inter and JetBrains Mono, loaded from Google Fonts through `@remotion/google-fonts` at render time.
- `audio/script.json` is the single source for the voiceover: one line per scene with its start frame and the frame it must end by, plus the voice, pad, ducking, and fade settings. Lines follow the same rule against unverifiable claims as the on-screen copy.
- `audio/generate.js` mixes everything into one mono 16-bit `public/soundtrack.wav`. Each voice line is normalized to a -3 dBFS peak. The pad is an F minor add9 chord (sine, detuned sine, and pulse-width-modulated square) through a swept 24 dB per octave low-pass, normalized to a -18 dBFS peak. Ducking lowers the pad by 6 dB while a line plays, with a 150 ms attack and a 500 ms release. The pad fades in over 60 frames and out over 90. `Promo.tsx` plays the file from frame 0.
- No licensed music or samples are used.
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
- `promo/audio/script.json`, `promo/audio/model.json`, `promo/audio/generate.js` — voiceover script, pinned speech model, and soundtrack generator
- `.github/workflows/promo.yml` — install, lint, format check, type-check, generate soundtrack, render, verify, report loudness, upload `promo-video` (MP4 and soundtrack WAV)

## Acceptance or verification criteria

- [x] The promo package lints, passes the Prettier check, and type-checks in CI.
- [x] CI renders `TronPromo` to an MP4 that ffprobe reports as 1920×1080, 30 fps, 900 frames.
- [x] The MP4 is uploaded as the `promo-video` artifact.
- [x] A run without a committed lockfile uploads `promo-lockfile`.
- [x] The app's `CI` workflow stays green and doesn't lint or install `promo/`. Verified by [run 38293438881](https://github.com/tharunkumar1177/tron/actions/runs/38293438881).
- [x] CI generates the soundtrack from the pinned model, and every line ends before its `endByFrame`.
- [x] The rendered MP4 has an AAC audio track, and the loudness is logged.
- [x] The `promo-video` artifact includes `soundtrack.wav`.

## Remaining gaps and unknowns

- No vertical (1080×1920) cut exists yet.
- CI checks the video's format but not how it looks. Frame-by-frame visual review is manual.
- Rendering depends on Google Fonts being reachable from the CI runner. Generating the soundtrack depends on Hugging Face being reachable whenever the model cache misses.
- CI checks that the audio exists, how long it is, its peak level, and its loudness. Pronunciation and the overall mix have to be checked by listening.
- Integrated loudness is -21.6 LUFS, below the designed -14 LUFS web target. Peak normalization (-3 dBFS voice) leaves the sparse voiceover quiet. Reaching -14 LUFS needs loudness normalization with a true-peak limiter in `audio/generate.js` or an ffmpeg `loudnorm` pass.
