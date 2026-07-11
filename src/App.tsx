import { useEffect, useState } from "react";
import { useTranslation } from "react-i18next";
import { Maximize2, Minimize2, Minus, X } from "lucide-react";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { toast } from "sonner";
import { AgentLifecyclePage } from "@/components/AgentLifecyclePage";
import { Button } from "@/components/ui/button";
import { extractErrorMessage } from "@/utils/errorUtils";
import {
  DRAG_REGION_ATTR,
  DRAG_REGION_STYLE,
  isLinux,
  isWindows,
} from "@/lib/platform";

const DEFAULT_DRAG_BAR_HEIGHT = isWindows() || isLinux() ? 0 : 28;
const HEADER_HEIGHT = 64;

function App() {
  const { t } = useTranslation();
  const [isWindowMaximized, setIsWindowMaximized] = useState(false);
  const [useAppWindowControls, setUseAppWindowControls] = useState(false);

  useEffect(() => {
    let active = true;
    let unlistenResize: (() => void) | undefined;

    const syncWindowState = async () => {
      try {
        const currentWindow = getCurrentWindow();
        const updateState = async () => {
          const [decorated, maximized] = await Promise.all([
            currentWindow.isDecorated(),
            currentWindow.isMaximized(),
          ]);
          if (active) {
            setUseAppWindowControls(!decorated);
            setIsWindowMaximized(maximized);
          }
        };

        await updateState();
        unlistenResize = await currentWindow.onResized(() => {
          void updateState();
        });
      } catch (error) {
        console.error("[App] Failed to sync window state", error);
      }
    };

    void syncWindowState();
    return () => {
      active = false;
      unlistenResize?.();
    };
  }, []);

  const notifyWindowControlError = (error: unknown) => {
    toast.error(
      t("notifications.windowControlFailed", {
        defaultValue: "Window control failed: {{error}}",
        error: extractErrorMessage(error),
      }),
    );
  };

  const handleWindowMinimize = async () => {
    try {
      await getCurrentWindow().minimize();
    } catch (error) {
      notifyWindowControlError(error);
    }
  };

  const handleWindowToggleMaximize = async () => {
    try {
      const currentWindow = getCurrentWindow();
      await currentWindow.toggleMaximize();
      setIsWindowMaximized(await currentWindow.isMaximized());
    } catch (error) {
      notifyWindowControlError(error);
    }
  };

  const handleWindowClose = async () => {
    try {
      await getCurrentWindow().close();
    } catch (error) {
      notifyWindowControlError(error);
    }
  };

  const dragBarHeight = useAppWindowControls ? 32 : DEFAULT_DRAG_BAR_HEIGHT;
  const contentTopOffset = dragBarHeight + HEADER_HEIGHT;

  return (
    <div
      className="flex h-screen flex-col overflow-hidden bg-background text-foreground selection:bg-primary/30"
      style={{ paddingTop: contentTopOffset }}
    >
      {(dragBarHeight > 0 || useAppWindowControls) && (
        <div
          className="fixed left-0 right-0 top-0 z-[70] flex items-center justify-end px-2"
          {...DRAG_REGION_ATTR}
          style={{ ...DRAG_REGION_STYLE, height: dragBarHeight }}
        >
          {useAppWindowControls && (
            <div className="flex items-center gap-1" data-tauri-no-drag>
              <Button
                variant="ghost"
                size="icon"
                onClick={() => void handleWindowMinimize()}
                title={t("header.windowMinimize")}
                className="h-7 w-7"
              >
                <Minus className="h-4 w-4" />
              </Button>
              <Button
                variant="ghost"
                size="icon"
                onClick={() => void handleWindowToggleMaximize()}
                title={
                  isWindowMaximized
                    ? t("header.windowRestore")
                    : t("header.windowMaximize")
                }
                className="h-7 w-7"
              >
                {isWindowMaximized ? (
                  <Minimize2 className="h-4 w-4" />
                ) : (
                  <Maximize2 className="h-4 w-4" />
                )}
              </Button>
              <Button
                variant="ghost"
                size="icon"
                onClick={() => void handleWindowClose()}
                title={t("header.windowClose")}
                className="h-7 w-7 hover:bg-red-500/15 hover:text-red-500"
              >
                <X className="h-4 w-4" />
              </Button>
            </div>
          )}
        </div>
      )}

      <header
        className="fixed z-50 w-full border-b border-border/60 bg-background/80 backdrop-blur-md"
        {...DRAG_REGION_ATTR}
        style={{
          ...DRAG_REGION_STYLE,
          top: dragBarHeight,
          height: HEADER_HEIGHT,
        }}
      >
        <div className="flex h-full items-center px-6" {...DRAG_REGION_ATTR}>
          <h1 className="text-lg font-semibold">Agent Manager</h1>
        </div>
      </header>

      <main className="min-h-0 flex-1 overflow-y-auto px-4 py-6 sm:px-6">
        <div className="mx-auto w-full max-w-6xl">
          <AgentLifecyclePage />
        </div>
      </main>
    </div>
  );
}

export default App;
