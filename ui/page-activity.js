(() => {
  'use strict';
  let edited = false;
  for (const type of ['beforeinput', 'input', 'change']) {
    window.addEventListener(type, event => {
      if (event.isTrusted) edited = true;
    }, { capture: true, passive: true });
  }
  Object.defineProperty(window, '__liteRustReleasePosition', {
    value() {
      // Cross-origin documents and shadow components can hide their state.
      if (edited || document.readyState !== 'complete' ||
          window.onbeforeunload || document.designMode === 'on' ||
          document.querySelector('iframe, frame, portal, fencedframe, embed, object, canvas')) return null;
      const tree = document.createTreeWalker(document, NodeFilter.SHOW_ELEMENT);
      let inspected = 0;
      while (tree.nextNode()) {
        const element = tree.currentNode;
        if (++inspected > 12000 || element.shadowRoot || element.localName.includes('-')) return null;
      }
      for (const media of document.querySelectorAll('video, audio')) {
        if (!media.paused && !media.ended) return null;
      }
      for (const field of document.querySelectorAll('input, textarea, select')) {
        if (field instanceof HTMLSelectElement) {
          if ([...field.options].some(option => option.selected !== option.defaultSelected) &&
              (field.multiple || field.selectedIndex !== 0 || field.querySelector('[selected]'))) return null;
        } else if (field.type === 'checkbox' || field.type === 'radio') {
          if (field.checked !== field.defaultChecked) return null;
        } else if (field.value !== field.defaultValue) {
          return null;
        }
      }
      return [Math.round(window.scrollX), Math.round(window.scrollY)];
    },
    configurable: false,
    writable: false
  });
})();
