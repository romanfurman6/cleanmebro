# Website

Static site for https://cmdmeow.furman.cat, deployed by `.github/workflows/pages.yml`
on every push to `main` that touches `site/`.

- `index.html`, `style.css`: the landing page.
- `play/`: video player for shared recordings. `play/?v=<video url>` (optional `&t=<title>`).
  Only http(s) URLs play. The host must allow range requests (S3 does) for seeking.
- `icon.png` is copied from `docs/images/icon.png` at deploy time (PNGs are gitignored).

Preview locally: `cp docs/images/icon.png site/ && python3 -m http.server -d site 8080`.
