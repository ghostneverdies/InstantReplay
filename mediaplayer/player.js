"use strict";

function post(obj) {
  if (window.chrome && window.chrome.webview) {
    window.chrome.webview.postMessage(JSON.stringify(obj));
  }
}

const params = new URLSearchParams(location.search);
const SRC = params.get("src") || "";
const START_T = parseFloat(params.get("t")) || 0;
const MODE = params.get("mode") === "fullscreen" ? "fullscreen" : "inline";

const v = document.getElementById("v");
const bar = document.getElementById("bar");
const closeBtn = document.getElementById("closeBtn");
const center = document.getElementById("center");
const bigPlay = document.getElementById("bigPlay");
const bigReplay = document.getElementById("bigReplay");
const errorBox = document.getElementById("error");
const retryBtn = document.getElementById("retry");
const playPause = document.getElementById("playPause");
const icPlay = document.getElementById("icPlay");
const icPause = document.getElementById("icPause");
const cur = document.getElementById("cur");
const dur = document.getElementById("dur");
const seek = document.getElementById("seek");
const mute = document.getElementById("mute");
const icWave = document.getElementById("icWave");
const icSlash = document.getElementById("icSlash");
const vol = document.getElementById("vol");
const fsBtn = document.getElementById("fs");
const icExpand = document.getElementById("icExpand");
const icCompress = document.getElementById("icCompress");

let scrubbing = false;
let mutedByUser = null;

function fmt(t) {
  if (!isFinite(t) || t < 0) t = 0;
  t = Math.floor(t);
  const h = Math.floor(t / 3600);
  const m = Math.floor((t % 3600) / 60);
  const s = t % 60;
  const pad = (n) => (n < 10 ? "0" + n : "" + n);
  return h > 0 ? `${h}:${pad(m)}:${pad(s)}` : `${m}:${pad(s)}`;
}

function updateSeekFill() {
  const max = parseFloat(seek.max) || 0;
  const pct = max > 0 ? (parseFloat(seek.value) / max) * 100 : 0;
  seek.style.background =
    `linear-gradient(to right, var(--accent) ${pct}%, rgba(255,255,255,0.25) ${pct}%)`;
}

function updateVolFill() {
  const pct = (parseFloat(vol.value) || 0) * 100;
  vol.style.background =
    `linear-gradient(to right, var(--accent) ${pct}%, rgba(255,255,255,0.25) ${pct}%)`;
}

function updateIcons() {
  icPlay.style.display = v.paused ? "" : "none";
  icPause.style.display = v.paused ? "none" : "";
  const muted = v.muted || v.volume <= 0.004;
  icSlash.style.display = muted ? "" : "none";
  icWave.style.opacity = muted ? "0" : Math.min(1, v.volume * 1.25).toFixed(3);

  if (MODE === "fullscreen") {
    icExpand.style.display = "none";
    icCompress.style.display = "";
  } else {
    icExpand.style.display = "";
    icCompress.style.display = "none";
  }

  bigReplay.hidden = !v.ended;
  bigPlay.hidden = v.ended || !v.paused;
  document.body.classList.toggle("paused", v.paused && !v.ended && !v.srcError);
}

function postState() {
  post({
    type: "state",
    playing: !v.paused && !v.ended && v.readyState >= 2,
    t: v.currentTime,
    d: isFinite(v.duration) ? v.duration : 0,
    v: v.volume,
    m: v.muted || v.volume <= 0.004,
    ended: v.ended,
    ready: v.readyState >= 1 || (isFinite(v.duration) && v.duration > 0),
    err: v.srcError ? "video-error" : null,
  });
}

v.addEventListener("play", () => { postState(); updateIcons(); });
v.addEventListener("pause", () => { postState(); updateIcons(); });
v.addEventListener("ended", () => { postState(); updateIcons(); });
v.addEventListener("volumechange", () => { postState(); updateVolFill(); updateIcons(); });
v.addEventListener("loadedmetadata", () => {
  if (isFinite(v.duration) && v.duration > 0) {
    seek.max = v.duration;
    dur.textContent = fmt(v.duration);
  } else {
    seek.disabled = true;
    dur.textContent = "--:--";
  }
  if (START_T > 0) v.currentTime = START_T;
  updateSeekFill();
  updateIcons();
  postState();
});
v.addEventListener("durationchange", () => {
  if (isFinite(v.duration) && v.duration > 0) {
    seek.disabled = false;
    seek.max = v.duration;
    dur.textContent = fmt(v.duration);
    updateSeekFill();
  }
});
v.addEventListener("timeupdate", () => {
  if (!scrubbing) {
    if (isFinite(v.duration) && v.duration > 0) seek.value = v.currentTime;
    updateSeekFill();
    cur.textContent = fmt(v.currentTime);
  }
});
v.addEventListener("error", () => {
  v.srcError = true;
  errorBox.hidden = false;
  updateIcons();
  postState();
});

setInterval(postState, 250);

function togglePlay() {
  if (v.ended) {
    v.currentTime = 0;
    v.play();
  } else if (v.paused) {
    v.play();
  } else {
    v.pause();
  }
}

playPause.addEventListener("click", togglePlay);
bigPlay.addEventListener("click", togglePlay);
bigReplay.addEventListener("click", togglePlay);

document.addEventListener("click", (e) => {
  if (e.target.closest("#bar, #closeBtn, .big, #error")) return;
  togglePlay();
});

retryBtn.addEventListener("click", () => {
  errorBox.hidden = true;
  delete v.srcError;
  v.load();
  tryPlay();
});

closeBtn.addEventListener("click", () => {
  post({ type: "close" });
});

seek.addEventListener("input", () => {
  scrubbing = true;
  v.currentTime = parseFloat(seek.value);
  updateSeekFill();
  cur.textContent = fmt(v.currentTime);
});
seek.addEventListener("change", () => { scrubbing = false; postState(); });

vol.addEventListener("input", () => {
  v.volume = parseFloat(vol.value);
  if (v.volume > 0) {
    if (v.muted) v.muted = false;
    mutedByUser = false;
  } else {
    mutedByUser = true;
  }
  updateVolFill();
  updateIcons();
  try { localStorage.setItem("ir_vol", String(v.volume)); } catch (_) { }
  postState();
});

mute.addEventListener("click", () => {
  mutedByUser = !v.muted;
  v.muted = mutedByUser;
  try { localStorage.setItem("ir_muted", v.muted ? "1" : "0"); } catch (_) { }
  postState();
  updateIcons();
});

fsBtn.addEventListener("click", toggleFullscreen);

function toggleFullscreen() {
  if (MODE === "fullscreen") post({ type: "fs", act: "exit" });
  else post({ type: "fs", act: "enter" });
}

document.addEventListener("keydown", (e) => {
  if (e.key === "Escape") {
    e.preventDefault();
    post({ type: "close" });
    return;
  }

  const tag = (e.target && e.target.tagName) || "";
  if (tag === "INPUT" || tag === "TEXTAREA") return;
  switch (e.key) {
    case " ":
    case "k":
      e.preventDefault();
      togglePlay();
      break;
    case "ArrowRight":
      e.preventDefault();
      v.currentTime = Math.min(v.currentTime + 5, isFinite(v.duration) ? v.duration : v.currentTime + 5);
      break;
    case "ArrowLeft":
      e.preventDefault();
      v.currentTime = Math.max(v.currentTime - 5, 0);
      break;
    case "m":
      mute.click();
      break;
    case "f":
      toggleFullscreen();
      break;
  }
});

let idleTimer = null;
function activity() {
  document.body.classList.remove("idle");
  if (idleTimer) clearTimeout(idleTimer);
  idleTimer = setTimeout(() => {
    if (!v.paused && !v.ended && errorBox.hidden && !scrubbing) {
      document.body.classList.add("idle");
    }
  }, 2500);
}
["mousemove", "pointerdown", "keydown"].forEach((ev) =>
  document.addEventListener(ev, activity, { passive: true })
);

function applyStoredVolume() {
  try {
    const sv = parseFloat(localStorage.getItem("ir_vol") || "1");
    if (isFinite(sv) && sv >= 0 && sv <= 1) {
      v.volume = sv;
      vol.value = sv;
      updateVolFill();
    }
    if (localStorage.getItem("ir_muted") === "1") {
      v.muted = true;
      mutedByUser = true;
    }
  } catch (_) { }
  updateIcons();
}

function tryPlay() {
  const p = v.play();
  if (!p) return;
  p.catch(() => {
    if (mutedByUser === true) return;
    v.muted = true;
    const retry = v.play();
    if (!retry) return;
    retry
      .then(() => {
        if (mutedByUser !== true) {
          v.muted = false;
          v.dispatchEvent(new Event('volumechange'));
          updateVolFill();
        }
      })
      .catch(() => { });
  });
}

document.body.classList.add(MODE);
try { window.focus(); } catch (_) { }
updateSeekFill();
updateVolFill();
updateIcons();

if (!SRC) {
  errorBox.querySelector("p").textContent = "No media source was provided.";
  errorBox.hidden = false;
} else {
  v.src = SRC;
  applyStoredVolume();
  updateIcons();
  tryPlay();
}

window.receiveCommand = function (cmd) {
  if (!cmd) return;
  try {
    switch (cmd.c) {
      case "play":
        v.play();
        break;
      case "pause":
        v.pause();
        break;
      case "seek":
        if (isFinite(cmd.t)) v.currentTime = cmd.t;
        break;
      case "seekplay":
        if (isFinite(cmd.t)) v.currentTime = cmd.t;
        v.play();
        break;
      case "fullscreen":
        toggleFullscreen();
        break;
      case "close":
        post({ type: "close" });
        break;
      case "accent":
        if (typeof cmd.v === "string" && /^#[0-9a-fA-F]{6,8}$/.test(cmd.v)) {
          document.documentElement.style.setProperty("--accent", cmd.v);
        }
        break;
    }
  } catch (_) { }
};