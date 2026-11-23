# Instant Replay site plan

kind: software (Windows replay recorder; site-kind software)

deal: hero centred; nav-mega; footer-dark; close-type; rhythm space; buttons tonal; mark dots;
palette white-mint-tiles (a white ground and mint accent keep the product window the brightest thing
on the page and match the app's own light accent colour); type familjen; radius soft; no CSS ground;
effect none at first pass; moment pinned-chapters in the three-step section only.

pages: one page, `index.html`, served from the repository's `site/` folder by GitHub Pages.

## Copy (written before the build)

- h1: Replay recorder for Windows games
- lead: Press one hotkey and Instant Replay keeps the last 30, 60 or 120 seconds as an MP4. It follows
  a game from windowed to fullscreen and keeps the audio in sync.
- pill: Version 1.6 — free and open source.
- main action, word for word in nav, hero, download section and close: Download for Windows.

### Sections
1. hero (centred, one screen) — product window rises out of a lit frame; the Home page is the picture.
2. app-frame (`#app`, one screen) — "Every replay is already there when you need it." List of saved
   replays beside the open clip. Live HTML window, not a screenshot.
3. features-2x2 (`#capture`, two screens) — "Capture that follows the game." Four tiles: whole
   monitor, one game, audio that stays in sync, anti-cheat safety.
4. pinned-chapters (`#how`, two screens) — "Save a clip in three steps." Buffer, hotkey, gallery.
   The one dealt moment; scroll only inside this section.
5. specs rows (`#specs`, one screen) — "Choose what goes into the clip." Whole Desktop against Game
   Capture, with real values only.
6. faq (`#faq`, one screen) — six buyer questions; every answer open.
7. download (`#download`, one screen) — real release links and requirements.
8. footer.closing (`#contact`, one screen) — close-type, download action repeated, links to #ids.

## Assets
- No client screenshots were supplied. The product window is live HTML on the hero and in `#app`.
- `assets/screens/home.svg`, `clip.svg`, `gallery.svg` are drawn views of that same window for the
  moment's three steps; they are stand-ins for real screenshots and are listed in the report.
- `assets/icon.png` is the project's own icon from `E:\projects\InstantReplay\assets\icons\icon.png`.
- GitHub mark is resolved locally by `icons.mjs scan`.
