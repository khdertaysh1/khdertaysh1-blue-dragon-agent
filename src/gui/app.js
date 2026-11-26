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
    const expanded = new Set(
      Array.from(box.querySelectorAll("details[open]"), node => node.dataset.group)
    );
    const content = document.createDocumentFragment();
    let group, summary, steps = 0;
    let arabic = navigator.language.startsWith("ar");
    for (const [index, [role, text]] of rows.entries()) {
      if (role === "You") arabic = /[\u0600-\u06ff]/u.test(text);
      const article = document.createElement("article");
      const label = document.createElement("b");
      const body = document.createElement("pre");
      label.textContent = role;
      body.dir = "auto";
      body.textContent = text;
      article.append(label, body);
      if (role === "Tool" || role === "Result") {
        if (!group) {
          group = document.createElement("details");
          group.dataset.group = JSON.stringify([state.session, index, text]);
          group.open = expanded.has(group.dataset.group);
          summary = document.createElement("summary");
          group.append(summary);
          content.append(group);
          steps = 0;
        }
        if (role === "Tool") {
          steps++;
          const name = text.replace(/ \(step \d+\)$/, "");
          const count = arabic ? "عدد الخطوات " + steps.toLocaleString("ar") : steps + (steps === 1 ? " step" : " steps");
          summary.textContent = name + " · " + count;
        } else if (!steps) summary.textContent = "Tool result";
        group.append(article);
      } else {
        group = null;
        content.append(article);
      }
    }
    box.replaceChildren(content);
    if (bottom) box.scrollTop = box.scrollHeight;
    lastRows = fingerprint;
  }
  approvalId = state.approval?.id;
  el("approval").hidden = !state.approval;
  el("allow").textContent = state.approval?.page_reads ? "Allow for this task" : "Approve once";
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
