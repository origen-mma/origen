window.MathJax = {
  tex: {
    inlineMath: [["\\(", "\\)"]],
    displayMath: [["\\[", "\\]"]],
    processEscapes: true,
    processEnvironments: true
  },
  options: {
    ignoreHtmlClass: ".*|",
    processHtmlClass: "arithmatex"
  },
  startup: {
    ready: function () {
      MathJax.startup.defaultReady();
      MathJax.startup.promise.then(function () {
        MathJax.typesetPromise();
      });
    }
  }
};

document$.subscribe(function () {
  if (typeof MathJax !== "undefined" && MathJax.typesetPromise) {
    MathJax.typesetPromise();
  }
});
