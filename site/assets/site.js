(() => {
  const root = document.documentElement;
  const toggle = document.querySelector('#theme-toggle');
  const meta = document.querySelector('meta[name="theme-color"]');

  function applyTheme(theme) {
    root.dataset.theme = theme;
    try { localStorage.setItem('notoffice-theme', theme); } catch (_) {}
    if (meta) meta.setAttribute('content', theme === 'dark' ? '#171411' : '#f4f0e8');
  }

  if (toggle) {
    toggle.addEventListener('click', () => {
      applyTheme(root.dataset.theme === 'dark' ? 'light' : 'dark');
    });
  }

  const navToggle = document.querySelector('.nav-toggle');
  const navLinks = document.querySelector('#nav-links');
  if (navToggle && navLinks) {
    navToggle.addEventListener('click', () => {
      const open = navLinks.classList.toggle('open');
      navToggle.setAttribute('aria-expanded', String(open));
    });
    navLinks.querySelectorAll('a').forEach((link) => {
      link.addEventListener('click', () => {
        navLinks.classList.remove('open');
        navToggle.setAttribute('aria-expanded', 'false');
      });
    });
  }

  const copy = document.querySelector('#copy-sha');
  const sha = document.querySelector('#sha');
  if (copy && sha) {
    copy.addEventListener('click', async () => {
      const value = sha.textContent.trim();
      try {
        await navigator.clipboard.writeText(value);
      } catch (_) {
        const range = document.createRange();
        range.selectNodeContents(sha);
        const selection = window.getSelection();
        selection.removeAllRanges();
        selection.addRange(range);
        document.execCommand('copy');
        selection.removeAllRanges();
      }
      const old = copy.textContent;
      copy.textContent = 'Скопировано';
      copy.disabled = true;
      setTimeout(() => { copy.textContent = old; copy.disabled = false; }, 1600);
    });
  }

  const targets = document.querySelectorAll('.feature-card, .release-grid article, .audience-card, .shot-card, .privacy-card');
  if ('IntersectionObserver' in window && !window.matchMedia('(prefers-reduced-motion: reduce)').matches) {
    targets.forEach((el, index) => {
      el.classList.add('reveal');
      el.style.transitionDelay = `${Math.min(index % 6, 5) * 45}ms`;
    });
    const observer = new IntersectionObserver((entries) => {
      entries.forEach((entry) => {
        if (entry.isIntersecting) {
          entry.target.classList.add('visible');
          observer.unobserve(entry.target);
        }
      });
    }, { threshold: 0.12 });
    targets.forEach((el) => observer.observe(el));
  }
})();
