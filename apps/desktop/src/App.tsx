import { useState } from "react";
import Unlock from "./pages/Unlock";
import VaultList from "./pages/VaultList";
import ItemDetail from "./pages/ItemDetail";

/** Application view state. */
type View =
  | { page: "unlock" }
  | { page: "list" }
  | { page: "detail"; itemId: string };

function App() {
  const [view, setView] = useState<View>({ page: "unlock" });

  const handleUnlocked = () => setView({ page: "list" });
  const handleSelectItem = (id: string) =>
    setView({ page: "detail", itemId: id });
  const handleBack = () => setView({ page: "list" });
  const handleLocked = () => setView({ page: "unlock" });

  switch (view.page) {
    case "unlock":
      return <Unlock onUnlocked={handleUnlocked} />;
    case "list":
      return (
        <VaultList
          onSelectItem={handleSelectItem}
          onLocked={handleLocked}
        />
      );
    case "detail":
      return <ItemDetail itemId={view.itemId} onBack={handleBack} />;
  }
}

export default App;
