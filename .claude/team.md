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
- pre_release_check: the `Full browser suite` workflow (`browser-full.yml`) must have
  passed on the release commit, on its first attempt (a re-run of failed jobs does
  not count). Dispatch it on that commit
  (`gh workflow run browser-full.yml --ref main`) and wait for it before tagging. The
  release workflow's `browser-gate` job fails the tag build if no passing run exists
  for the tagged commit.
- production_url: https://krausen.xyz
- Images are published to GHCR by the release workflow, then dispatched to the
  private `jasadams/krausen-cloud` deployment workflow. Production rollout is
  separately gated by that repository's configuration.
