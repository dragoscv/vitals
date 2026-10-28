#!/usr/bin/env node
// Builds the iPhone app's labelled "Sample PC" data from the contract
// keyframe Rust writes (crates/vitals-server/tests/android_contract.rs), so
// the sample always decodes with the current wire model. Adds a believable
// process list and a day of history; nothing here is presented as real.
import { readFileSync, writeFileSync, mkdirSync } from 'node:fs';
import { dirname, join } from 'node:path';
import { fileURLToPath } from 'node:url';

const root = join(dirname(fileURLToPath(import.meta.url)), '..');
const src = join(root, 'apps/ios/VitalsCore/Tests/VitalsCoreTests/Contract/keyframe_full.json');
const outDir = join(root, 'apps/ios/Vitals/Resources');
mkdirSync(outDir, { recursive: true });

const frame = JSON.parse(readFileSync(src, 'utf8'));
const sys = frame.payload.system;
sys.cpu.perCore = [28, 31, 19, 44, 22, 25, 37, 18, 21, 33, 26, 29, 17, 40, 24, 23];
sys.cpu.total = 27.3;
sys.cpu.throttled = null;
sys.battery = null;
sys.memory.total = 34359738368;
sys.memory.used = 15032385536;
sys.memory.available = 19327352832;
sys.networks[0].mac = null; // no wake-up target in the sample

const base = frame.payload.processes[0];
const names = [
  ['chrome.exe', 'Google Chrome', 6.2, 1.9e9],
  ['Code.exe', 'Visual Studio Code', 3.1, 1.2e9],
  ['explorer.exe', 'Windows Explorer', 0.4, 2.1e8],
  ['Spotify.exe', 'Spotify', 0.9, 3.4e8],
  ['Teams.exe', 'Microsoft Teams', 1.6, 6.8e8],
  ['vitals-desktop.exe', 'Vitals', 0.7, 1.1e8],
  ['steam.exe', 'Steam', 0.3, 2.6e8],
  ['OneDrive.exe', 'Microsoft OneDrive', 0.1, 9.0e7],
  ['MsMpEng.exe', 'Antimalware Service Executable', 1.2, 2.9e8],
  ['dwm.exe', 'Desktop Window Manager', 1.8, 1.5e8],
];
frame.payload.processes = names.map(([name, description, cpu, mem], i) => ({
  ...base,
  key: { pid: 4100 + i * 37, startTime: 133700000000000000 + i },
  parent: null,
  name,
  description,
  kind: i < 7 ? 'app' : 'background',
  state: 'running',
  flags: 0,
  protection: 'none',
  cpu,
  memoryPrivate: Math.round(mem),
  memoryWorkingSet: Math.round(mem * 1.2),
  diskRead: 0,
  diskWrite: 0,
  gpu: null,
  gpuMemory: null,
  user: 'Sample',
}));
writeFileSync(join(outDir, 'sample-keyframe.json'), JSON.stringify(frame, null, 2) + '\n');

const now = 1700000000;
const history = [];
for (let i = 0; i < 288; i++) {
  const t = now - (287 - i) * 300;
  const cpu = 18 + 12 * Math.sin(i / 20) + (i % 7);
  history.push({
    ts: t,
    cpuPercent: cpu,
    cpuKernelPercent: cpu / 6,
    memoryUsed: 14e9 + 2e9 * Math.sin(i / 40),
    memoryTotal: 34359738368,
    diskReadBps: 2e6,
    diskWriteBps: 1e6,
    netRxBps: 3e5,
    netTxBps: 1e5,
    gpuPercent: 10 + 8 * Math.sin(i / 15),
    cpuTempC: 52 + 10 * Math.sin(i / 20),
    powerDrawW: null,
  });
}
writeFileSync(join(outDir, 'sample-history.json'), JSON.stringify(history) + '\n');
console.log('wrote sample-keyframe.json + sample-history.json');
