import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { X } from "lucide-react";
import { Button } from "./ui/button";
import { Card, CardContent } from "./ui/card";

interface DebugSnapshot {
  dt_ms: number;
  fps: number;
  current_offset: number;
  target_offset: number;
  smoothing_factor: number;
}

interface DebugOverlayProps {
  onClose: () => void;
}

export function DebugOverlay({ onClose }: DebugOverlayProps) {
  const [snapshot, setSnapshot] = useState<DebugSnapshot | null>(null);

  useEffect(() => {
    let active = true;

    const poll = async () => {
      try {
        const snap = await invoke<DebugSnapshot>("get_debug_snapshot");
        if (active) {
          setSnapshot(snap);
        }
      } catch (e) {
        console.error(e);
      }
    };

    poll();
    const interval = setInterval(poll, 16); // Poll at roughly 60fps to see changes

    return () => {
      active = false;
      clearInterval(interval);
    };
  }, []);

  if (!snapshot) return null;

  return (
    <Card className="fixed bottom-4 right-4 w-64 bg-background/90 backdrop-blur border-primary/20 shadow-lg z-50 overflow-hidden font-mono text-xs">
      <div className="flex items-center justify-between px-3 py-2 bg-muted/50 border-b">
        <span className="font-semibold text-primary">WM Debug</span>
        <Button variant="ghost" size="icon" className="h-5 w-5 hover:bg-destructive/20 hover:text-destructive" onClick={onClose}>
          <X className="h-3 w-3" />
        </Button>
      </div>
      <CardContent className="p-3 space-y-2">
        <div className="flex justify-between">
          <span className="text-muted-foreground">FPS</span>
          <span className={snapshot.fps < 50 ? "text-destructive" : "text-green-500"}>
            {snapshot.fps.toFixed(1)}
          </span>
        </div>
        <div className="flex justify-between">
          <span className="text-muted-foreground">Frame Time</span>
          <span>{snapshot.dt_ms.toFixed(2)} ms</span>
        </div>
        <div className="flex justify-between">
          <span className="text-muted-foreground">Smoothing</span>
          <span>{snapshot.smoothing_factor.toFixed(4)}</span>
        </div>

        <div className="pt-2 border-t mt-2"></div>

        <div className="flex justify-between">
          <span className="text-muted-foreground">Target X</span>
          <span>{snapshot.target_offset.toFixed(1)}</span>
        </div>
        <div className="flex justify-between">
          <span className="text-muted-foreground">Current X</span>
          <span>{snapshot.current_offset.toFixed(1)}</span>
        </div>
        <div className="flex justify-between">
          <span className="text-muted-foreground">Diff</span>
          <span className={Math.abs(snapshot.target_offset - snapshot.current_offset) > 0.5 ? "text-amber-500" : ""}>
            {Math.abs(snapshot.target_offset - snapshot.current_offset).toFixed(2)}
          </span>
        </div>
      </CardContent>
    </Card>
  );
}
