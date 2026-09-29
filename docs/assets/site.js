// OpenVoiceType documentation site: theme switch, links to GitHub, "On this page", heading anchors, copy buttons and
// the demo in the hero. First-party and dependency-free; the pages work without it (only the GitHub links need it).
(() => {
  const root = document.documentElement;
  const base = root.dataset.baseurl || "";
  const repo = "https://github.com/mahfuzur/openvoicetype/blob/master";

  // --- Theme: follows the system until the reader picks one; the choice is remembered on this device only.
  const toggle = document.querySelector(".theme-toggle");
  const systemDark = window.matchMedia("(prefers-color-scheme: dark)");
  const current = () => root.dataset.theme || (systemDark.matches ? "dark" : "light");
  if (toggle) {
    toggle.addEventListener("click", () => {
      const next = current() === "dark" ? "light" : "dark";
      root.dataset.theme = next;
      try { localStorage.setItem("ovt-theme", next); } catch (e) { /* private window: not remembered */ }
    });
  }

  // --- Links to the repository. The site is the repository's docs/ folder, so a link that leaves the site
  // ("../README.md" from GUIDE.md) is that path in the repository, and a link to an unpublished note (plans/*.md)
  // is docs/<path>. Both open on GitHub.
  for (const a of document.querySelectorAll("a[href]")) {
    const raw = a.getAttribute("href");
    if (!raw || /^(?:[a-z][a-z0-9+.-]*:|#|\/\/)/i.test(raw)) continue;
    const url = new URL(raw, location.href);
    if (url.origin !== location.origin) continue;
    let target = null;
    if (!url.pathname.startsWith(base + "/")) target = repo + url.pathname;
    else if (/\.md$/i.test(url.pathname)) target = repo + "/docs" + url.pathname.slice(base.length);
    if (target) {
      a.href = target + url.hash;
      a.classList.add("to-github");
    }
  }

  // --- Heading anchors and "On this page", on documentation pages.
  const prose = document.querySelector(".prose");
  const toc = document.querySelector(".toc");
  if (prose) {
    const headings = [...prose.querySelectorAll("h2[id], h3[id]")];
    for (const h of headings) {
      const link = document.createElement("a");
      link.className = "anchor";
      link.href = "#" + h.id;
      link.setAttribute("aria-label", "Link to this section");
      link.textContent = "#";
      h.append(link);
    }
    if (toc && headings.length >= 3) {
      const list = toc.querySelector(".toc-list");
      const byId = new Map();
      for (const h of headings) {
        const a = document.createElement("a");
        a.href = "#" + h.id;
        a.textContent = h.firstChild ? h.firstChild.textContent.trim() : h.textContent;
        if (h.tagName === "H3") a.className = "toc-sub";
        list.append(a);
        byId.set(h.id, a);
      }
      toc.hidden = false;
      // Highlights the section being read: the last heading above the top of the window.
      let frame = 0;
      const update = () => {
        frame = 0;
        let active = headings[0];
        for (const h of headings) if (h.getBoundingClientRect().top < 120) active = h;
        for (const a of byId.values()) a.removeAttribute("aria-current");
        byId.get(active.id).setAttribute("aria-current", "true");
      };
      addEventListener("scroll", () => { frame ||= requestAnimationFrame(update); }, { passive: true });
      update();
    }

    // Copy buttons on code blocks.
    for (const pre of prose.querySelectorAll("pre")) {
      const button = document.createElement("button");
      button.type = "button";
      button.className = "copy";
      button.textContent = "Copy";
      button.addEventListener("click", async () => {
        try {
          await navigator.clipboard.writeText(pre.innerText.replace(/\n$/, ""));
          button.textContent = "Copied";
        } catch (e) {
          button.textContent = "Select and ⌘C";
        }
        setTimeout(() => { button.textContent = "Copy"; }, 1600);
      });
      pre.parentElement.classList.add("has-copy");
      pre.parentElement.append(button);
    }
  }

  // --- The hero demo: the overlay steps through a dictation, then the text appears.
  const demo = document.querySelector("[data-demo]");
  if (demo && !window.matchMedia("(prefers-reduced-motion: reduce)").matches) {
    const steps = [
      ["recording", 2600],
      ["transcribing", 900],
      ["polishing", 1100],
      ["pasted", 3400],
    ];
    let i = 0;
    const run = () => {
      const [state, ms] = steps[i];
      demo.dataset.state = state;
      i = (i + 1) % steps.length;
      setTimeout(run, ms);
    };
    run();
  }
})();
