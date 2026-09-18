(function () {
  var DEFAULT_LANG = "zh-CN";
  var navEl = document.getElementById("docs-nav");
  var articleEl = document.getElementById("doc-article");
  var pagerEl = document.getElementById("doc-pager");
  var tocEl = document.getElementById("doc-toc-nav");
  var searchEl = document.getElementById("docs-search");

  function lang() {
    return (window.DolphinI18n && window.DolphinI18n.lang) || DEFAULT_LANG;
  }

  function contentFor(code) {
    var store = window.DolphinDocsContent || {};
    return store[code] || store[DEFAULT_LANG] || { groups: [] };
  }

  function flatPages(code) {
    var pages = [];
    contentFor(code).groups.forEach(function (group) {
      (group.pages || []).forEach(function (page) {
        pages.push({
          id: page.id,
          title: page.title,
          body: page.body,
          groupId: group.id,
          groupTitle: group.title
        });
      });
    });
    return pages;
  }

  function route() {
    var raw = (window.location.hash || "").replace(/^#\/?/, "").trim();
    return raw;
  }

  function goTo(id) {
    if (route() === id) render(id);
    else window.location.hash = "#/" + id;
  }

  function stripHtml(html) {
    var tmp = document.createElement("div");
    tmp.innerHTML = html;
    return (tmp.textContent || "").toLowerCase();
  }

  var searchIndex = {};

  function buildSearchIndex(code) {
    searchIndex[code] = flatPages(code).map(function (page) {
      return { id: page.id, title: page.title, text: stripHtml(page.body) };
    });
  }

  function renderSidebar(code, activeId) {
    var groups = contentFor(code).groups;
    var html = "";
    groups.forEach(function (group) {
      html += '<div class="docs-nav-group" data-group="' + group.id + '">';
      html += '<div class="group-title">' + group.title + "</div>";
      (group.pages || []).forEach(function (page) {
        html +=
          '<a href="#/' +
          page.id +
          '" data-page-id="' +
          page.id +
          '"' +
          (page.id === activeId ? ' class="active"' : "") +
          ">" +
          page.title +
          "</a>";
      });
      html += "</div>";
    });
    navEl.innerHTML = html;
  }

  function buildToc(article) {
    var headings = article.querySelectorAll("h2, h3");
    var html = "";
    headings.forEach(function (heading, index) {
      var id = "sec-" + index;
      heading.id = id;
      html +=
        '<a class="level-' +
        heading.tagName.substr(1) +
        '" href="#' +
        id +
        '">' +
        heading.textContent +
        "</a>";
    });
    tocEl.innerHTML = html;
  }

  function renderPager(code, page, pages) {
    var index = pages.findIndex(function (p) {
      return p.id === page.id;
    });
    var prev = index > 0 ? pages[index - 1] : null;
    var next = index >= 0 && index < pages.length - 1 ? pages[index + 1] : null;
    var html = "";
    html += prev
      ? '<a class="prev" href="#/' +
        prev.id +
        '"><span class="dir">' +
        window.DolphinI18n.t("docs.prev") +
        "</span>" +
        prev.title +
        "</a>"
      : "<span></span>";
    html += next
      ? '<a class="next" href="#/' +
        next.id +
        '"><span class="dir">' +
        window.DolphinI18n.t("docs.next") +
        "</span>" +
        next.title +
        "</a>"
      : "<span></span>";
    pagerEl.innerHTML = html;
  }

  function render(id) {
    var code = lang();
    var pages = flatPages(code);
    if (!pages.length) {
      articleEl.innerHTML = '<p class="docs-empty">' + window.DolphinI18n.t("docs.loading") + "</p>";
      return;
    }
    var page =
      pages.find(function (p) {
        return p.id === id;
      }) || null;

    if (!page) {
      articleEl.innerHTML =
        '<h1 data-i18n="docs.notfound">' +
        window.DolphinI18n.t("docs.notfound") +
        '</h1><p><a href="#/' +
        pages[0].id +
        '">' +
        pages[0].title +
        "</a></p>";
      pagerEl.innerHTML = "";
      tocEl.innerHTML = "";
      renderSidebar(code, null);
      document.title = "Dolphin Docs";
      return;
    }

    renderSidebar(code, page.id);
    articleEl.innerHTML = page.body;
    buildToc(articleEl);
    renderPager(code, page, pages);
    document.title = page.title + " · Dolphin";
    window.scrollTo({ top: 0, behavior: "auto" });
  }

  function onRoute() {
    var id = route();
    if (!id) {
      var pages = flatPages(lang());
      if (pages.length) {
        window.location.hash = "#/" + pages[0].id;
        return;
      }
    }
    render(id);
  }

  function applySearch(term) {
    var code = lang();
    var query = term.trim().toLowerCase();
    var index = searchIndex[code] || [];
    var matched = null;
    if (query) {
      matched = {};
      index.forEach(function (item) {
        if (item.title.toLowerCase().indexOf(query) !== -1 || item.text.indexOf(query) !== -1) {
          matched[item.id] = true;
        }
      });
    }
    navEl.querySelectorAll("a[data-page-id]").forEach(function (a) {
      var show = !query || (matched && matched[a.getAttribute("data-page-id")]);
      a.classList.toggle("hidden", !show);
    });
    navEl.querySelectorAll(".docs-nav-group").forEach(function (group) {
      var any = false;
      group.querySelectorAll("a[data-page-id]").forEach(function (a) {
        if (!a.classList.contains("hidden")) any = true;
      });
      group.style.display = any ? "" : "none";
    });
  }

  function boot() {
    var code = lang();
    buildSearchIndex(code);
    onRoute();
    window.addEventListener("hashchange", onRoute);
    document.addEventListener("dolphin:langchange", function () {
      var c = lang();
      buildSearchIndex(c);
      if (searchEl) searchEl.value = "";
      onRoute();
      var active = navEl.querySelector("a.active");
      if (active) active.classList.add("active");
    });
    if (searchEl) {
      searchEl.addEventListener("input", function () {
        applySearch(searchEl.value);
      });
    }
  }

  if (document.readyState === "loading") {
    document.addEventListener("DOMContentLoaded", boot);
  } else {
    boot();
  }
})();
