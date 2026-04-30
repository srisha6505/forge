import { memo, useCallback, useEffect, useState } from "react";
import type { MouseEvent } from "react";
import {
  MessageSquare,
  MessageSquarePlus,
} from "./ui/Icons";
import { GhostBtn } from "./ui";
import { deleteChat, listChats, type ChatSummary } from "../lib/tauri";
import { ContextMenu, type MenuItem } from "./ContextMenu";

interface Props {
  vaultPath: string | null;
  reloadKey?: number;
  onOpenChat: (chatId: string, title: string) => void;
  onNewChat?: () => void;
}

function ChatHistorySidebar({
  vaultPath,
  reloadKey,
  onOpenChat,
  onNewChat,
}: Props) {
  const [chats, setChats] = useState<ChatSummary[]>([]);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [menu, setMenu] = useState<{ x: number; y: number; chat: ChatSummary } | null>(null);

  const reload = useCallback(async () => {
    if (!vaultPath) {
      setChats([]);
      setError(null);
      return;
    }
    setLoading(true);
    try {
      const list = await listChats(vaultPath);
      // Backend already sorts updated-desc, but defensively re-sort.
      list.sort((a, b) => b.updated.localeCompare(a.updated));
      setChats(list);
      setError(null);
    } catch (e) {
      setError(String(e));
      setChats([]);
    } finally {
      setLoading(false);
    }
  }, [vaultPath]);

  useEffect(() => {
    void reload();
  }, [reload, reloadKey]);

  const handleContextMenu = (e: MouseEvent<HTMLDivElement>, chat: ChatSummary) => {
    e.preventDefault();
    e.stopPropagation();
    setMenu({ x: e.clientX, y: e.clientY, chat });
  };

  const buildMenuItems = (chat: ChatSummary): MenuItem[] => {
    if (!vaultPath) return [];
    return [
      {
        label: "Open in chat panel",
        onClick: () => onOpenChat(chat.id, chat.title),
      },
      {
        label: "Copy chat ID",
        onClick: () => {
          navigator.clipboard?.writeText(chat.id).catch(() => {});
        },
      },
      { kind: "sep" },
      {
        label: "Delete chat",
        destructive: true,
        onClick: async () => {
          if (!confirm(`Delete "${chat.title}"? This can't be undone.`)) return;
          try {
            await deleteChat(vaultPath, chat.id);
            await reload();
          } catch (err) {
            alert(`Delete failed: ${err}`);
          }
        },
      },
    ];
  };

  return (
    <aside
      style={{
        height: "100%",
        background: "var(--background-secondary)",
        display: "flex",
        flexDirection: "column",
        minWidth: 0,
        overflow: "hidden",
      }}
    >
      {/* Header */}
      <div
        style={{
          height: 36,
          minHeight: 36,
          padding: "0 8px 0 14px",
          display: "flex",
          alignItems: "center",
          gap: 4,
          borderBottom: "1px solid var(--background-modifier-border)",
        }}
      >
        <span
          style={{
            flex: 1,
            fontSize: "var(--font-ui-small)",
            fontWeight: 600,
            color: "var(--text-normal)",
          }}
        >
          Chats
        </span>
        <GhostBtn
          icon={<MessageSquarePlus size={14} />}
          label="New chat"
          size={24}
          onClick={onNewChat}
        />
      </div>

      {/* List */}
      <div style={{ flex: 1, overflowY: "auto", padding: "4px 0 8px" }}>
        {!vaultPath ? (
          <EmptyMsg text="Open a vault to see chats" />
        ) : loading && chats.length === 0 ? (
          <EmptyMsg text="Loading…" />
        ) : error ? (
          <EmptyMsg text={`Error: ${error}`} />
        ) : chats.length === 0 ? (
          <EmptyMsg text="No chats yet" />
        ) : (
          chats.map((chat) => (
            <div
              key={chat.id}
              onClick={() => onOpenChat(chat.id, chat.title)}
              onContextMenu={(e) => handleContextMenu(e, chat)}
              style={{
                padding: "8px 14px",
                cursor: "pointer",
                transition:
                  "background var(--motion-duration-fast) var(--motion-ease)",
              }}
              onMouseEnter={(e: MouseEvent<HTMLDivElement>) => {
                e.currentTarget.style.background =
                  "var(--background-modifier-hover)";
              }}
              onMouseLeave={(e: MouseEvent<HTMLDivElement>) => {
                e.currentTarget.style.background = "transparent";
              }}
            >
              <div style={{ display: "flex", alignItems: "center", gap: 6 }}>
                <span
                  style={{
                    color: "var(--text-muted)",
                    display: "flex",
                    flexShrink: 0,
                  }}
                >
                  <MessageSquare size={14} />
                </span>
                <span
                  style={{
                    flex: 1,
                    fontSize: "var(--font-ui-medium)",
                    fontWeight: 500,
                    color: "var(--text-normal)",
                    overflow: "hidden",
                    textOverflow: "ellipsis",
                    whiteSpace: "nowrap",
                  }}
                >
                  {chat.title}
                </span>
              </div>
              <div
                style={{
                  display: "flex",
                  alignItems: "center",
                  gap: 6,
                  marginTop: 2,
                  paddingLeft: 20,
                }}
              >
                <span
                  style={{
                    fontSize: "var(--font-ui-smaller)",
                    color: "var(--text-faint)",
                  }}
                >
                  {formatRelative(chat.updated)}
                </span>
                {chat.model && (
                  <>
                    <span
                      style={{
                        fontSize: "var(--font-ui-smaller)",
                        color: "var(--text-faint)",
                      }}
                    >
                      ·
                    </span>
                    <span
                      style={{
                        fontSize: "var(--font-ui-smaller)",
                        color: "var(--text-faint)",
                        overflow: "hidden",
                        textOverflow: "ellipsis",
                        whiteSpace: "nowrap",
                      }}
                    >
                      {chat.model}
                    </span>
                  </>
                )}
              </div>
            </div>
          ))
        )}
      </div>

      {/* Footer */}
      <div
        style={{
          height: 26,
          padding: "0 12px",
          borderTop: "1px solid var(--background-modifier-border)",
          display: "flex",
          alignItems: "center",
          fontSize: "var(--font-ui-small)",
          color: "var(--text-muted)",
        }}
      >
        {chats.length} conversation{chats.length === 1 ? "" : "s"}
      </div>

      {menu && (
        <ContextMenu
          x={menu.x}
          y={menu.y}
          items={buildMenuItems(menu.chat)}
          onClose={() => setMenu(null)}
        />
      )}
    </aside>
  );
}

export default memo(ChatHistorySidebar);

function EmptyMsg({ text }: { text: string }) {
  return (
    <div
      style={{
        padding: "16px 14px",
        fontSize: "var(--font-ui-small)",
        color: "var(--text-faint)",
      }}
    >
      {text}
    </div>
  );
}

function formatRelative(iso: string): string {
  if (!iso) return "—";
  try {
    const d = new Date(iso);
    if (isNaN(d.getTime())) return iso;
    return d.toLocaleDateString(undefined, {
      month: "short",
      day: "numeric",
      year: "numeric",
    });
  } catch {
    return iso;
  }
}
