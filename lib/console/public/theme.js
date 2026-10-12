// License-Identifier: HGL
// Copyright (C) The Usecode Authors (see AUTHORS)

// Picks the theme before the page paints, so it never flashes: the one the
// visitor chose, else the system's. src/lib/theme.ts takes over from here.
(function () {
  var theme;
  try {
    theme = localStorage.getItem("theme");
  } catch (e) {}
  var dark = theme === "dark" || (theme !== "light" && matchMedia("(prefers-color-scheme: dark)").matches);
  document.documentElement.classList.toggle("dark", dark);
})();
