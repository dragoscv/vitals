# Licence texts

Standard texts from the [SPDX licence list](https://github.com/spdx/license-list-data/tree/main/text),
one file per SPDX id. `scripts/third-party-notices.ps1` copies the text of every licence a shipped
dependency names into `THIRD_PARTY_NOTICES.md`, which the installer puts beside the app. A
dependency under a licence with no file here fails that script: read the licence, then add its
SPDX text here (and to `deny.toml`).
