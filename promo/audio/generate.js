import { mkdir, readFile, writeFile } from "node:fs/promises";
import { fileURLToPath } from "node:url";
import { env } from "@huggingface/transformers";
import { KokoroTTS } from "kokoro-js";

const PAD_NOTES_HZ = [174.61, 207.65, 261.63, 392];
const MIX_CEILING_DBFS = -1;

async function readJson(name) {
  const text = await readFile(new URL(name, import.meta.url), "utf8");
  return JSON.parse(text);
}

const script = await readJson("./script.json");
const model = await readJson("./model.json");
const outputUrl = new URL("../public/soundtrack.wav", import.meta.url);

function dbToGain(db) {
  return 10 ** (db / 20);
}

function gainToDb(gain) {
  return 20 * Math.log10(gain);
}

function peakOf(samples) {
  let peak = 0;
  for (const sample of samples) {
    peak = Math.max(peak, Math.abs(sample));
  }
  return peak;
}

function smoothStep(value) {
  return value * value * (3 - 2 * value);
}

function smoothing(ms, sampleRate) {
  return Math.exp(-1 / ((ms / 1000) * sampleRate));
}

function framesToSamples(frames, sampleRate) {
  return Math.round((frames / script.fps) * sampleRate);
}

async function synthesizeVoice() {
  env.cacheDir = fileURLToPath(
    new URL("../.cache/huggingface/", import.meta.url),
  );
  env.remotePathTemplate = `{model}/resolve/${model.revision}/`;
  const tts = await KokoroTTS.from_pretrained(model.id, {
    dtype: model.dtype,
    device: "cpu",
  });
  const clips = [];
  for (const line of script.lines) {
    const audio = await tts.generate(line.text, {
      voice: script.voice.id,
      speed: script.voice.speed,
    });
    clips.push({
      line,
      samples: audio.audio,
      sampleRate: audio.sampling_rate,
    });
  }
  return clips;
}

function placeVoice(clips, sampleRate, totalSamples) {
  const track = new Float32Array(totalSamples);
  const speaking = new Uint8Array(totalSamples);
  const targetPeak = dbToGain(script.voice.peakDbfs);
  for (const { line, samples } of clips) {
    const start = framesToSamples(line.startFrame, sampleRate);
    const end = start + samples.length;
    const endFrame = Math.ceil((end / sampleRate) * script.fps);
    const seconds = (samples.length / sampleRate).toFixed(2);
    const timing = `${line.scene}: ${seconds} s, frames ${line.startFrame}-${endFrame}`;
    console.log(timing);
    if (end > framesToSamples(line.endByFrame, sampleRate)) {
      const message = `${line.scene} line ends at frame ${endFrame}, past ${line.endByFrame}. Shorten "${line.text}".`;
      throw new Error(message);
    }
    const peak = peakOf(samples);
    if (peak === 0) {
      throw new Error(`${line.scene} line produced silence.`);
    }
    const gain = targetPeak / peak;
    for (let index = 0; index < samples.length; index += 1) {
      track[start + index] += samples[index] * gain;
      speaking[start + index] = 1;
    }
  }
  return { track, speaking };
}

function synthesizePad(sampleRate, totalSamples) {
  const pad = new Float32Array(totalSamples);
  const filter = [0, 0, 0, 0];
  for (let index = 0; index < totalSamples; index += 1) {
    const time = index / sampleRate;
    const duty = 0.5 + 0.2 * Math.sin(2 * Math.PI * 0.07 * time);
    let sample = 0;
    PAD_NOTES_HZ.forEach((frequency, note) => {
      const phase = frequency * time + note * 0.25;
      const fraction = phase % 1;
      sample += Math.sin(2 * Math.PI * phase);
      sample += 0.5 * Math.sin(2 * Math.PI * phase * 1.003);
      sample += 0.35 * (fraction < duty ? 1 : -1);
    });
    const sweep = 2 ** (0.35 * Math.sin(2 * Math.PI * 0.05 * time));
    const alpha = 1 - Math.exp((-2 * Math.PI * 400 * sweep) / sampleRate);
    for (let stage = 0; stage < filter.length; stage += 1) {
      filter[stage] += alpha * (sample - filter[stage]);
      sample = filter[stage];
    }
    pad[index] = sample;
  }

  const gain = dbToGain(script.pad.peakDbfs) / peakOf(pad);
  const fadeIn = framesToSamples(script.pad.fadeInFrames, sampleRate);
  const fadeOut = framesToSamples(script.pad.fadeOutFrames, sampleRate);
  for (let index = 0; index < totalSamples; index += 1) {
    const rise = Math.min(1, index / fadeIn);
    const fall = Math.min(1, (totalSamples - 1 - index) / fadeOut);
    pad[index] *= gain * smoothStep(rise) * smoothStep(fall);
  }
  return pad;
}

function duckEnvelope(speaking, sampleRate) {
  const envelope = new Float32Array(speaking.length);
  const ducked = dbToGain(script.pad.duckDb);
  const attack = smoothing(script.pad.duckAttackMs, sampleRate);
  const release = smoothing(script.pad.duckReleaseMs, sampleRate);
  let gain = 1;
  for (let index = 0; index < speaking.length; index += 1) {
    const target = speaking[index] ? ducked : 1;
    const coefficient = target < gain ? attack : release;
    gain = target + coefficient * (gain - target);
    envelope[index] = gain;
  }
  return envelope;
}

function encodeWav(samples, sampleRate) {
  const dataBytes = samples.length * 2;
  const buffer = Buffer.alloc(44 + dataBytes);
  buffer.write("RIFF", 0, "ascii");
  buffer.writeUInt32LE(36 + dataBytes, 4);
  buffer.write("WAVE", 8, "ascii");
  buffer.write("fmt ", 12, "ascii");
  buffer.writeUInt32LE(16, 16);
  buffer.writeUInt16LE(1, 20);
  buffer.writeUInt16LE(1, 22);
  buffer.writeUInt32LE(sampleRate, 24);
  buffer.writeUInt32LE(sampleRate * 2, 28);
  buffer.writeUInt16LE(2, 32);
  buffer.writeUInt16LE(16, 34);
  buffer.write("data", 36, "ascii");
  buffer.writeUInt32LE(dataBytes, 40);
  samples.forEach((sample, index) => {
    const clamped = Math.max(-1, Math.min(1, sample));
    buffer.writeInt16LE(Math.round(clamped * 32767), 44 + index * 2);
  });
  return buffer;
}

async function main() {
  const clips = await synthesizeVoice();
  const sampleRate = clips[0].sampleRate;
  if (clips.some((clip) => clip.sampleRate !== sampleRate)) {
    throw new Error("Voice clips have different sample rates.");
  }

  const totalSamples = framesToSamples(script.durationInFrames, sampleRate);
  const { track, speaking } = placeVoice(clips, sampleRate, totalSamples);
  const pad = synthesizePad(sampleRate, totalSamples);
  const envelope = duckEnvelope(speaking, sampleRate);
  const mix = new Float32Array(totalSamples);
  for (let index = 0; index < totalSamples; index += 1) {
    mix[index] = track[index] + pad[index] * envelope[index];
  }

  const peakDb = gainToDb(peakOf(mix));
  if (peakDb > MIX_CEILING_DBFS) {
    const message = `Mix peaks at ${peakDb.toFixed(1)} dBFS, above ${MIX_CEILING_DBFS} dBFS.`;
    throw new Error(message);
  }

  await mkdir(new URL(".", outputUrl), { recursive: true });
  await writeFile(outputUrl, encodeWav(mix, sampleRate));
  const seconds = (totalSamples / sampleRate).toFixed(2);
  const summary = `Soundtrack: ${seconds} s at ${sampleRate} Hz, peak ${peakDb.toFixed(1)} dBFS`;
  console.log(summary);
}

// The phonemizer installs an uncaughtException handler that prints its own
// bundled source instead of the error, so failures are reported here.
try {
  await main();
} catch (error) {
  console.error(error instanceof Error ? error.message : error);
  process.exit(1);
}
