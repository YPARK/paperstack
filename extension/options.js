const field = document.getElementById("token");
const said = document.getElementById("said");

chrome.storage.local.get("token").then(({ token }) => {
  field.value = token || "";
  said.textContent = token ? "Kept." : "";
});

field.addEventListener("input", async () => {
  const token = field.value.trim();
  await chrome.storage.local.set({ token });
  said.textContent = /^[0-9a-f]{64}$/.test(token) ? "Kept." : "Not yet a token: 64 letters, 0–9 and a–f.";
});
