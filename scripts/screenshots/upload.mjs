// Copies a capture run to GCS using the VM's own service account (metadata
// server token), so the devcloud workspace needs no gcloud and no key file.
// Usage: node upload.mjs <dir> <bucket>   -> gs://<bucket>/shots/<stamp>/...
import { readdirSync, readFileSync, statSync } from 'node:fs';
import { join, relative } from 'node:path';

const [dir, bucket] = process.argv.slice(2);
const tok = await (
  await fetch(
    'http://metadata.google.internal/computeMetadata/v1/instance/service-accounts/default/token',
    { headers: { 'Metadata-Flavor': 'Google' }, signal: AbortSignal.timeout(10000) },
  )
).json();
const stamp = new Date().toISOString().replace(/[-:]/g, '').slice(0, 15);
const files = [];
const walk = (d) => {
  for (const n of readdirSync(d)) {
    const p = join(d, n);
    if (statSync(p).isDirectory()) walk(p);
    else files.push(p);
  }
};
walk(dir);
for (const f of files) {
  const name = `shots/${stamp}/${relative(dir, f).replaceAll('\\', '/')}`;
  const r = await fetch(
    `https://storage.googleapis.com/upload/storage/v1/b/${bucket}/o?uploadType=media&name=${encodeURIComponent(name)}`,
    {
      method: 'POST',
      body: readFileSync(f),
      headers: {
        Authorization: `Bearer ${tok.access_token}`,
        'Content-Type': 'application/octet-stream',
      },
      signal: AbortSignal.timeout(60000),
    },
  );
  if (!r.ok) throw new Error(`${name}: ${r.status} ${await r.text()}`);
}
console.log(`UPLOADED gs://${bucket}/shots/${stamp}/ (${files.length} files)`);
