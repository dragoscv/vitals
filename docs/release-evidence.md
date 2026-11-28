# Release evidence

One section per shipped version, newest first. Each section is written by
`scripts/release-evidence.mjs`, which reads the release run and its artefacts
from GitHub (read-only, through `gh`) and takes the channel readings from a
file another tool produced. Nothing on this page is a claim unless it says so:

- **VERIFIED** — the value was read, from GitHub (the run, its jobs, its
  artefacts) or from the channel readings file or from a file on disk, and its
  source is named in the row.
- **not read** — the value could not be read, and the reason is given. A
  `not read` row is **not** a pass; it is the honest statement that this
  evidence does not exist yet. The verdict at the foot of each section counts
  both, so the two can never be confused.

Nothing in a section is a claim about the channels when no readings file was
given; the section says so in its own words.

Regenerate one version's section — it replaces only that section and leaves
every other version untouched:

```text
node scripts/release-evidence.mjs --tag v1.5.2
```

Add the downloaded artefacts so their local SHA-256 is read from disk:

```text
node scripts/release-evidence.mjs --tag v1.5.2 -Assets .copilot-tmp/release-1-5-2/artifact
```

Offline, from a readings file that holds everything:

```text
node scripts/release-evidence.mjs --tag v1.5.2 -Channels readings.json --offline
```

## Vitals 1.5.1

_Generated 2026-11-28T04:16:02.874Z by `scripts/release-evidence.mjs` against run [46096442294](https://github.com/dragoscv/vitals/actions/runs/46096442294)._

_Reproduce with `node scripts/release-evidence.mjs --tag v1.5.1`._

### The run

| Field       | Value                                                       |
| ----------- | ----------------------------------------------------------- |
| Run id      | 46096442294                                                 |
| Run URL     | https://github.com/dragoscv/vitals/actions/runs/46096442294 |
| Event       | push                                                        |
| Status      | waiting                                                     |
| Conclusion  | not read — the run has not finished (status: waiting)       |
| Head sha    | 358f2d8686af9b142ebca3d8f81c2ebd41f8647b                    |
| Head branch | v1.5.1                                                      |
| Tag         | v1.5.1                                                      |
| Tag commit  | 358f2d8686af9b142ebca3d8f81c2ebd41f8647b                    |

> The run is `waiting`, so later jobs (the publish, readback and channel jobs) do not appear in the table below yet.

#### Jobs

| Job                     | Status    | Conclusion                                            |
| ----------------------- | --------- | ----------------------------------------------------- |
| Plan                    | completed | success                                               |
| Build (android)         | completed | success                                               |
| Verify                  | completed | success                                               |
| SBOM                    | completed | success                                               |
| Build (windows-arm64)   | completed | success                                               |
| Build (macos-universal) | completed | success                                               |
| Build (linux-x64)       | completed | success                                               |
| Build (windows-x64)     | completed | success                                               |
| Sign + notarise macOS   | waiting   | not read — the job has not finished (status: waiting) |
| Rehearse publish        | completed | skipped                                               |

### Artefacts

| Artefact               | Size (bytes) | GitHub digest                                                           | Local SHA-256                                                          |
| ---------------------- | ------------ | ----------------------------------------------------------------------- | ---------------------------------------------------------------------- |
| vitals-windows-x64     | 7828567      | sha256:1753c5d551af38f2cceb2dfdf659ac2f848dcc8b7fb2a08bab31094c73202d83 | not read — no -Assets directory was given, so nothing on disk was read |
| vitals-windows-arm64   | 6926521      | sha256:5431d85ca57c8698384f646248d7b7faeab7c18d132b6d3531ee6980feb1287e | not read — no -Assets directory was given, so nothing on disk was read |
| vitals-macos-universal | 13155218     | sha256:706544c8ac6f9b457655cc289c16c684e5a381d7ae95e28558b319103c57c53f | not read — no -Assets directory was given, so nothing on disk was read |
| vitals-linux-x64       | 103069502    | sha256:2bc9b778d5b3ae9fb81fde5797a50406ae80244457674e7a4e05314600d21352 | not read — no -Assets directory was given, so nothing on disk was read |
| android-unsigned       | 34870739     | sha256:af321e77626ec461cd776886b1ea51b3d24b0e41cf77dd0f3131faffaf884934 | not read — no -Assets directory was given, so nothing on disk was read |
| vitals-sbom            | 113010       | sha256:907480ed5658011826b89a7abfaaccf66f05bfb888cee298fea51e6cd4af20b2 | not read — no -Assets directory was given, so nothing on disk was read |

### Channels

No channel readings are available (no -Channels readings file was given), so **nothing about the channels is claimed** here: every row below is `not read`.

| Channel                       | Published version                 | What was read         | When                  | Command                       |
| ----------------------------- | --------------------------------- | --------------------- | --------------------- | ----------------------------- |
| GitHub release assets         | not read — no reading in the file | not read — no reading | not read — no reading | not read — no command was run |
| winget (dragoscv/winget-pkgs) | not read — no reading in the file | not read — no reading | not read — no reading | not read — no command was run |
| Scoop                         | not read — no reading in the file | not read — no reading | not read — no reading | not read — no command was run |
| Chocolatey                    | not read — no reading in the file | not read — no reading | not read — no reading | not read — no command was run |
| npm (@vitals-app/client)      | not read — no reading in the file | not read — no reading | not read — no reading | not read — no command was run |
| Google Play                   | not read — no reading in the file | not read — no reading | not read — no reading | not read — no command was run |

### Verdict

**45 claim(s) VERIFIED, 32 unread.** A `not read` row is not a pass: it means this sheet holds no evidence for it yet.
