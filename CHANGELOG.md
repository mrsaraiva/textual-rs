# Changelog

All notable changes to this project will be documented in this file.

The format is based on Keep a Changelog, and this project follows SemVer-ish versioning
until the API stabilizes.

## [Unreleased]

### Added
- Synchronized output bracketing (DECSET 2026) around frame writes to prevent resize tearing / corruption. Disable with `TEXTUAL_SYNC_OUTPUT=0`.
- Disabled terminal line wrap while running in alt-screen mode (restored on exit) to reduce artifacts during heavy update bursts.

