# ADR-0010: Published release assets and tags are immutable

## Status
Accepted; supersedes ADR-0009's replacement of assets when a tag is rerun. Its stable installer names, download
URLs and native-runner builds remain in force. ADR-0013 adds updater assets to each release.

## Date
2026-10-03

## Context
Stable README download links must keep serving the intended version's installers. ADR-0009 allowed rerunning a
tag to replace its assets, which could silently change a binary after someone downloaded or verified it.
GitHub supports immutable releases for this repository, including release attestations, without changing visibility.

## Decision
Enable repository release immutability before the first publication. Create releases through `gh release create`,
which uploads assets to a draft before publishing. Published assets cannot be replaced or individually deleted;
their tag cannot move or be deleted while the release exists. Release titles, notes and latest/pre-release metadata
can still be edited.

The publication script refuses any existing release or draft and never requests asset replacement. Only an
explicit `404` from the release lookup permits creation; other API failures stop the job. Fix published binaries
with a new patch version and tag. An interrupted draft requires review before cleanup or retry.

## Rationale
Immutability preserves the identity of downloaded binaries and prevents silent asset substitution. GitHub creates
an attestation linking the tag, commit and assets. Uploading before publication keeps the initial asset set complete.

## Alternatives considered
- Replace assets on a repeated tag: silently changes previously published content and conflicts with immutability.
- Publish a draft manually every time: adds a manual step but does not itself prevent later asset replacement.
- Keep immutable versioned installers in another storage service: adds infrastructure without a current requirement.

## Consequences
- A bad published installer requires a new version; the same tag cannot be repurposed to repair it.
- Rerunning a published tag fails without overwriting assets. Review leftover drafts from interrupted publication.
- Stable README URLs continue to follow the release marked Latest, as in ADR-0009.
- Immutability does not restrict creation of new release tags. The private repository's GitHub Free plan does not
  provide the planned tag ruleset; authorized maintainers must review tag pushes.
- Disabling the policy is not a promise that already published assets can be replaced or tag names reused.

## Validation / fitness criteria
- `GET /repos/chepio-tech/file-forge/immutable-releases` reports `enabled: true`.
- `pnpm vitest run docs/workflowSecurity.test.ts docs/readmeDownloads.test.ts` passes. Publication tests use a
  local CLI stub to verify refusal of existing releases and API errors without any external writes.
- After an approved publication, GitHub reports an immutable release with the expected seven installers.

## Reconsider when
- Signing or automatic updates need additional assets: attach them before publication and update the release tests.
- Distribution moves to a different repository or service: retain the immutable identity of released versions.

## References
- https://docs.github.com/en/code-security/concepts/supply-chain-security/immutable-releases
- https://docs.github.com/en/code-security/how-tos/secure-your-supply-chain/establish-provenance-and-integrity/prevent-release-changes
- https://cli.github.com/manual/gh_release_create
