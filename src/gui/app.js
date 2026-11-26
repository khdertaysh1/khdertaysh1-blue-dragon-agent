"use strict";
const el = (id) => document.getElementById(id);
if (location.hash.length > 1) {
  sessionStorage.setItem("key", location.hash.slice(1));
  history.replaceState(null, "", "/");
}
async function api(action, fields = {}) {
  const response = await fetch("/api", {
    method: "POST",
    headers: {
      Authorization: "Bearer " + sessionStorage.getItem("key"),
      "Content-Type": "application/json",
    },
    body: JSON.stringify({ action, ...fields }),
  });
  if (!response.ok) {
    const body = await response.text();
    let message = body;
    try {
      message = JSON.parse(body).error;
    } catch {}
    throw new Error(message);
  }
  return response.json();
}
function listen(id, event, fn) {
  el(id).addEventListener(event, async (e) => {
    e.preventDefault();
    try {
      el("error").textContent = "";
      await fn();
    } catch (err) {
      el("error").textContent = err.message;
    }
  });
}
let approvalId, lastRows, workspace, session, revision;
function render(state) {
  revision = state.revision;
  session = state.session;
  el("model").textContent = state.model;
  el("root").textContent = state.workspace;
  if (workspace !== state.workspace) {
    workspace = state.workspace;
    el("workspace").value = workspace;
  }
  el("send").disabled = state.busy;
  el("status").textContent = state.approval
    ? "Waiting for approval"
    : state.busy
      ? "Working…"
      : "Ready";
  const rows = state.draft
    ? [...state.rows, ["Dragon", state.draft]]
    : state.rows;
  const fingerprint = JSON.stringify(rows);
  if (lastRows !== fingerprint) {
    const box = el("history"),
      bottom = box.scrollHeight - box.scrollTop - box.clientHeight < 40;
    box.replaceChildren(
      ...rows.map(([role, text]) => {
        const article = document.createElement("article"),
          label = document.createElement("b"),
          body = document.createElement("pre");
        label.textContent = role;
        body.dir = "auto";
        body.textContent = text;
        article.append(label, body);
        return article;
      }),
    );
    if (bottom) box.scrollTop = box.scrollHeight;
    lastRows = fingerprint;
  }
  approvalId = state.approval?.id;
  el("approval").hidden = !state.approval;
  el("detail").textContent = state.approval
    ? state.approval.action + "\n" + state.approval.detail
    : "";
}
async function poll() {
  try {
    const state = await api("state", { since: revision });
    if (state) render(state);
  } catch (err) {
    revision = undefined;
    el("status").textContent = err.message;
    el("send").disabled = true;
  }
  setTimeout(poll, 600);
}
listen("compose", "submit", async () => {
  const text = el("prompt").value;
  el("send").disabled = true;
  try {
    await api("send", { text, session });
    el("prompt").value = "";
  } finally {
    revision = undefined;
  }
});
for (const id of ["allow", "deny"])
  listen(id, "click", async () => {
    await api("approve", { id: approvalId, allow: id === "allow" });
    el("approval").hidden = true;
  });
function toggle(open) {
  el("panel").hidden = !open;
  el("sessions").setAttribute("aria-expanded", String(open));
}
async function openSession(workspace, session) {
  await api("open", { workspace, session });
  toggle(false);
}
listen("open", "submit", () => openSession(el("workspace").value, null));
listen("sessions", "click", async () => {
  if (!el("panel").hidden) return toggle(false);
  toggle(true);
  const saved = await api("sessions");
  el("saved").replaceChildren(
    ...saved.map((s) => {
      const b = document.createElement("button");
      b.textContent = s.workspace + " · " + s.id;
      b.onclick = () =>
        openSession(s.workspace, s.id).catch(
          (e) => (el("error").textContent = e.message),
        );
      return b;
    }),
  );
});
poll();
