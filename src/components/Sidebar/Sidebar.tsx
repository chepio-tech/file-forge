// Components
import Icon from "@/components/Icon/Icon";
// Types
import type { GroupDefinition, ToolId } from "@/features/featureCatalog";
// Styles
import "./Sidebar.css";
// Consts
import messages from "@/messages/messages";

interface SidebarProps {
  groups: GroupDefinition[];
  activeTool: ToolId;
  onSelect: (tool: ToolId) => void;
}

function Sidebar({ groups, activeTool, onSelect }: SidebarProps) {
  return (
    <aside className="sidebar">
      <div className="sidebar__brand" data-tauri-drag-region>
        <img className="sidebar__logo" src="/app-icon.png" alt="" width={28} height={28} data-tauri-drag-region />
        <span className="sidebar__name" data-tauri-drag-region>
          {messages.app.name}
        </span>
      </div>

      <nav className="sidebar__nav" aria-label={messages.nav.label}>
        {groups.map((group) => (
          <section key={group.id} className={`sidebar__group sidebar__group--${group.id}`}>
            <h2 className="sidebar__group-title" id={`group-${group.id}`}>
              <span className="sidebar__swatch" aria-hidden="true" />
              {messages.nav.groups[group.id]}
            </h2>
            <ul className="sidebar__tools" aria-labelledby={`group-${group.id}`}>
              {group.tools.map((tool) => {
                const { nav, title } = messages.tools[tool.id];
                if (tool.status === "soon") {
                  return (
                    <li key={tool.id}>
                      <span className="sidebar__tool sidebar__tool--soon">
                        <Icon name={tool.icon} />
                        <span className="sidebar__tool-title" aria-hidden="true">
                          {nav}
                        </span>
                        <span className="visually-hidden">{title}</span>
                        <span className="sidebar__badge">{messages.nav.soon}</span>
                      </span>
                    </li>
                  );
                }
                const active = tool.id === activeTool;
                return (
                  <li key={tool.id}>
                    <button
                      type="button"
                      className={`sidebar__tool${active ? " sidebar__tool--active" : ""}`}
                      aria-current={active ? "page" : undefined}
                      aria-label={title}
                      onClick={() => onSelect(tool.id)}
                    >
                      <Icon name={tool.icon} />
                      <span className="sidebar__tool-title">{nav}</span>
                    </button>
                  </li>
                );
              })}
            </ul>
          </section>
        ))}
      </nav>
    </aside>
  );
}

export default Sidebar;
