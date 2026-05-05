// ZapretGUI frontend.
//
// Layout philosophy:
//  - Two pages (main / settings) toggled by adding/removing `is-active` on
//    `<main class="page">`. No SPA framework.
//  - The Tauri Rust backend exposes a small set of commands. We mirror its
//    state in `state` and re-render on change.

// ---------- Tauri bindings ----------
// In a built Tauri app, `window.__TAURI__.core.invoke` is injected. When
// running the static HTML in a plain browser (e.g. `python -m http.server`)
// we fall back to no-op stubs so the page still renders without errors.

const TAURI = (window.__TAURI__ && window.__TAURI__.core) || null;
const TAURI_EVENT = (window.__TAURI__ && window.__TAURI__.event) || null;
const DIALOG = (window.__TAURI__ && window.__TAURI__.dialog) || null;

async function invoke(cmd, args) {
  if (!TAURI) {
    console.warn("[mock] invoke", cmd, args);
    return mockInvoke(cmd, args);
  }
  return TAURI.invoke(cmd, args);
}

async function listen(event, cb) {
  if (!TAURI_EVENT) return () => {};
  return TAURI_EVENT.listen(event, cb);
}

async function pickFolder() {
  // Try every known path to the dialog plugin. Tauri 2 exposes plugins under
  // `window.__TAURI__.dialog` only when withGlobalTauri=true AND the plugin's
  // own JS package is loaded. As a robust fallback, we go through the core
  // `invoke()` directly: this works as long as the plugin is registered in
  // Rust, regardless of which JS injection actually happened.
  if (DIALOG && DIALOG.open) {
    try {
      return await DIALOG.open({ directory: true, multiple: false });
    } catch (e) {
      console.warn("DIALOG.open failed, falling back to plugin:dialog|open", e);
    }
  }
  if (TAURI && TAURI.invoke) {
    try {
      return await TAURI.invoke("plugin:dialog|open", {
        options: { directory: true, multiple: false },
      });
    } catch (e) {
      console.error("plugin:dialog|open failed", e);
      toast(`Не удалось открыть выбор папки: ${e}`);
    }
  }
  toast("Выбор папки недоступен. Введите путь вручную ниже.");
  return null;
}

// ---------- Mock backend (browser preview) ----------
const mockState = {
  settings: {
    zapret_path: null,
    strategy: null,
    game_filter: "off",
    minimize_to_tray: true,
    theme: "dark",
    autostart_zapret: false,
  },
  active: false,
  pid: null,
};

async function mockInvoke(cmd, args) {
  switch (cmd) {
    case "get_settings":
      return mockState.settings;
    case "save_settings":
      mockState.settings = args.settings;
      return null;
    case "detect_zapret_path":
      return null;
    case "validate_zapret_path":
      return Boolean(args.path);
    case "inspect_zapret_path":
      return {
        path: args.path,
        valid: Boolean(args.path),
        has_winws: true,
        has_general_bat: true,
        has_service_bat: true,
        has_lists: true,
        suggested_subfolder: null,
        message: "Папка zapret найдена",
      };
    case "list_strategies_cmd":
      return [
        { name: "general", file_name: "general.bat" },
        { name: "general (ALT)", file_name: "general (ALT).bat" },
        { name: "general (FAKE TLS AUTO)", file_name: "general (FAKE TLS AUTO).bat" },
      ];
    case "get_status":
      return { active: mockState.active, pid: mockState.pid };
    case "start_zapret":
      mockState.active = true;
      mockState.pid = 12345;
      return { active: true, pid: 12345 };
    case "stop_zapret":
      mockState.active = false;
      mockState.pid = null;
      return { active: false, pid: null };
    case "is_admin":
      return true;
    case "relaunch_as_admin":
      return null;
    case "quit_app":
      return null;
  }
  return null;
}

// ---------- DOM ----------
const $ = (sel) => document.querySelector(sel);

const els = {
  body: document.body,
  pageMain: $("#page-main"),
  pageSettings: $("#page-settings"),
  openSettings: $("#open-settings"),
  backMain: $("#back-main"),
  bigButton: $("#big-button"),
  statusText: $("#status-text"),
  statusSub: $("#status-sub"),
  cardStrategy: $("#card-strategy"),
  cardStrategyValue: $("#card-strategy-value"),
  cardGame: $("#card-game"),
  cardGameValue: $("#card-game-value"),
  zapretPathRow: $("#row-zapret-path"),
  zapretPathValue: $("#zapret-path-value"),
  zapretPathStatus: $("#zapret-path-status"),
  zapretPathMessage: $("#zapret-path-message"),
  zapretPathInput: $("#zapret-path-input"),
  zapretPathApply: $("#zapret-path-apply"),
  zapretPathRedetect: $("#zapret-path-redetect"),
  strategySelect: $("#strategy-select"),
  gameFilterSelect: $("#game-filter-select"),
  optTray: $("#opt-tray"),
  optAutostart: $("#opt-autostart"),
  optDark: $("#opt-dark"),
  adminWarning: $("#admin-warning"),
  restartAdmin: $("#restart-admin"),
  toast: $("#toast"),
};

const STATE = {
  settings: null,
  status: { active: false, pid: null },
  strategies: [],
  busy: false,
};

// ---------- Render helpers ----------
function applyTheme(theme) {
  els.body.dataset.theme = theme === "light" ? "light" : "dark";
}

function gameFilterLabel(mode) {
  switch (mode) {
    case "all":
      return "TCP+UDP";
    case "tcp":
      return "TCP";
    case "udp":
      return "UDP";
    default:
      return "Выкл";
  }
}

function strategyDisplayName(fileName) {
  if (!fileName) return "—";
  return fileName.replace(/\.bat$/i, "");
}

function renderMain() {
  const { active } = STATE.status;
  const strat = STATE.settings?.strategy ?? null;

  if (STATE.busy) {
    els.bigButton.dataset.state = "busy";
  } else {
    els.bigButton.dataset.state = active ? "on" : "off";
  }

  if (active) {
    els.statusText.textContent = "Активно";
    els.statusSub.textContent = strat
      ? `Стратегия: ${strategyDisplayName(strat)}`
      : "winws.exe запущен";
    els.statusText.style.color = "var(--success)";
  } else {
    els.statusText.textContent = "Отключено";
    els.statusText.style.color = "";
    if (!STATE.settings?.zapret_path) {
      els.statusSub.textContent = "Сначала укажите папку zapret в настройках";
    } else if (!strat) {
      els.statusSub.textContent = "Выберите стратегию в настройках";
    } else {
      els.statusSub.textContent = `Стратегия: ${strategyDisplayName(strat)}`;
    }
  }

  els.cardStrategyValue.textContent = strat ? strategyDisplayName(strat) : "—";
  els.cardGameValue.textContent = gameFilterLabel(STATE.settings?.game_filter);
}

function renderSettings() {
  if (!STATE.settings) return;
  const s = STATE.settings;

  els.zapretPathValue.textContent = s.zapret_path || "не выбрана";
  els.zapretPathInput.value = s.zapret_path || "";
  // The badge state is updated separately when we inspect the path.

  // Strategy select
  els.strategySelect.innerHTML = "";
  if (STATE.strategies.length === 0) {
    const opt = document.createElement("option");
    opt.value = "";
    opt.textContent = "—";
    els.strategySelect.appendChild(opt);
    els.strategySelect.disabled = true;
  } else {
    els.strategySelect.disabled = false;
    for (const st of STATE.strategies) {
      const opt = document.createElement("option");
      opt.value = st.file_name;
      opt.textContent = st.name;
      if (st.file_name === s.strategy) opt.selected = true;
      els.strategySelect.appendChild(opt);
    }
    if (!s.strategy && STATE.strategies.length > 0) {
      // Default to general.bat if present, else first.
      const def =
        STATE.strategies.find((x) => x.file_name.toLowerCase() === "general.bat") ??
        STATE.strategies[0];
      s.strategy = def.file_name;
      saveSettings();
    }
  }

  els.gameFilterSelect.value = s.game_filter || "off";
  els.optTray.checked = !!s.minimize_to_tray;
  els.optAutostart.checked = !!s.autostart_zapret;
  els.optDark.checked = s.theme !== "light";
}

function setPathBadge(ok) {
  els.zapretPathStatus.classList.remove("badge--ok", "badge--bad");
  if (ok) {
    els.zapretPathStatus.textContent = "OK";
    els.zapretPathStatus.classList.add("badge--ok");
  } else {
    els.zapretPathStatus.textContent = "не найден";
    els.zapretPathStatus.classList.add("badge--bad");
  }
}

function setPathMessage(msg) {
  if (!msg) {
    els.zapretPathMessage.textContent = "";
    els.zapretPathMessage.hidden = true;
  } else {
    els.zapretPathMessage.textContent = msg;
    els.zapretPathMessage.hidden = false;
  }
}

/// Try to apply a folder path: inspect it, save if valid, suggest subfolder if not.
async function applyZapretPath(rawPath) {
  if (!rawPath) {
    return;
  }
  const path = String(rawPath).trim().replace(/^["']|["']$/g, "");
  if (!path) return;

  let info;
  try {
    info = await invoke("inspect_zapret_path", { path });
  } catch (e) {
    toast(`Ошибка проверки пути: ${e}`);
    return;
  }

  if (info.valid) {
    STATE.settings.zapret_path = info.path;
    await saveSettings();
    await refreshStrategies();
    setPathBadge(true);
    setPathMessage("Папка zapret найдена ✓");
    renderSettings();
    renderMain();
    toast("Папка zapret сохранена");
    return;
  }

  // Invalid, but maybe we can suggest a subfolder.
  if (info.suggested_subfolder) {
    const ok = window.confirm(
      `${info.message}\n\nИспользовать вместо неё:\n${info.suggested_subfolder}?`
    );
    if (ok) {
      await applyZapretPath(info.suggested_subfolder);
      return;
    }
  }

  // Save anyway so the user can see what they entered, but flag it.
  STATE.settings.zapret_path = info.path;
  await saveSettings();
  setPathBadge(false);
  setPathMessage(info.message);
  renderSettings();
  renderMain();
  toast(info.message, 5000);
}

let toastTimer = null;
function toast(message, duration = 2400) {
  els.toast.textContent = message;
  els.toast.hidden = false;
  if (toastTimer) clearTimeout(toastTimer);
  toastTimer = setTimeout(() => {
    els.toast.hidden = true;
  }, duration);
}

// ---------- Page navigation ----------
function showPage(name) {
  els.pageMain.classList.toggle("is-active", name === "main");
  els.pageSettings.classList.toggle("is-active", name === "settings");
}

// ---------- Actions ----------
async function loadAll() {
  STATE.settings = await invoke("get_settings");
  applyTheme(STATE.settings.theme);
  if (!STATE.settings.zapret_path) {
    const detected = await invoke("detect_zapret_path");
    if (detected) {
      STATE.settings.zapret_path = detected;
      await saveSettings();
    }
  }
  await refreshStrategies();
  await refreshStatus();
  if (STATE.settings.zapret_path) {
    try {
      const info = await invoke("inspect_zapret_path", {
        path: STATE.settings.zapret_path,
      });
      setPathBadge(info.valid);
      setPathMessage(info.valid ? "" : info.message);
    } catch (_e) {
      setPathBadge(false);
    }
  } else {
    setPathBadge(false);
    setPathMessage("");
  }
  renderMain();
  renderSettings();
  await checkAdmin();
}

async function refreshStrategies() {
  if (!STATE.settings?.zapret_path) {
    STATE.strategies = [];
    return;
  }
  STATE.strategies = await invoke("list_strategies_cmd", {
    zapretPath: STATE.settings.zapret_path,
  });
}

async function refreshStatus() {
  STATE.status = await invoke("get_status");
}

async function saveSettings() {
  await invoke("save_settings", { settings: STATE.settings });
}

async function checkAdmin() {
  const ok = await invoke("is_admin");
  els.adminWarning.hidden = !!ok;
}

async function toggle() {
  if (STATE.busy) return;
  if (!STATE.status.active) {
    if (!STATE.settings.zapret_path) {
      toast("Сначала укажите папку с zapret в настройках");
      showPage("settings");
      return;
    }
    if (!STATE.settings.strategy) {
      toast("Выберите стратегию");
      showPage("settings");
      return;
    }
    STATE.busy = true;
    renderMain();
    try {
      STATE.status = await invoke("start_zapret");
      toast("zapret запущен");
    } catch (e) {
      toast(`Ошибка запуска: ${e}`);
    } finally {
      STATE.busy = false;
      renderMain();
    }
  } else {
    STATE.busy = true;
    renderMain();
    try {
      STATE.status = await invoke("stop_zapret");
      toast("zapret остановлен");
    } catch (e) {
      toast(`Ошибка остановки: ${e}`);
    } finally {
      STATE.busy = false;
      renderMain();
    }
  }
}

// ---------- Wiring ----------
els.openSettings.addEventListener("click", () => showPage("settings"));
els.backMain.addEventListener("click", () => showPage("main"));
els.bigButton.addEventListener("click", toggle);
els.cardStrategy.addEventListener("click", () => showPage("settings"));
els.cardGame.addEventListener("click", () => showPage("settings"));

els.zapretPathRow.addEventListener("click", async (e) => {
  // Don't intercept clicks on the manual-input area or its children.
  if (e.target.closest(".path-input")) return;
  const path = await pickFolder();
  if (!path) return;
  await applyZapretPath(path);
});

els.zapretPathApply.addEventListener("click", async () => {
  await applyZapretPath(els.zapretPathInput.value);
});

els.zapretPathInput.addEventListener("keydown", async (e) => {
  if (e.key === "Enter") {
    e.preventDefault();
    await applyZapretPath(els.zapretPathInput.value);
  }
});

els.zapretPathRedetect.addEventListener("click", async () => {
  const detected = await invoke("detect_zapret_path");
  if (detected) {
    await applyZapretPath(detected);
  } else {
    toast(
      "Не удалось найти zapret. Введите путь вручную (например, C:\\zapret-discord-youtube)",
      4500
    );
  }
});

els.strategySelect.addEventListener("change", async (e) => {
  STATE.settings.strategy = e.target.value || null;
  await saveSettings();
  renderMain();
});

els.gameFilterSelect.addEventListener("change", async (e) => {
  STATE.settings.game_filter = e.target.value;
  await saveSettings();
  renderMain();
});

els.optTray.addEventListener("change", async (e) => {
  STATE.settings.minimize_to_tray = e.target.checked;
  await saveSettings();
});

els.optAutostart.addEventListener("change", async (e) => {
  STATE.settings.autostart_zapret = e.target.checked;
  await saveSettings();
});

els.optDark.addEventListener("change", async (e) => {
  STATE.settings.theme = e.target.checked ? "dark" : "light";
  applyTheme(STATE.settings.theme);
  await saveSettings();
});

els.restartAdmin.addEventListener("click", async () => {
  try {
    await invoke("relaunch_as_admin");
  } catch (e) {
    toast(`UAC отклонён: ${e}`);
  }
});

// Listen for status changes pushed from the backend (e.g. autostart, tray menu).
listen("zapret-status", (event) => {
  STATE.status = event.payload;
  renderMain();
});

// Periodic status polling - cheap, since the command just queries sysinfo.
setInterval(async () => {
  if (!STATE.busy) {
    const prev = STATE.status.active;
    STATE.status = await invoke("get_status");
    if (prev !== STATE.status.active) renderMain();
  }
}, 2500);

// Boot.
loadAll().catch((e) => {
  console.error(e);
  toast(`Ошибка инициализации: ${e}`);
});
