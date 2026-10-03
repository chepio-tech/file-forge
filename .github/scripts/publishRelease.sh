#!/usr/bin/env bash
set -euo pipefail

: "${TAG:?TAG is required}"
: "${GITHUB_REPOSITORY:?GITHUB_REPOSITORY is required}"

version=$(sed -n 's/^version = "\(.*\)"$/\1/p' Cargo.toml | head -n 1)
if [ "$TAG" != "v$version" ]; then
  echo "::error::Tag $TAG does not match the Cargo.toml version $version"
  exit 1
fi

shopt -s nullglob
assets=(installers/*)
if [ "${#assets[@]}" -eq 0 ]; then
  echo "::error::No installers to publish"
  exit 1
fi

# Only an explicit 404 permits creation; authentication/network failures must stop publication.
if response=$(gh api --include --silent "repos/$GITHUB_REPOSITORY/releases/tags/$TAG"); then
  echo "::error::Release $TAG already exists. Publish a new version; existing releases are never overwritten."
  exit 1
else
  status=$(printf '%s\n' "$response" | awk 'NR == 1 { print $2 }')
  if [ "$status" != "404" ]; then
    echo "::error::Cannot confirm that release $TAG is absent; refusing to publish"
    exit 1
  fi
fi

# gh uploads assets to a draft before publishing, so immutable releases start with the complete set.
gh release create "$TAG" "${assets[@]}" --verify-tag --title "FileForge $TAG" \
  --notes "Installers for macOS, Windows and Linux. Builds are not signed or notarized yet; see the README for first-launch steps."
