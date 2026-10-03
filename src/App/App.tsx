// Core
import { useState } from "react";
// Components
import ChepioTechFooter from "@/components/ChepioTechFooter/ChepioTechFooter";
import Sidebar from "@/components/Sidebar/Sidebar";
// Types
import type { ToolId } from "@/features/featureCatalog";
// Styles
import "./App.css";
// Consts
import { featureCatalog, firstReadyTool } from "@/features/featureCatalog";
import { toolPanels } from "./toolPanels";

const readyTools = featureCatalog.flatMap((group) => group.tools).filter((tool) => tool.status === "ready");

function App() {
  const [activeTool, setActiveTool] = useState<ToolId>(() => firstReadyTool()?.id ?? "pdfCompress");

  return (
    <div className="app">
      <Sidebar groups={featureCatalog} activeTool={activeTool} onSelect={setActiveTool} />
      <main className="app__main">
        {readyTools.map((tool) => {
          const Panel = toolPanels[tool.id];
          if (!Panel) return null;
          const active = tool.id === activeTool;
          return (
            <div key={tool.id} className="app__panel" hidden={!active}>
              <Panel tool={tool} active={active} />
            </div>
          );
        })}
      </main>
      <footer className="app__footer">
        <ChepioTechFooter />
      </footer>
    </div>
  );
}

export default App;
