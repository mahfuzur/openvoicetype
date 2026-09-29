# The documentation website, Detailed Plan

**Goal:** https://mahfuzur.github.io/openvoicetype/ is a real documentation site: a landing page that says what the app
is and why it can be trusted, and the Guide, the diagrams, Claude's terms and the roadmap as readable pages. Today the
address shows GitHub's 404, because `docs/` has no `index.html`.

**Status legend:** ☐ to do · ◐ in progress · ☑ done

**Status: ☑ Done on `docs/website` (2026-09-29).** Notes from building it are in §8.

**Why it matters.** People decide in a minute whether to install a dictation app that can read what they say. The site
has to answer, on the first screen and in plain words: what it does, what leaves the Mac, how well it works (with the
numbers and how they were measured), and that everything can be checked, down to the line of code (the Archify diagrams).

## 1. Decisions

| Question | Decision |
|---|---|
| Hosting | GitHub Pages as it's set up now: `master`, folder `/docs`, GitHub's built-in Jekyll build. No workflow, no new setting |
| Generator | Jekyll with the plugins GitHub Pages enables by default (relative links, optional front matter, titles from headings, readme index). No custom plugins (GitHub's build doesn't run them) |
| Source of truth | The existing Markdown stays the source: `GUIDE.md`, `TERMS.md`, `ROADMAP.md`, `ARTWORK.md`, `diagrams/README.md` get the site's layout through `_config.yml` defaults, with no front matter added to them. They keep working on GitHub |
| New pages | Only the landing page (`index.html`). Its install, eval and privacy sections are short versions of the README's, which stays the full reference |
| Not published | `plans/` and `research/` (internal working notes). Links to them, and to files outside `docs/` (`../README.md`, `../prompts/`), open the file on GitHub instead (§3) |
| Design | Original, from the app's own artwork: the icon's ink (`#12121C`, `#2B2E42`) and its waveform's blue (`#59A6FF`) to violet (`#B88FFF`); light and dark themes; system fonts. Documentation layout: top bar, side navigation, an "On this page" list, readable line length. Works at phone width |
| Privacy of the site | No third-party anything: no web fonts, analytics, CDNs or embeds. Everything is served from the repository |
| Words | The project's rules apply: "works with your own Claude Code", never "free Claude" or "no limits"; "early software" and the Open Anyway step stay visible; S1-mini is credited as "S1-mini by Superwhisper" |
| Mobbin | Asked for, but no Mobbin MCP is connected in this session (it needs the maintainer's Mobbin account). The design follows common documentation-site patterns instead; a Mobbin pass can come later |

## 2. The landing page

1. **Hero:** the icon, the one-line promise, Download for Mac (latest release) and View on GitHub, and the facts that
   matter first: macOS 13.3+ on Apple Silicon, MIT, early software.
2. **How it feels:** press, speak, pasted, shown with the real overlay screenshots.
3. **What it does:** six feature cards (any app, keeps your meaning, Command Mode, modes, nothing lost, dictionary).
4. **See exactly how it works:** the six Archify diagrams as cards (light or dark screenshot to match the reader), each
   opening the interactive page, with the point that every box links to its line of code.
5. **How well it works:** the eval table, the speed numbers and the caveats, dated.
6. **Privacy:** what stays on the Mac, what is sent and to whom, and that the site itself loads nothing third-party.
7. **Install:** three steps, including Open Anyway, and a link to building from source.
8. **Footer:** credits (whisper.cpp, S1-mini by Superwhisper, llama.cpp, Archify), the license, Claude's terms.

## 3. Links

- Markdown links between published pages become `.html` links (GitHub's `jekyll-relative-links`).
- A small script in the layout (first-party, inline) sends every other repository link to GitHub: a link that resolves
  outside the site (`../README.md` from `GUIDE.md`) or to an unpublished `.md` file (`plans/M3-speed.md`) opens the same
  path under `github.com/mahfuzur/openvoicetype/blob/master/`. The site root is the repo's `docs/` folder, so the mapping
  is exact at any depth. Without JavaScript, only those links break; everything else is plain HTML.

## 4. Files

```
docs/_config.yml            site settings, layout defaults, excluded folders
docs/_layouts/default.html  top bar, footer, theme, the link script
docs/_layouts/doc.html      side navigation + content + "On this page"
docs/_data/nav.yml          the side navigation
docs/_includes/             head, header, footer pieces
docs/index.html             the landing page
docs/assets/site.css        the design (one file, no framework)
docs/assets/site.js         the link rewrite and the "On this page" list
scripts/preview-site.sh     builds (and serves) the site locally with the same github-pages gem GitHub uses
```

## 5. Tasks

| # | Task | Status |
|---|---|---|
| W1 | This plan | ☑ |
| W2 | `_config.yml`, layouts, includes, navigation, CSS, the link script | ☑ |
| W3 | The landing page | ☑ |
| W4 | `scripts/preview-site.sh` (bundler into `app/build/site`, `--serve`), shellcheck clean | ☑ |
| W5 | Checks: the site builds with the github-pages gem with no errors; every internal link and image resolves; every rewritten GitHub link points to a file that exists in the repo; no third-party requests; screenshots at 1440 and 390 px wide in light and dark, looked at and fixed | ☑ |
| W6 | Docs: README (link to the site), CONTRIBUTING (preview command), CLAUDE.md (Commands) | ☑ |

## 6. Acceptance criteria

- The root URL shows the landing page after merge; the Guide, How it works, Claude's terms, Roadmap and Artwork pages
  open from the navigation; the interactive diagrams open from their cards.
- No broken internal links or images in the local build.
- The site makes no request to any host but itself (GitHub links are ordinary links the reader clicks).
- Readable and usable at 390 px wide; both themes checked.
- The Markdown files still read correctly on GitHub.

## 7. Out of scope (later)

- A custom domain, search, versioned docs.
- A Mobbin-informed design pass (needs the Mobbin MCP set up with the maintainer's account).
- A CI job that builds the site on every PR.

## 8. Notes from building it (2026-09-29)

- **Same build as GitHub.** `scripts/preview-site.sh` installs the `github-pages` gem (Jekyll 3.10 and GitHub's plugin
  versions) into `app/build/site/vendor`, 88 MB, in about 25 s. Ruby 3.4 no longer bundles `csv`, `base64`, `bigdecimal`
  and `logger`, so the generated Gemfile adds them.
- **The checker** (`scripts/check-site.mjs`) found one real problem while building: its own filter skipped the diagrams
  index. Final run: 6 pages, 356 links and images, 23 of them opening on GitHub, all resolving; no third-party loads.
- **The overlay in the hero is HTML and CSS**, not the PNG screenshots: those have a light grey background that looked
  wrong on the dark hero, and the rebuilt pill can step through Recording, Transcribing, Polishing and Pasted.
  It stops moving with Reduce Motion.
- **Looked at, and fixed:** the step icons weren't aligned; the message field collapsed while the demo recorded (it now
  keeps its place with a placeholder); a diagram card's text sat lower than the others; "On this page" highlighted the
  wrong section (it now follows the scroll position). The home title no longer says "keeps your words yours", which
  overstates it when Claude is the engine.
- **Checked in a real browser (headless Chrome):** the GitHub links resolve (a folder link opens its tree view), the
  theme switch works and is remembered, and the demo runs through all four states. No horizontal scroll at 390 px.
