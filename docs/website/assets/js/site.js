(function () {
  var THEME_KEY = "dolphin-theme";

  function preferredTheme() {
    try {
      var saved = localStorage.getItem(THEME_KEY);
      if (saved === "light" || saved === "dark") return saved;
    } catch (e) {}
    return window.matchMedia && window.matchMedia("(prefers-color-scheme: dark)").matches
      ? "dark"
      : "light";
  }

  function applyTheme(theme) {
    document.documentElement.setAttribute("data-theme", theme);
    document.querySelectorAll("[data-theme-toggle]").forEach(function (btn) {
      btn.textContent = theme === "dark" ? "☀" : "☾";
    });
  }

  function setupTheme() {
    applyTheme(preferredTheme());
    document.querySelectorAll("[data-theme-toggle]").forEach(function (btn) {
      btn.addEventListener("click", function () {
        var next =
          document.documentElement.getAttribute("data-theme") === "dark" ? "light" : "dark";
        try {
          localStorage.setItem(THEME_KEY, next);
        } catch (e) {}
        applyTheme(next);
      });
    });
  }

  function setupLang() {
    document.querySelectorAll("[data-lang-btn]").forEach(function (btn) {
      btn.addEventListener("click", function () {
        window.DolphinI18n.setLang(btn.getAttribute("data-lang-btn"));
      });
    });
  }

  function setupMenu() {
    var toggle = document.querySelector("[data-menu-toggle]");
    var links = document.querySelector(".nav-links");
    if (!toggle || !links) return;
    toggle.addEventListener("click", function () {
      links.classList.toggle("open");
    });
  }

  function highlightNav() {
    var path = window.location.pathname.split("/").pop();
    if (path === "" || path === "index.html") path = "index.html";
    document.querySelectorAll(".nav-links a[data-nav]").forEach(function (a) {
      if (a.getAttribute("href").split("/").pop() === path) a.classList.add("active");
    });
  }

  function setupYear() {
    document.querySelectorAll("[data-year]").forEach(function (el) {
      el.textContent = String(new Date().getFullYear());
    });
  }

  function setupTabs() {
    document.querySelectorAll("[data-tabs]").forEach(function (root) {
      var buttons = root.querySelectorAll(".tabs button");
      var panels = root.querySelectorAll(".tab-panel");
      buttons.forEach(function (btn) {
        btn.addEventListener("click", function () {
          buttons.forEach(function (b) {
            b.classList.remove("is-active");
          });
          panels.forEach(function (p) {
            p.classList.remove("is-active");
          });
          btn.classList.add("is-active");
          var target = root.querySelector("#" + btn.getAttribute("data-tab"));
          if (target) target.classList.add("is-active");
        });
      });
    });
  }

  function boot() {
    window.DolphinI18n.apply(window.DolphinI18n.lang);
    setupTheme();
    setupLang();
    setupMenu();
    setupTabs();
    highlightNav();
    setupYear();
  }

  if (document.readyState === "loading") {
    document.addEventListener("DOMContentLoaded", boot);
  } else {
    boot();
  }
})();
