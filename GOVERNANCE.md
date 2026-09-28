# Governance

## Model

Vitals follows a benevolent dictator for life (BDFL) model. The maintainer,
Dragos Catalin Vladulescu ([@dragoscv](https://github.com/dragoscv)), has the
final say on what is merged, what is released and where the project goes.

This keeps decisions fast for a project with one maintainer. It does not mean
decisions are made without reasons: they are written down, and anyone can
challenge them.

## How decisions are made

- **Day-to-day changes** are decided in pull request review.
- **Architectural and policy decisions** are recorded as Architecture
  Decision Records in [`docs/adr`](docs/adr). An ADR states the context, the
  options considered, the decision and its consequences. To propose a change
  to one, open a discussion or a pull request adding a new ADR that
  supersedes it; ADRs are not edited to rewrite history.
- **Ideas and larger proposals** start in
  [GitHub Discussions](https://github.com/dragoscv/vitals/discussions) under
  Ideas, before any code is written.

## Releases

Releases are cut by the maintainer. The process, including versioning,
signing and publishing, is described in [`docs/releasing.md`](docs/releasing.md).

## Contributors

Anyone can contribute; see [CONTRIBUTING.md](CONTRIBUTING.md). Every
contribution requires a Developer Certificate of Origin sign-off. Contributors
are credited in [CONTRIBUTORS.md](CONTRIBUTORS.md).

A contributor with a sustained record of good reviews and changes may be
invited to become a co-maintainer with merge rights. If that happens, this
document will be updated to describe how maintainers share decisions.

## Code of conduct

Everyone taking part is expected to follow the
[Code of Conduct](CODE_OF_CONDUCT.md). The maintainer enforces it.

## Continuity

If the maintainer becomes unable to continue, the MIT licence ensures that
anyone can fork and carry on the project.
