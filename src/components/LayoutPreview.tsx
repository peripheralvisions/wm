import { useState } from "react";
import { ChevronLeft, ChevronRight, Monitor, AppWindow, MoveHorizontal } from "lucide-react";
import { Badge } from "./ui/badge";
import { Button } from "./ui/button";

interface LayoutPreviewProps {
  gap: number;
  columnSizingMode: string;
  columnSizingValue: number;
  smoothScrolling: boolean;
  snapToWindow: boolean;
}

interface MockWindow {
  id: string;
  title: string;
  app: string;
  color: string;
  iconBg: string;
}

const MOCK_WINDOWS: MockWindow[] = [
  { id: "term", title: "alacritty — cargo run", app: "Terminal", color: "from-zinc-800 to-zinc-900", iconBg: "bg-emerald-500/20 text-emerald-400" },
  { id: "browser", title: "GitHub — Repository", app: "Browser", color: "from-blue-950/40 to-slate-900", iconBg: "bg-blue-500/20 text-blue-400" },
  { id: "code", title: "Visual Studio Code — wm.rs", app: "Editor", color: "from-indigo-950/40 to-slate-900", iconBg: "bg-indigo-500/20 text-indigo-400" },
  { id: "music", title: "Spotify — Daily Mix 1", app: "Music", color: "from-emerald-950/40 to-slate-900", iconBg: "bg-emerald-500/20 text-emerald-400" },
  { id: "notes", title: "Obsidian — Tiling WM Notes", app: "Notes", color: "from-purple-950/40 to-slate-900", iconBg: "bg-purple-500/20 text-purple-400" },
];

export function LayoutPreview({
  gap,
  columnSizingMode,
  columnSizingValue,
  smoothScrolling,
  snapToWindow,
}: LayoutPreviewProps) {
  const [scrollIndex, setScrollIndex] = useState(1);
  const [activeWindowId, setActiveWindowId] = useState("browser");

  // Calculate proportional visual values for preview container (container is ~500px wide viewport)
  // Real gap is 0-128px; in preview scale down to ~0-32px
  const visualGap = Math.max(4, Math.min(36, Math.round((gap / 128) * 32)));

  // Calculate width in percentage or proportional pixels
  let windowWidthPercent: number;
  if (columnSizingMode === "percent") {
    // 10% - 100% -> map to 30% - 85% visually for nice tiling strip presentation
    windowWidthPercent = 30 + (columnSizingValue / 100) * 55;
  } else {
    // 300px - 2000px -> map to 35% - 85% visually
    windowWidthPercent = 35 + ((columnSizingValue - 300) / 1700) * 50;
  }

  // Scroll offset calculation
  const totalOffsetPercent = scrollIndex * (windowWidthPercent + (visualGap / 5));

  const handlePrev = () => {
    setScrollIndex((prev) => Math.max(0, prev - 1));
  };

  const handleNext = () => {
    setScrollIndex((prev) => Math.min(MOCK_WINDOWS.length - 1, prev + 1));
  };

  return (
    <div className="rounded-xl border bg-card/60 backdrop-blur-sm p-4 space-y-3 shadow-sm">
      <div className="flex items-center justify-between">
        <div className="flex items-center gap-2">
          <div className="p-1.5 rounded-md bg-primary/10 text-primary">
            <Monitor className="h-4 w-4" />
          </div>
          <div>
            <div className="flex items-center gap-2">
              <span className="text-sm font-semibold tracking-tight">Interactive Tiling Strip Preview</span>
              <Badge variant="outline" className="text-[10px] font-mono px-1.5 py-0">
                {columnSizingMode === "percent" ? `${columnSizingValue}% width` : `${columnSizingValue}px width`}
              </Badge>
            </div>
            <p className="text-xs text-muted-foreground">
              Simulated infinite horizontal workspace layout
            </p>
          </div>
        </div>

        <div className="flex items-center gap-1">
          <Button
            variant="outline"
            size="icon"
            className="h-7 w-7 rounded-md"
            onClick={handlePrev}
            disabled={scrollIndex === 0}
            title="Scroll Left"
          >
            <ChevronLeft className="h-4 w-4" />
          </Button>
          <Button
            variant="outline"
            size="icon"
            className="h-7 w-7 rounded-md"
            onClick={handleNext}
            disabled={scrollIndex >= MOCK_WINDOWS.length - 1}
            title="Scroll Right"
          >
            <ChevronRight className="h-4 w-4" />
          </Button>
        </div>
      </div>

      {/* Screen Viewport Frame */}
      <div className="relative h-48 w-full overflow-hidden rounded-lg border bg-gradient-to-b from-background to-muted/40 p-3 shadow-inner">
        {/* Top Desktop Bar */}
        <div className="absolute top-0 left-0 right-0 h-6 bg-muted/70 border-b px-3 flex items-center justify-between text-[10px] text-muted-foreground font-mono select-none">
          <div className="flex items-center gap-2">
            <span className="inline-block h-2 w-2 rounded-full bg-emerald-500 animate-pulse"></span>
            <span>Workspace 1</span>
          </div>
          <div className="flex items-center gap-3">
            <span className="flex items-center gap-1">
              <MoveHorizontal className="h-3 w-3" />
              {snapToWindow ? "Snap Mode" : "Continuous"}
            </span>
            <span>Gap: {gap}px</span>
          </div>
        </div>

        {/* Scrollable Window Strip */}
        <div
          className={`absolute top-8 bottom-3 left-3 right-3 flex items-center ${
            smoothScrolling ? "transition-transform duration-300 ease-out" : "transition-none"
          }`}
          style={{
            transform: `translateX(calc(${50 - windowWidthPercent / 2}% - ${totalOffsetPercent}%))`,
            gap: `${visualGap}px`,
          }}
        >
          {MOCK_WINDOWS.map((win, idx) => {
            const isCenter = idx === scrollIndex;
            const isSelected = win.id === activeWindowId;

            return (
              <div
                key={win.id}
                onClick={() => {
                  setScrollIndex(idx);
                  setActiveWindowId(win.id);
                }}
                className={`group relative h-full shrink-0 rounded-lg border transition-all duration-200 cursor-pointer select-none flex flex-col overflow-hidden ${
                  win.color
                } ${
                  isCenter || isSelected
                    ? "ring-2 ring-primary border-primary shadow-md scale-100 opacity-100"
                    : "border-border/60 opacity-65 hover:opacity-90 scale-[0.97]"
                }`}
                style={{
                  width: `${windowWidthPercent}%`,
                  minWidth: "120px",
                }}
              >
                {/* Mock Window Header */}
                <div className="flex items-center justify-between px-2.5 py-1.5 border-b border-border/40 bg-background/50 text-[11px] font-medium">
                  <div className="flex items-center gap-1.5 truncate">
                    <span className="flex gap-1">
                      <span className="h-2 w-2 rounded-full bg-red-500/80" />
                      <span className="h-2 w-2 rounded-full bg-amber-500/80" />
                      <span className="h-2 w-2 rounded-full bg-green-500/80" />
                    </span>
                    <span className="truncate text-foreground/80 font-mono text-[10px] ml-1">{win.app}</span>
                  </div>
                  {isCenter && (
                    <Badge variant="secondary" className="h-3.5 px-1 text-[9px] font-mono leading-none">
                      Focus
                    </Badge>
                  )}
                </div>

                {/* Mock Window Content */}
                <div className="p-2.5 flex-1 flex flex-col justify-between text-xs">
                  <div className="space-y-1.5">
                    <div className="text-[10px] text-muted-foreground truncate font-mono">{win.title}</div>
                    <div className="space-y-1">
                      <div className="h-1.5 bg-muted-foreground/15 rounded-full w-3/4" />
                      <div className="h-1.5 bg-muted-foreground/15 rounded-full w-1/2" />
                      <div className="h-1.5 bg-muted-foreground/15 rounded-full w-5/6" />
                    </div>
                  </div>

                  <div className="flex items-center justify-between text-[9px] text-muted-foreground">
                    <span>Column #{idx + 1}</span>
                    <AppWindow className="h-3 w-3 opacity-40" />
                  </div>
                </div>
              </div>
            );
          })}
        </div>
      </div>

      {/* Preview Footer Metrics */}
      <div className="flex items-center justify-between text-[11px] text-muted-foreground px-1">
        <div className="flex items-center gap-2">
          <span>Alt + Scroll moves strip horizontally</span>
        </div>
        <div className="flex items-center gap-3 font-mono">
          <span>Gap: <strong className="text-foreground">{gap}px</strong></span>
          <span>Column: <strong className="text-foreground">{columnSizingValue}{columnSizingMode === "percent" ? "%" : "px"}</strong></span>
        </div>
      </div>
    </div>
  );
}
