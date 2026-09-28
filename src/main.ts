import { invoke } from "@tauri-apps/api/core";

// TODO: replace with your UI logic. This is a smoke test that the
// frontend <-> Rust command bridge works.
const form = document.querySelector<HTMLFormElement>("#greet-form");
const input = document.querySelector<HTMLInputElement>("#greet-input");
const msg = document.querySelector<HTMLParagraphElement>("#greet-msg");

form?.addEventListener("submit", async (e) => {
  e.preventDefault();
  if (!input || !msg) return;
  msg.textContent = await invoke<string>("greet", { name: input.value || "world" });
});
