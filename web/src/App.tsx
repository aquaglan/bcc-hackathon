import { useState } from "react";
import { Sidebar } from "./screens/Sidebar";
import { ReviewPage } from "./screens/ReviewPage";
import { Upload } from "./screens/Upload";
import { useRoute } from "./route";
import { useToast } from "./toast";

export function App() {
  const route = useRoute();
  const [toast, closeToast] = useToast();
  // Bumped when a review is created or re-checked so the sidebar reloads at once.
  const [listEpoch, setListEpoch] = useState(0);
  const refreshList = () => setListEpoch((n) => n + 1);

  return (
    <div className="app">
      <Sidebar activeId={route.name === "review" ? route.id : null} epoch={listEpoch} />
      <main className="main">
        {route.name === "new" ? (
          <Upload onCreated={refreshList} />
        ) : (
          <ReviewPage key={route.id} id={route.id} tab={route.tab} onChanged={refreshList} />
        )}
      </main>
      {toast && (
        <div className="toast" role="alert" key={toast}>
          <span>{toast}</span>
          <button type="button" onClick={closeToast} aria-label="Закрыть">
            <svg viewBox="0 0 24 24" width="16" height="16" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round">
              <path d="M6 6l12 12M18 6L6 18" />
            </svg>
          </button>
        </div>
      )}
    </div>
  );
}
