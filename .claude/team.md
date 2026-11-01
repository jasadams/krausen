# Krausen release configuration

- team_key: KRA
- team_name: Krausen
- github_repo: jasadams/krausen

## Versioning

- method: date-based
- tag_format: vYYYY.MM.DD.N
- version_source: git-tag

## Release

- pipelines: Build and release Krausen
- production_url: https://krausen.xyz
- Images are published to GHCR by the release workflow, then dispatched to the
  private `jasadams/krausen-cloud` deployment workflow. Production rollout is
  separately gated by that repository's configuration.
