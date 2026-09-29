#!/usr/bin/env node
// Checks the built documentation site (scripts/preview-site.sh runs it): every link and image between pages resolves,
// every #section exists, the links the site sends to GitHub (assets/site.js) point at files that exist in the repository,
// and no page loads anything from another host.
//
// Usage: node scripts/check-site.mjs <built site> <repository root>

import fs from "node:fs";
import path from "node:path";

const [site, repoRoot] = process.argv.slice(2);
if (!site || !repoRoot) {
  console.error("usage: check-site.mjs <built site> <repository root>");
  process.exit(2);
}
const base = "/openvoicetype";
const problems = [];
const pages = [];

// Every page in the site's layout (Archify's diagram pages are generated, and checked by render-diagrams.sh).
const isSitePage = (file) => fs.readFileSync(file, "utf8").includes(`data-baseurl="${base}"`);
(function walk(dir) {
  for (const entry of fs.readdirSync(dir, { withFileTypes: true })) {
    const full = path.join(dir, entry.name);
    if (entry.isDirectory()) walk(full);
    else if (entry.name.endsWith(".html") && isSitePage(full)) pages.push(full);
  }
})(site);

const idsCache = new Map();
function idsOf(file) {
  if (!idsCache.has(file)) {
    const html = fs.readFileSync(file, "utf8");
    idsCache.set(file, new Set([...html.matchAll(/\sid="([^"]+)"/g)].map((m) => m[1])));
  }
  return idsCache.get(file);
}

// The built file for a site path, or null.
function builtFile(pathname) {
  let rel = decodeURIComponent(pathname.slice(base.length)) || "/";
  let file = path.join(site, rel);
  if (rel.endsWith("/")) file = path.join(file, "index.html");
  if (fs.existsSync(file) && fs.statSync(file).isDirectory()) file = path.join(file, "index.html");
  return fs.existsSync(file) ? file : null;
}

let links = 0;
let github = 0;
for (const page of pages) {
  const html = fs.readFileSync(page, "utf8");
  const pageUrl = new URL("http://site" + base + "/" + path.relative(site, page).replace(/\\/g, "/"));
  const label = path.relative(site, page);

  // Anything loaded from another host.
  for (const m of html.matchAll(/<(script|img|iframe|source|link|video|audio)\b[^>]*?\s(src|srcset|href)="([^"]+)"/gi)) {
    const [, tag, attr, value] = m;
    if (tag.toLowerCase() === "link" && !/rel="(stylesheet|icon|preload|modulepreload)"/i.test(m[0])) continue;
    if (/^(https?:)?\/\//i.test(value)) problems.push(`${label}: <${tag} ${attr}> loads from another host: ${value}`);
  }
  if (/@import|url\(\s*["']?https?:/i.test(html)) problems.push(`${label}: CSS loads from another host`);

  for (const m of html.matchAll(/\s(href|src)="([^"]*)"/g)) {
    const value = m[2].replace(/&amp;/g, "&");
    if (!value || /^(?:[a-z][a-z0-9+.-]*:|\/\/)/i.test(value)) continue; // absolute URLs are the reader's to follow
    links++;
    const url = new URL(value, pageUrl);
    if (!url.pathname.startsWith(base + "/")) {
      // Leaves the site: assets/site.js opens it on GitHub, at that path in the repository.
      github++;
      const target = path.join(repoRoot, decodeURIComponent(url.pathname));
      if (!fs.existsSync(target)) problems.push(`${label}: ${value} → GitHub, but ${url.pathname} isn't in the repository`);
      continue;
    }
    if (/\.md$/i.test(url.pathname)) {
      // An unpublished note: opens on GitHub under docs/.
      github++;
      const target = path.join(repoRoot, "docs", decodeURIComponent(url.pathname.slice(base.length)));
      if (!fs.existsSync(target)) problems.push(`${label}: ${value} → GitHub, but docs${url.pathname.slice(base.length)} doesn't exist`);
      continue;
    }
    const file = builtFile(url.pathname);
    if (!file) {
      problems.push(`${label}: broken link ${value}`);
      continue;
    }
    if (url.hash && file.endsWith(".html") && isSitePage(file)) {
      const id = decodeURIComponent(url.hash.slice(1));
      if (id && !idsOf(file).has(id)) problems.push(`${label}: ${value}: no #${id} on that page`);
    }
  }
}

if (problems.length) {
  console.error(`check-site: ${problems.length} problem(s) in ${pages.length} pages:`);
  for (const p of [...new Set(problems)]) console.error("  " + p);
  process.exit(1);
}
console.log(`check-site: ${pages.length} pages, ${links} links and images checked (${github} open on GitHub), no third-party loads`);
