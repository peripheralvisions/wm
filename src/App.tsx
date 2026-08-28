import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { Button } from "./components/ui/button";
import { Card, CardHeader, CardTitle, CardContent, CardDescription } from "./components/ui/card";
import { Label } from "./components/ui/label";
import { Slider } from "./components/ui/slider";
import { Switch } from "./components/ui/switch";
import { RadioGroup, RadioGroupItem } from "./components/ui/radio-group";
import { Separator } from "./components/ui/separator";
import "./App.css";

interface WmConfig {
  enabled: boolean;
  gap: number;
  scroll_speed: number;
  snap_to_window: boolean;
  column_sizing_mode: string;
  column_sizing_value: number;
  smooth_scrolling: boolean;
}

const DEFAULT_CONFIG: WmConfig = {
  enabled: true,
  gap: 16,
  scroll_speed: 100,
  snap_to_window: false,
  column_sizing_mode: "percent",
  column_sizing_value: 50.0,
  smooth_scrolling: true,
};

function App() {
  const [config, setConfig] = useState<WmConfig | null>(null);
  const [saved, setSaved] = useState(false);

  useEffect(() => {
    invoke<WmConfig>("get_config").then(setConfig);
  }, []);

  useEffect(() => {
    if (saved) {
      const timer = setTimeout(() => setSaved(false), 2000);
      return () => clearTimeout(timer);
    }
  }, [saved]);

  const handleSave = async () => {
    if (config) {
      await invoke("set_config", { config });
      setSaved(true);
    }
  };

  const handleReset = () => {
    setConfig(DEFAULT_CONFIG);
  };

  if (!config) return null;

  return (
    <main className="container mx-auto max-w-2xl p-8 pb-12 min-h-screen bg-background text-foreground">
      <div className="mb-8 space-y-2">
        <h1 className="text-4xl font-bold tracking-tight">Settings</h1>
        <p className="text-muted-foreground text-lg">
          Configure your scrollable tiling window manager.
        </p>
      </div>

      <div className="space-y-8">
        <Card>
          <CardHeader>
            <CardTitle>General</CardTitle>
            <CardDescription>Core functionality of the window manager.</CardDescription>
          </CardHeader>
          <CardContent>
            <div className="flex items-center justify-between">
              <div className="space-y-0.5">
                <Label className="text-base">Enable Tiling</Label>
                <p className="text-sm text-muted-foreground">
                  Toggle the window manager on or off globally.
                </p>
              </div>
              <Switch
                checked={config.enabled}
                onCheckedChange={(v) => setConfig({ ...config, enabled: v })}
              />
            </div>
          </CardContent>
        </Card>

        <Card>
          <CardHeader>
            <CardTitle>Layout</CardTitle>
            <CardDescription>Adjust how windows are positioned and sized.</CardDescription>
          </CardHeader>
          <CardContent className="space-y-6">
            <div className="space-y-4">
              <div className="flex items-center justify-between">
                <Label className="text-base">Window Gap</Label>
                <span className="text-sm text-muted-foreground w-12 text-right">{config.gap}px</span>
              </div>
              <Slider
                value={[config.gap]}
                onValueChange={(v) => setConfig({ ...config, gap: v[0] })}
                max={128}
                step={4}
              />
            </div>

            <Separator />

            <div className="space-y-4">
              <div className="space-y-1">
                <Label className="text-base">Column Sizing Mode</Label>
                <p className="text-sm text-muted-foreground">
                  Determine if new windows use percentage or fixed pixel width.
                </p>
              </div>
              <RadioGroup
                value={config.column_sizing_mode}
                onValueChange={(v) => setConfig({ ...config, column_sizing_mode: v })}
                className="flex flex-col space-y-2 mt-2"
              >
                <div className="flex items-center space-x-3">
                  <RadioGroupItem value="percent" id="r1" />
                  <Label htmlFor="r1" className="font-normal">Percentage (%)</Label>
                </div>
                <div className="flex items-center space-x-3">
                  <RadioGroupItem value="pixel" id="r2" />
                  <Label htmlFor="r2" className="font-normal">Fixed Pixels (px)</Label>
                </div>
              </RadioGroup>
            </div>

            <div className="space-y-4 pt-2">
              <div className="flex items-center justify-between">
                <Label className="text-base">Default Column Size</Label>
                <span className="text-sm text-muted-foreground w-12 text-right">
                  {config.column_sizing_value}{config.column_sizing_mode === "percent" ? "%" : "px"}
                </span>
              </div>
              {config.column_sizing_mode === "percent" ? (
                <Slider
                  value={[config.column_sizing_value]}
                  onValueChange={(v) => setConfig({ ...config, column_sizing_value: v[0] })}
                  min={10}
                  max={100}
                  step={5}
                />
              ) : (
                <Slider
                  value={[config.column_sizing_value]}
                  onValueChange={(v) => setConfig({ ...config, column_sizing_value: v[0] })}
                  min={300}
                  max={2000}
                  step={50}
                />
              )}
            </div>
          </CardContent>
        </Card>

        <Card>
          <CardHeader>
            <CardTitle>Scrolling & Navigation</CardTitle>
            <CardDescription>Configure how panning and focusing behaves.</CardDescription>
          </CardHeader>
          <CardContent className="space-y-6">
            <div className="flex items-center justify-between">
              <div className="space-y-0.5">
                <Label className="text-base">Smooth Scrolling</Label>
                <p className="text-sm text-muted-foreground">
                  Animate horizontal panning.
                </p>
              </div>
              <Switch
                checked={config.smooth_scrolling}
                onCheckedChange={(v) => setConfig({ ...config, smooth_scrolling: v })}
              />
            </div>

            <Separator />

            <div className="flex items-center justify-between">
              <div className="space-y-0.5">
                <Label className="text-base">Snap to Applications</Label>
                <p className="text-sm text-muted-foreground">
                  Alt+Scroll skips to next/previous window instead of continuous scrolling.
                </p>
              </div>
              <Switch
                checked={config.snap_to_window}
                onCheckedChange={(v) => setConfig({ ...config, snap_to_window: v })}
              />
            </div>

            <Separator />

            <div className={config.snap_to_window ? "opacity-50 pointer-events-none transition-opacity" : "transition-opacity"}>
              <div className="space-y-4">
                <div className="flex items-center justify-between">
                  <Label className="text-base">Scroll Speed</Label>
                  <span className="text-sm text-muted-foreground w-12 text-right">{config.scroll_speed}px</span>
                </div>
                <Slider
                  value={[config.scroll_speed]}
                  onValueChange={(v) => setConfig({ ...config, scroll_speed: v[0] })}
                  min={10}
                  max={500}
                  step={10}
                />
              </div>
            </div>
          </CardContent>
        </Card>

        <div className="flex justify-between items-center pt-4">
          <Button variant="ghost" onClick={handleReset}>Reset to Defaults</Button>
          <div className="flex items-center gap-4">
            {saved && <span className="text-sm text-green-500 font-medium">Saved successfully!</span>}
            <Button size="lg" onClick={handleSave} className="min-w-32">
              Save Settings
            </Button>
          </div>
        </div>
      </div>
    </main>
  );
}

export default App;
