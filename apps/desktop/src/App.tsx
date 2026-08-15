import { useState, useEffect, useCallback } from "react";
import { invoke } from "@tauri-apps/api/core";
import Unlock from "./pages/Unlock";
import VaultList from "./pages/VaultList";
import ItemDetail from "./pages/ItemDetail";
import Devices from "./pages/Devices";

/** Application view state. */
type View =
  | { page: "unlock" }
  | { page: "list" }
  | { page: "detail"; itemId: string }
  | { page: "devices" };

function App() {
  const [view, setView] = useState<View>({ page: "unlock" });

  const handleUnlocked = () => setView({ page: "list" });
  const handleSelectItem = (id: string) =>
    setView({ page: "detail", itemId: id });
  const handleBack = () => setView({ page: "list" });
  const handleLocked = useCallback(async () => {
    try {
      await invoke("lock_vault");
    } catch {
      // already locked
    }
    setView({ page: "unlock" });
  }, []);
  const handleDevices = () => setView({ page: "devices" });

  // Keyboard shortcuts (only active when vault is unlocked)
  useEffect(() => {
    if (view.page === "unlock") return;

    const handler = (e: KeyboardEvent) => {
      const mod = e.metaKey || e.ctrlKey;

      if (mod && e.key === "l") {
        e.preventDefault();
        handleLocked();
      }
      if (mod && e.key === "n") {
        e.preventDefault();
        // Dispatch custom event that VaultList listens to
        window.dispatchEvent(new CustomEvent("zvault:open-add"));
      }
      if (mod && e.key === "f") {
        e.preventDefault();
        // Dispatch custom event that VaultList listens to
        window.dispatchEvent(new CustomEvent("zvault:focus-search"));
      }
      if (e.key === "Escape") {
        // Dispatch custom event that modals/detail views listen to
        window.dispatchEvent(new CustomEvent("zvault:escape"));
      }
    };

    window.addEventListener("keydown", handler);
    return () => window.removeEventListener("keydown", handler);
  }, [view.page, handleLocked]);

  // Listen for escape to go back from detail/devices
  useEffect(() => {
    if (view.page !== "detail" && view.page !== "devices") return;

    const handler = () => setView({ page: "list" });
    window.addEventListener("zvault:escape", handler);
    return () => window.removeEventListener("zvault:escape", handler);
  }, [view.page]);

  switch (view.page) {
    case "unlock":
      return <Unlock onUnlocked={handleUnlocked} />;
    case "list":
      return (
        <VaultList
          onSelectItem={handleSelectItem}
          onLocked={handleLocked}
          onDevices={handleDevices}
        />
      );
    case "detail":
      return <ItemDetail itemId={view.itemId} onBack={handleBack} />;
    case "devices":
      return <Devices onBack={handleBack} />;
  }
}

export default App;
