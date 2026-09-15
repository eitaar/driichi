import { StrictMode } from "react";
import { createRoot } from "react-dom/client";

function BootstrapScreen() {
  return (
    <main>
      <h1>Double Riichi</h1>
      <p>Workspace bootstrap complete.</p>
    </main>
  );
}

createRoot(document.getElementById("root")!).render(
  <StrictMode>
    <BootstrapScreen />
  </StrictMode>,
);
