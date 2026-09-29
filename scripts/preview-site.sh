#!/usr/bin/env bash
# Builds the documentation site (docs/, published by GitHub Pages at https://mahfuzur.github.io/openvoicetype/) with the
# same github-pages gem GitHub uses, so what you see is what the merge publishes.
#
# Usage: scripts/preview-site.sh [--serve]
#   (none)    build into app/build/site/_site and check it: internal links, images, and the GitHub links the site sends
#             readers to (files outside docs/ and unpublished notes) must all exist
#   --serve   build, then serve it at http://127.0.0.1:4000/openvoicetype/ and rebuild on changes (Ctrl-C to stop)
#
# Needs Ruby 3 with Bundler. The gems install once into app/build/site/vendor (about 100 MB), not system-wide.

set -euo pipefail
export PATH="/opt/homebrew/bin:/usr/local/bin:$HOME/.local/bin:$PATH"

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
WORK="$ROOT/app/build/site"
SITE="$WORK/_site"

fail() { echo "preview-site: $*" >&2; exit 1; }

command -v bundle >/dev/null || fail "Ruby's Bundler is required (gem install bundler)"
mkdir -p "$WORK"
# github-pages pins Jekyll and every plugin to the versions GitHub runs. Ruby 3.4 no longer bundles csv, base64,
# bigdecimal and logger, which Jekyll 3.10 needs; webrick is for --serve.
cat >"$WORK/Gemfile" <<'EOF'
source "https://rubygems.org"
gem "github-pages", group: :jekyll_plugins
gem "webrick"
gem "csv"
gem "base64"
gem "bigdecimal"
gem "logger"
EOF
export BUNDLE_GEMFILE="$WORK/Gemfile" BUNDLE_PATH="$WORK/vendor"
# The GitHub metadata plugin asks GitHub's API for the latest release; without a token it only warns.
export JEKYLL_ENV=production
bundle check >/dev/null 2>&1 || bundle install --quiet || fail "bundle install failed"

jekyll() { bundle exec jekyll "$@" --source "$ROOT/docs" --destination "$SITE"; }

if [[ "${1:-}" == --serve ]]; then
  jekyll serve --host 127.0.0.1 --port 4000 --livereload
  exit 0
fi
[[ -z "${1:-}" ]] || fail "unknown option: $1"

jekyll build --strict_front_matter
node "$ROOT/scripts/check-site.mjs" "$SITE" "$ROOT"
