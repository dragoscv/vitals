#!/usr/bin/env node
// Keep the Home Assistant key pin from rotting.
//
// `crates/vitals-mqtt/tests/home_assistant.rs` pins every MQTT discovery
// document Vitals can publish against Home Assistant's *accepted* vocabulary,
// recorded in `crates/vitals-mqtt/tests/fixtures/ha_discovery_keys.json`. That
// fixture is a snapshot of a vocabulary Home Assistant changes - it ships
// monthly - and a key can be renamed or removed. When that happens the pin
// asserts something that is no longer true and *every test stays green*, which
// is the same failure mode as the 1.3.0 defect the pin exists for (three
// availability keys inside the `device` block, discarded silently by Home
// Assistant).
//
// This program re-derives the vocabulary and either reports the difference
// (`--check`) or writes the refreshed fixture (`--write`, the default). It is
// the method the fixture's own `_provenance` records: the schema objects are
// unwrapped and their keys printed **from inside the official container**, so
// the keys come from Home Assistant itself rather than from a copy of what
// Vitals sends. The container is chosen over a checked-out source tree because
// provenance was derived that way and because a container is the running
// consumer, not a reading of it; the derivation lives in `PYTHON_PROGRAM` below
// so it is reproducible from this file alone.
//
// Exit codes - the workflow depends on all three being distinct:
//   0  the fixture matches the current image (nothing to do)
//   1  the fixture is out of date (`--check` only; write mode turns this into 0)
//   2  the check could not run: no Docker, no image, no Home Assistant in it.
//      This is a failure, never a pass. A schedule that silently did nothing
//      would be the worst outcome, so there is no skip path here at all.
//
// Environment:
//   HA_PIN_IMAGE    image to enumerate (default the `stable` tag). A ref that
//                   already carries `@sha256:` is used as-is and never pulled.
//   HA_PIN_NO_PULL  set to 1 to enumerate whatever the ref resolves to locally
//                   (offline, or an image already pulled) instead of pulling.

import { spawnSync } from 'node:child_process';
import { readFileSync, writeFileSync } from 'node:fs';
import { dirname, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

const HERE = dirname(fileURLToPath(import.meta.url));
const FIXTURE = resolve(
  HERE,
  '..',
  'crates',
  'vitals-mqtt',
  'tests',
  'fixtures',
  'ha_discovery_keys.json',
);
const DEFAULT_IMAGE = 'ghcr.io/home-assistant/home-assistant:stable';

/// The top-level keys Home Assistant accepts in an MQTT discovery document,
/// per component, and the keys it accepts inside the `device` block.
///
/// `probatio` (Home Assistant's own schema library, which replaced voluptuous
/// here) keeps the accepted names as the keys of a `Schema.schema` mapping, so
/// a schema's vocabulary is simply those keys. A discovery entry point is often
/// a combinator (`probatio.All`) that composes one `Schema` with extra
/// validators, so the helper walks to the first `Schema` it can reach. If Home
/// Assistant restructures any of these names the import or the attribute lookup
/// raises, the container exits non-zero, and the workflow goes red rather than
/// quietly deriving nothing - which is the point.
const PYTHON_PROGRAM = `
import json
import sys

from probatio.schema import Schema

from homeassistant.components.mqtt import binary_sensor as mqtt_binary_sensor
from homeassistant.components.mqtt import schemas as mqtt_schemas
from homeassistant.components.mqtt import sensor as mqtt_sensor
import homeassistant.const as const


def first_schema(obj):
    """The probatio.Schema that holds the accepted keys, however it is wrapped."""
    if isinstance(obj, Schema):
        return obj
    for candidate in getattr(obj, "validators", []) or []:
        if isinstance(candidate, Schema):
            return candidate
    raise SystemExit("no probatio.Schema reachable from %r" % (obj,))


def keys(obj):
    # A marker (probatio.Optional/Required) compares and prints as its own
    # name, but str() is what makes that a promise rather than a coincidence.
    return sorted(str(key) for key in first_schema(obj).schema)


payload = {
    "version": getattr(const, "__version__", None),
    "components": {
        "sensor": keys(mqtt_sensor._PLATFORM_SCHEMA_BASE),
        "binary_sensor": keys(mqtt_binary_sensor.PLATFORM_SCHEMA_MODERN),
    },
    "device": keys(mqtt_schemas.MQTT_ENTITY_DEVICE_INFO_SCHEMA),
}

# Delimited, because importing Home Assistant can print to stdout and the JSON
# must still be findable.
print("__HA_PIN_JSON__")
json.dump(payload, sys.stdout, indent=2, sort_keys=True)
print("")
print("__HA_PIN_END__")
`;

/// A fatal, expected failure - a bad image, no Docker, no Home Assistant.
///
/// Thrown rather than `process.exit()`ed on purpose: `process.exit()` does not
/// wait for stdout already buffered to a pipe or a file, so it silently
/// truncates the output a moment before it exits. The workflow pipes this
/// program's output into the tracking issue, so a truncated diff is exactly the
/// wrong thing to hand it.
class Fatal extends Error {}

function die(message, code = 2) {
  const error = new Fatal(message);
  error.exitCode = code;
  throw error;
}

/// Run a command, returning `{ status, stdout, stderr }` rather than throwing,
/// so a missing Docker is a legible message instead of a stack trace.
function run(command, args, options = {}) {
  return spawnSync(command, args, { encoding: 'utf8', ...options });
}

function docker(args, options = {}) {
  const result = run('docker', args, options);
  if (result.error) {
    die(`cannot run docker (${result.error.message}) - is Docker installed and on PATH?`);
  }
  return result;
}

function resolveImage() {
  const image = process.env.HA_PIN_IMAGE?.trim() || DEFAULT_IMAGE;
  // A ref that already names a digest is a deliberate pin: never move it.
  return { image, pinned: image.includes('@sha256:') };
}

function ensureImage(image, pinned) {
  if (pinned) {
    console.log(`ha-pin-refresh: using the pinned image ${image}`);
    return;
  }
  if (process.env.HA_PIN_NO_PULL === '1') {
    console.log('ha-pin-refresh: HA_PIN_NO_PULL=1, enumerating the local image without pulling');
    return;
  }
  console.log(`ha-pin-refresh: pulling ${image}`);
  // Streamed, so a large pull is visible in the job log rather than silent.
  const pull = docker(['pull', image], { stdio: 'inherit' });
  if (pull.status !== 0) {
    die(`docker pull ${image} failed (exit ${pull.status})`);
  }
}

/// The image digest, as `<repo>@sha256:<digest>` - what `_provenance` records so
/// that a reviewer can tell exactly which bytes the key sets came from.
function imageDigest(image) {
  if (image.includes('@sha256:')) {
    return image;
  }
  const result = docker(['image', 'inspect', image, '--format', '{{json .RepoDigests}}']);
  if (result.status !== 0) {
    die(`docker image inspect ${image} failed: ${result.stderr.trim()}`);
  }
  let digests;
  try {
    digests = JSON.parse(result.stdout.trim() || '[]');
  } catch {
    die(`could not parse the RepoDigests of ${image}: ${result.stdout.trim()}`);
  }
  if (!Array.isArray(digests) || digests.length === 0) {
    die(
      `the image ${image} has no RepoDigest - it was built locally, not pulled, so its bytes ` +
        'cannot be named in the fixture provenance',
    );
  }
  return digests[0];
}

/// Boot the image, run the enumeration inside it, and read the result back.
///
/// `--entrypoint python3 ... -` runs the program from stdin. No `--pull`: the
/// image is already present (pulled above, or pinned), and a second pull inside
/// `docker run` would be a place for the two steps to disagree.
function enumerate(image) {
  const result = docker(['run', '--rm', '-i', '--entrypoint', 'python3', image, '-'], {
    input: PYTHON_PROGRAM,
    maxBuffer: 64 * 1024 * 1024,
  });
  if (result.status !== 0) {
    die(
      `the enumeration container exited ${result.status}; Home Assistant did not import or the ` +
        `schemas moved:\n${result.stderr.trim()}`,
    );
  }
  const start = result.stdout.indexOf('__HA_PIN_JSON__');
  const end = result.stdout.indexOf('__HA_PIN_END__');
  if (start === -1 || end === -1 || end < start) {
    die(
      'the enumeration printed no delimited result - it imported Home Assistant but produced no ' +
        `key sets. Output was:\n${result.stdout.trim()}`,
    );
  }
  const json = result.stdout.slice(start + '__HA_PIN_JSON__'.length, end).trim();
  let payload;
  try {
    payload = JSON.parse(json);
  } catch {
    die(`the enumeration produced unparseable JSON:\n${json}`);
  }
  for (const field of ['version', 'components', 'device']) {
    if (payload[field] === undefined || payload[field] === null) {
      die(`the enumeration omitted \`${field}\`: ${json}`);
    }
  }
  for (const component of ['sensor', 'binary_sensor']) {
    if (!Array.isArray(payload.components[component])) {
      die(`the enumeration omitted components.${component}: ${json}`);
    }
  }
  if (!Array.isArray(payload.device)) {
    die(`the enumeration omitted device: ${json}`);
  }
  return payload;
}

/// A sorted array of strings, as a set, for comparing a derived list to the
/// recorded one without depending on either being sorted.
function sameKeys(left, right) {
  if (left.length !== right.length) {
    return false;
  }
  const sorted = [...right].sort();
  return [...left].sort().every((value, index) => value === sorted[index]);
}

/// Every way the recorded fixture differs from what the image now accepts.
function differences(fixture, derived, image) {
  const lines = [];
  let changed = false;

  for (const component of ['sensor', 'binary_sensor']) {
    const recorded = (fixture.components?.[component] ?? []).map(String);
    const now = derived.components[component].map(String);
    if (sameKeys(recorded, now)) {
      continue;
    }
    changed = true;
    const added = now.filter((key) => !recorded.includes(key));
    const removed = recorded.filter((key) => !now.includes(key));
    lines.push(`components.${component}: +${added.length}/-${removed.length}`);
    if (added.length) {
      lines.push(`  added:   ${added.join(', ')}`);
    }
    if (removed.length) {
      lines.push(`  removed: ${removed.join(', ')}`);
    }
  }

  const recordedDevice = (fixture.device ?? []).map(String);
  const nowDevice = derived.device.map(String);
  if (!sameKeys(recordedDevice, nowDevice)) {
    changed = true;
    const added = nowDevice.filter((key) => !recordedDevice.includes(key));
    const removed = recordedDevice.filter((key) => !nowDevice.includes(key));
    lines.push(`device: +${added.length}/-${removed.length}`);
    if (added.length) {
      lines.push(`  added:   ${added.join(', ')}`);
    }
    if (removed.length) {
      lines.push(`  removed: ${removed.join(', ')}`);
    }
  }

  // The provenance names a specific image, so a moved `stable` makes the file
  // claim a version it was not derived from. That is out of date too: the fix
  // is one command, and the whole value of this file is that its provenance is
  // true. The `checked` date is deliberately not compared - it is always
  // today's.
  const recordedProvenance = fixture._provenance?.enumerated_with;
  const derivedProvenance = provenanceFor(image, derived.version);
  if (recordedProvenance !== derivedProvenance) {
    changed = true;
    lines.push('provenance.enumerated_with:');
    lines.push(`  recorded: ${recordedProvenance}`);
    lines.push(`  derived:  ${derivedProvenance}`);
  }

  return { changed, lines };
}

function provenanceFor(image, version) {
  return `${image} (Home Assistant ${version})`;
}

function today() {
  return new Date().toISOString().slice(0, 10);
}

function main() {
  const args = process.argv.slice(2);
  const known = new Set(['--check', '--write']);
  const unknown = args.filter((argument) => !known.has(argument));
  if (unknown.length) {
    die(`unknown argument ${unknown[0]} - expected --check or --write`);
  }
  const check = args.includes('--check');

  const { image, pinned } = resolveImage();
  ensureImage(image, pinned);
  const digest = imageDigest(image);
  // Enumerate from the digest, not the tag, so the bytes that produced the key
  // sets are the bytes the provenance names - a tag could move mid-run.
  const derived = enumerate(pinned ? image : digest);

  const fixture = JSON.parse(readFileSync(FIXTURE, 'utf8'));
  const { changed, lines } = differences(fixture, derived, digest);

  if (!changed) {
    console.log(`ha-pin-refresh: up to date (${provenanceFor(digest, derived.version)})`);
    return;
  }

  console.log('ha-pin-refresh: the fixture is out of date');
  for (const line of lines) {
    console.log(`  ${line}`);
  }

  if (check) {
    console.log('ha-pin-refresh: --check, so nothing was written; run without --check to refresh');
    // Set rather than call `process.exit(1)`, which would truncate the diff
    // just printed. See the note on `Fatal`.
    process.exitCode = 1;
    return;
  }

  // Only the vocabulary and the provenance move. Everything else in
  // `_provenance` - the files the keys live in, the docs link, the note - is
  // the reasoning a human wrote and is preserved byte for byte.
  const updated = {
    ...fixture,
    _provenance: {
      ...fixture._provenance,
      enumerated_with: provenanceFor(digest, derived.version),
      checked: today(),
    },
    components: {
      sensor: derived.components.sensor,
      binary_sensor: derived.components.binary_sensor,
    },
    device: derived.device,
  };
  writeFileSync(FIXTURE, `${JSON.stringify(updated, null, 2)}\n`);
  console.log(`ha-pin-refresh: wrote ${FIXTURE}`);
}

try {
  main();
} catch (error) {
  if (error instanceof Fatal) {
    console.error(`ha-pin-refresh: ${error.message}`);
    process.exitCode = error.exitCode;
  } else {
    throw error;
  }
}
