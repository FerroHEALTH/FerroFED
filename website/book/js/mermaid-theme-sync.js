// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1
//
// Redraws the diagrams when the book's theme moves between light and dark by
// a route the vendored mermaid-init.js does not watch. That script draws in
// mermaid's dark theme under ayu, navy and coal and in its default theme
// otherwise, chosen once from the class mdBook sets on <html>, and reloads the
// page on a click of a named theme. The "Auto" choice, and a change of the
// system colour scheme while Auto is in force, change that class with no such
// click, and the diagrams would keep the colours of the other theme. This
// script reloads the page whenever the class crosses between light and dark.
// No specification governs this: our own design.
(() => {
  const darkThemes = ['ayu', 'navy', 'coal'];
  const html = document.documentElement;
  if (document.querySelector('.mermaid') === null) {
    return;
  }
  const isDark = () => darkThemes.some((theme) => html.classList.contains(theme));
  const drawnDark = isDark();
  new MutationObserver(() => {
    if (isDark() !== drawnDark) {
      window.location.reload();
    }
  }).observe(html, { attributes: true, attributeFilter: ['class'] });
})();
