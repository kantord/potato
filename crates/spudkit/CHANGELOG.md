# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [0.1.2](https://github.com/kantord/spudkit/compare/spudkit-v0.1.1...spudkit-v0.1.2) - 2026-03-30

### Added

- add chrome-based frontend

### Fixed

- buffer partial lines across Docker stream chunks in run() ([#57](https://github.com/kantord/spudkit/pull/57))

## [0.1.1](https://github.com/kantord/spudkit/compare/spudkit-v0.1.0...spudkit-v0.1.1) - 2026-03-29

### Added

- allow mounting data folders ([#54](https://github.com/kantord/spudkit/pull/54))
- allow listing installed spuds ([#50](https://github.com/kantord/spudkit/pull/50))
- use spud- prefix for spud images ([#49](https://github.com/kantord/spudkit/pull/49))

### Fixed

- auto-quote html in templates ([#56](https://github.com/kantord/spudkit/pull/56))
- *(deps)* pin rust crate dirs to =6.0.0 ([#55](https://github.com/kantord/spudkit/pull/55))
- *(deps)* update rust crate uuid to v1.23.0 ([#47](https://github.com/kantord/spudkit/pull/47))
- *(deps)* pin rust crate mime_guess to =2.0.5 ([#46](https://github.com/kantord/spudkit/pull/46))

### Other

- serve static files directly from the container ([#45](https://github.com/kantord/spudkit/pull/45))
- make container non-optional for all apps ([#44](https://github.com/kantord/spudkit/pull/44))
