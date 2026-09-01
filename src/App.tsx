import { useEffect, useState, useCallback } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import {
  LayoutGrid,
  Sliders,
  MoveHorizontal,
  Keyboard,
  Sun,
  Moon,
  Laptop,
  RotateCcw,
  Save,
  Check,
  Bug,
  Activity,
  Layers,
  Eye,
  EyeOff,
  Info,
  Zap,
} from "lucide-react";

import { Button } from "./components/ui/button";
import { Card, CardHeader, CardTitle, CardContent, CardDescription } from "./components/ui/card";
import { Label } from "./components/ui/label";
import { Slider } from "./components/ui/slider";
import { Switch } from "./components/ui/switch";
import { RadioGroup, RadioGroupItem } from "./components/ui/radio-group";
import { Separator } from "./components/ui/separator";
import { Badge } from "./components/ui/badge";
import { Tabs, TabsList, TabsTrigger, TabsContent } from "./components/ui/tabs";
import { DebugOverlay } from "./components/DebugOverlay";
import { LayoutPreview } from "./components/LayoutPreview";
import "./App.css";

interface WmConfig {
  enabled: boolean;
  gap: number;
  scroll_speed: number;
  snap_to_window: boolean;
  snap_speed: number;
  column_sizing_mode: string;
  column_sizing_value: number;
  smooth_scrolling: boolean;
  block_alt_menu: boolean;
}

const DEFAULT_CONFIG: WmConfig = {
  enabled: true,
  gap: 16,
  scroll_speed: 100,
  snap_to_window: false,
  snap_speed: 35,
  column_sizing_mode: "percent",
  column_sizing_value: 50.0,
  smooth_scrolling: true,
  block_alt_menu: true,
};

type ThemeMode = "dark" | "light" | "system";

export function App() {
  const [config, setConfig] = useState<WmConfig | null>(null);
  const [initialConfig, setInitialConfig] = useState<WmConfig | null>(null);
  const [saved, setSaved] = useState(false);
  const [isSaving, setIsSaving] = useState(false);
  const [debugEnabled, setDebugEnabled] = useState(false);
  const [activeTab, setActiveTab] = useState("general");

  // User UI Preferences (persisted in localStorage)
  const [theme, setTheme] = useState<ThemeMode>(() => {
    return (localStorage.getItem("wm-theme") as ThemeMode) || "dark";
  });

  const [showPreview, setShowPreview] = useState<boolean>(() => {
    const saved = localStorage.getItem("wm-show-preview");
    return saved !== null ? saved === "true" : true;
  });

  // Apply Theme
  useEffect(() => {
    const root = document.documentElement;
    localStorage.setItem("wm-theme", theme);

    if (theme === "system") {
      const systemDark = window.matchMedia("(prefers-color-scheme: dark)").matches;
      if (systemDark) {
        root.classList.add("dark");
      } else {
        root.classList.remove("dark");
      }
    } else if (theme === "dark") {
      root.classList.add("dark");
    } else {
      root.classList.remove("dark");
    }
  }, [theme]);

  // Listen for system theme changes if set to system
  useEffect(() => {
    if (theme !== "system") return;
    const media = window.matchMedia("(prefers-color-scheme: dark)");
    const listener = (e: MediaQueryListEvent) => {
      if (e.matches) {
        document.documentElement.classList.add("dark");
      } else {
        document.documentElement.classList.remove("dark");
      }
    };
    media.addEventListener("change", listener);
    return () => media.removeEventListener("change", listener);
  }, [theme]);

  const toggleTheme = () => {
    setTheme((prev) => {
      if (prev === "dark") return "light";
      if (prev === "light") return "system";
      return "dark";
    });
  };

  const togglePreview = () => {
    setShowPreview((prev) => {
      const next = !prev;
      localStorage.setItem("wm-show-preview", String(next));
      return next;
    });
  };

  // Load WM Config & Debug state
  useEffect(() => {
    invoke<WmConfig>("get_config").then((cfg) => {
      setConfig(cfg);
      setInitialConfig(cfg);
    });

    invoke<boolean>("get_debug_state").then(setDebugEnabled);

    const unlisten = listen<boolean>("debug-toggle", (event) => {
      setDebugEnabled(event.payload);
    });

    return () => {
      unlisten.then((fn) => fn());
    };
  }, []);

  useEffect(() => {
    if (saved) {
      const timer = setTimeout(() => setSaved(false), 2500);
      return () => clearTimeout(timer);
    }
  }, [saved]);

  const handleSave = useCallback(async () => {
    if (config) {
      setIsSaving(true);
      try {
        await invoke("set_config", { config });
        setInitialConfig(config);
        setSaved(true);
      } catch (err) {
        console.error("Failed to save config:", err);
      } finally {
        setIsSaving(false);
      }
    }
  }, [config]);

  const handleReset = () => {
    setConfig(DEFAULT_CONFIG);
  };

  // Keyboard shortcut for saving (Ctrl+S / Cmd+S)
  useEffect(() => {
    const handleKeyDown = (e: KeyboardEvent) => {
      if ((e.ctrlKey || e.metaKey) && e.key === "s") {
        e.preventDefault();
        handleSave();
      }
    };
    window.addEventListener("keydown", handleKeyDown);
    return () => window.removeEventListener("keydown", handleKeyDown);
  }, [handleSave]);

  const isModified = JSON.stringify(config) !== JSON.stringify(initialConfig);

  if (!config) {
    return (
      <div className="flex h-screen w-screen items-center justify-center bg-background text-foreground">
        <div className="flex flex-col items-center gap-3">
          <div className="h-8 w-8 animate-spin rounded-full border-4 border-primary border-t-transparent" />
          <p className="text-sm font-medium text-muted-foreground">Loading WM settings...</p>
        </div>
      </div>
    );
  }

  return (
    <div className="min-h-screen bg-background text-foreground flex flex-col font-sans">
      {/* Top App Header */}
      <header className="sticky top-0 z-40 border-b bg-background/80 backdrop-blur-md px-6 py-3.5 flex items-center justify-between">
        <div className="flex items-center gap-3">
          <div className="flex h-9 w-9 items-center justify-center rounded-lg bg-primary text-primary-foreground shadow-sm">
            <LayoutGrid className="h-5 w-5" />
          </div>
          <div>
            <div className="flex items-center gap-2">
              <h1 className="text-base font-semibold tracking-tight leading-none">Tiling Window Manager</h1>
              <Badge
                variant={config.enabled ? "success" : "secondary"}
                className="gap-1 px-2 py-0 text-[10px] font-medium"
              >
                <span
                  className={`h-1.5 w-1.5 rounded-full ${
                    config.enabled ? "bg-emerald-500 animate-pulse" : "bg-muted-foreground"
                  }`}
                />
                {config.enabled ? "Active" : "Disabled"}
              </Badge>
              {isModified && (
                <Badge variant="outline" className="text-[10px] text-amber-500 border-amber-500/30">
                  Unsaved Changes
                </Badge>
              )}
            </div>
            <p className="text-xs text-muted-foreground mt-0.5">Scrollable Tiling Window Manager for Windows 11</p>
          </div>
        </div>

        {/* Header Action Controls */}
        <div className="flex items-center gap-2">
          {/* Theme Toggle Button */}
          <Button
            variant="ghost"
            size="icon"
            className="h-8 w-8 text-muted-foreground hover:text-foreground"
            onClick={toggleTheme}
            title={`Theme: ${theme.charAt(0).toUpperCase() + theme.slice(1)} (Click to switch)`}
          >
            {theme === "dark" ? (
              <Moon className="h-4 w-4" />
            ) : theme === "light" ? (
              <Sun className="h-4 w-4" />
            ) : (
              <Laptop className="h-4 w-4" />
            )}
          </Button>

          {/* Debug Overlay Toggle */}
          <Button
            variant={debugEnabled ? "secondary" : "ghost"}
            size="icon"
            className={`h-8 w-8 ${debugEnabled ? "text-primary border border-primary/20" : "text-muted-foreground"}`}
            onClick={() => setDebugEnabled(!debugEnabled)}
            title="Toggle Debug Monitor"
          >
            <Bug className="h-4 w-4" />
          </Button>

          <Separator orientation="vertical" className="h-5 mx-1" />

          {/* Reset Button */}
          <Button
            variant="ghost"
            size="sm"
            onClick={handleReset}
            className="h-8 gap-1.5 text-xs text-muted-foreground hover:text-foreground"
          >
            <RotateCcw className="h-3.5 w-3.5" />
            Reset
          </Button>

          {/* Save Button */}
          <Button
            size="sm"
            onClick={handleSave}
            disabled={isSaving}
            className={`h-8 gap-1.5 text-xs transition-all ${
              saved ? "bg-emerald-600 hover:bg-emerald-700 text-white" : ""
            }`}
          >
            {saved ? (
              <>
                <Check className="h-3.5 w-3.5" />
                Saved
              </>
            ) : isSaving ? (
              <>
                <div className="h-3 w-3 animate-spin rounded-full border-2 border-current border-t-transparent" />
                Saving...
              </>
            ) : (
              <>
                <Save className="h-3.5 w-3.5" />
                Save Settings
              </>
            )}
          </Button>
        </div>
      </header>

      {/* Main Content Area with Tabs */}
      <main className="flex-1 container max-w-4xl mx-auto p-6 pb-16">
        <Tabs value={activeTab} onValueChange={setActiveTab} className="space-y-6">
          <div className="flex items-center justify-between">
            <TabsList className="grid grid-cols-4 w-full max-w-lg h-9">
              <TabsTrigger value="general" className="gap-2 text-xs">
                <Sliders className="h-3.5 w-3.5" />
                General
              </TabsTrigger>
              <TabsTrigger value="layout" className="gap-2 text-xs">
                <Layers className="h-3.5 w-3.5" />
                Layout & Sizing
              </TabsTrigger>
              <TabsTrigger value="scrolling" className="gap-2 text-xs">
                <MoveHorizontal className="h-3.5 w-3.5" />
                Scrolling
              </TabsTrigger>
              <TabsTrigger value="shortcuts" className="gap-2 text-xs">
                <Keyboard className="h-3.5 w-3.5" />
                Shortcuts
              </TabsTrigger>
            </TabsList>

            {/* Quick Preview Toggle button in header right */}
            {activeTab === "layout" && (
              <Button
                variant="outline"
                size="sm"
                onClick={togglePreview}
                className="h-8 gap-1.5 text-xs"
              >
                {showPreview ? <EyeOff className="h-3.5 w-3.5" /> : <Eye className="h-3.5 w-3.5" />}
                {showPreview ? "Hide Preview" : "Show Preview"}
              </Button>
            )}
          </div>

          {/* TAB 1: GENERAL */}
          <TabsContent value="general" className="space-y-6">
            <Card>
              <CardHeader className="pb-4">
                <div className="flex items-center gap-2">
                  <div className="p-1.5 rounded-md bg-primary/10 text-primary">
                    <Zap className="h-4 w-4" />
                  </div>
                  <div>
                    <CardTitle className="text-base">Core Functionality</CardTitle>
                    <CardDescription>Master switches and Windows shell behavior integration.</CardDescription>
                  </div>
                </div>
              </CardHeader>
              <CardContent className="space-y-5">
                <div className="flex items-center justify-between">
                  <div className="space-y-0.5 pr-4">
                    <Label className="text-sm font-medium">Enable Tiling Window Manager</Label>
                    <p className="text-xs text-muted-foreground">
                      Enable or disable global window management, automatic tiling, and strip panning.
                    </p>
                  </div>
                  <Switch
                    checked={config.enabled}
                    onCheckedChange={(v) => setConfig({ ...config, enabled: v })}
                  />
                </div>

                <Separator />

                <div className="flex items-center justify-between">
                  <div className="space-y-0.5 pr-4">
                    <div className="flex items-center gap-2">
                      <Label className="text-sm font-medium">Block Application Alt Menus</Label>
                      <Badge variant="outline" className="text-[10px]">Recommended</Badge>
                    </div>
                    <p className="text-xs text-muted-foreground">
                      Prevents tapping <kbd className="px-1.5 py-0.5 text-[10px] font-mono bg-muted rounded border">Alt</kbd> from accidentally focusing menu bars in apps like Firefox, File Explorer, or Notepad when using Alt-drag or Alt-scroll.
                    </p>
                  </div>
                  <Switch
                    checked={config.block_alt_menu}
                    onCheckedChange={(v) => setConfig({ ...config, block_alt_menu: v })}
                  />
                </div>
              </CardContent>
            </Card>

            <Card>
              <CardHeader className="pb-4">
                <div className="flex items-center gap-2">
                  <div className="p-1.5 rounded-md bg-primary/10 text-primary">
                    <Activity className="h-4 w-4" />
                  </div>
                  <div>
                    <CardTitle className="text-base">System Status & Architecture</CardTitle>
                    <CardDescription>Low-level hooks and DWM frame management.</CardDescription>
                  </div>
                </div>
              </CardHeader>
              <CardContent className="space-y-3 text-xs">
                <div className="grid grid-cols-2 gap-3">
                  <div className="p-3 rounded-lg border bg-muted/30 space-y-1">
                    <span className="text-muted-foreground font-medium">Hook Architecture</span>
                    <p className="text-foreground font-semibold">Low-level WinEvent + WH_MOUSE_LL</p>
                    <p className="text-[11px] text-muted-foreground">High-performance native Windows API</p>
                  </div>
                  <div className="p-3 rounded-lg border bg-muted/30 space-y-1">
                    <span className="text-muted-foreground font-medium">Artifact Reduction</span>
                    <p className="text-foreground font-semibold">DWM Cloak & DeferWindowPos</p>
                    <p className="text-[11px] text-muted-foreground">Zero-flicker frame layout updates</p>
                  </div>
                </div>
              </CardContent>
            </Card>
          </TabsContent>

          {/* TAB 2: LAYOUT & SIZING */}
          <TabsContent value="layout" className="space-y-6">
            {/* Interactive Live Layout Preview Widget (Toggleable) */}
            {showPreview && (
              <LayoutPreview
                gap={config.gap}
                columnSizingMode={config.column_sizing_mode}
                columnSizingValue={config.column_sizing_value}
                smoothScrolling={config.smooth_scrolling}
                snapToWindow={config.snap_to_window}
                snapSpeed={config.snap_speed}
              />
            )}

            <Card>
              <CardHeader className="pb-4">
                <div className="flex items-center justify-between">
                  <div className="flex items-center gap-2">
                    <div className="p-1.5 rounded-md bg-primary/10 text-primary">
                      <Sliders className="h-4 w-4" />
                    </div>
                    <div>
                      <CardTitle className="text-base">Window Spacing & Gaps</CardTitle>
                      <CardDescription>Control the margin between tiled application columns.</CardDescription>
                    </div>
                  </div>
                  <Badge variant="outline" className="font-mono text-xs">
                    {config.gap} px
                  </Badge>
                </div>
              </CardHeader>
              <CardContent className="space-y-4">
                <div className="space-y-2">
                  <div className="flex justify-between text-xs text-muted-foreground">
                    <span>Compact (0px)</span>
                    <span>Default (16px)</span>
                    <span>Spacious (128px)</span>
                  </div>
                  <Slider
                    value={[config.gap]}
                    onValueChange={(v) => setConfig({ ...config, gap: v[0] })}
                    min={0}
                    max={128}
                    step={2}
                    className="py-1"
                  />
                </div>
              </CardContent>
            </Card>

            <Card>
              <CardHeader className="pb-4">
                <div className="flex items-center gap-2">
                  <div className="p-1.5 rounded-md bg-primary/10 text-primary">
                    <LayoutGrid className="h-4 w-4" />
                  </div>
                  <div>
                    <CardTitle className="text-base">Column Width Sizing</CardTitle>
                    <CardDescription>Determine how new windows are dimensioned horizontally on the strip.</CardDescription>
                  </div>
                </div>
              </CardHeader>
              <CardContent className="space-y-6">
                {/* Sizing Mode Selection */}
                <div className="space-y-3">
                  <Label className="text-sm font-medium">Sizing Unit</Label>
                  <RadioGroup
                    value={config.column_sizing_mode}
                    onValueChange={(v) => {
                      // Adjust default value when switching mode
                      const defaultValue = v === "percent" ? 50 : 960;
                      setConfig({
                        ...config,
                        column_sizing_mode: v,
                        column_sizing_value: defaultValue,
                      });
                    }}
                    className="grid grid-cols-2 gap-3"
                  >
                    <Label
                      htmlFor="r-percent"
                      className={`flex flex-col items-start gap-1 p-3 rounded-lg border cursor-pointer transition-colors ${
                        config.column_sizing_mode === "percent"
                          ? "border-primary bg-primary/5 ring-1 ring-primary"
                          : "border-border hover:bg-muted/50"
                      }`}
                    >
                      <div className="flex items-center gap-2 w-full">
                        <RadioGroupItem value="percent" id="r-percent" />
                        <span className="font-semibold text-xs">Percentage of Screen</span>
                      </div>
                      <span className="text-[11px] text-muted-foreground pl-6">
                        Windows scale dynamically with display resolution (e.g. 50% width).
                      </span>
                    </Label>

                    <Label
                      htmlFor="r-pixel"
                      className={`flex flex-col items-start gap-1 p-3 rounded-lg border cursor-pointer transition-colors ${
                        config.column_sizing_mode === "pixel"
                          ? "border-primary bg-primary/5 ring-1 ring-primary"
                          : "border-border hover:bg-muted/50"
                      }`}
                    >
                      <div className="flex items-center gap-2 w-full">
                        <RadioGroupItem value="pixel" id="r-pixel" />
                        <span className="font-semibold text-xs">Fixed Pixels</span>
                      </div>
                      <span className="text-[11px] text-muted-foreground pl-6">
                        Consistent absolute width regardless of monitor size (e.g. 960px).
                      </span>
                    </Label>
                  </RadioGroup>
                </div>

                <Separator />

                {/* Default Column Size Slider */}
                <div className="space-y-3">
                  <div className="flex items-center justify-between">
                    <div>
                      <Label className="text-sm font-medium">Default Column Width</Label>
                      <p className="text-xs text-muted-foreground">Initial size for newly launched applications.</p>
                    </div>
                    <Badge variant="secondary" className="font-mono text-xs px-2 py-0.5">
                      {config.column_sizing_value}
                      {config.column_sizing_mode === "percent" ? "%" : " px"}
                    </Badge>
                  </div>

                  {config.column_sizing_mode === "percent" ? (
                    <div className="space-y-2">
                      <div className="flex justify-between text-xs text-muted-foreground">
                        <span>Narrow (20%)</span>
                        <span>Half (50%)</span>
                        <span>Full (100%)</span>
                      </div>
                      <Slider
                        value={[config.column_sizing_value]}
                        onValueChange={(v) => setConfig({ ...config, column_sizing_value: v[0] })}
                        min={10}
                        max={100}
                        step={5}
                        className="py-1"
                      />
                    </div>
                  ) : (
                    <div className="space-y-2">
                      <div className="flex justify-between text-xs text-muted-foreground">
                        <span>300 px</span>
                        <span>1000 px</span>
                        <span>2000 px</span>
                      </div>
                      <Slider
                        value={[config.column_sizing_value]}
                        onValueChange={(v) => setConfig({ ...config, column_sizing_value: v[0] })}
                        min={300}
                        max={2000}
                        step={50}
                        className="py-1"
                      />
                    </div>
                  )}
                </div>
              </CardContent>
            </Card>
          </TabsContent>

          {/* TAB 3: SCROLLING & MOTION */}
          <TabsContent value="scrolling" className="space-y-6">
            <Card>
              <CardHeader className="pb-4">
                <div className="flex items-center gap-2">
                  <div className="p-1.5 rounded-md bg-primary/10 text-primary">
                    <MoveHorizontal className="h-4 w-4" />
                  </div>
                  <div>
                    <CardTitle className="text-base">Panning & Strip Motion</CardTitle>
                    <CardDescription>Configure horizontal workspace navigation and physics.</CardDescription>
                  </div>
                </div>
              </CardHeader>
              <CardContent className="space-y-5">
                <div className="flex items-center justify-between">
                  <div className="space-y-0.5 pr-4">
                    <div className="flex items-center gap-2">
                      <Label className="text-sm font-medium">Smooth Kinetic Scrolling</Label>
                      <Badge variant="outline" className="text-[10px]">60+ FPS</Badge>
                    </div>
                    <p className="text-xs text-muted-foreground">
                      Animate horizontal workspace panning smoothly when scrolling with Alt+Wheel.
                    </p>
                  </div>
                  <Switch
                    checked={config.smooth_scrolling}
                    onCheckedChange={(v) => setConfig({ ...config, smooth_scrolling: v })}
                  />
                </div>

                <Separator />

                <div className="flex items-center justify-between">
                  <div className="space-y-0.5 pr-4">
                    <div className="flex items-center gap-2">
                      <Label className="text-sm font-medium">Snap to Applications</Label>
                      <Badge variant="outline" className="text-[10px]">Discrete Steps</Badge>
                    </div>
                    <p className="text-xs text-muted-foreground">
                      Alt+Scroll snaps directly to the next/previous window center instead of free-form continuous scrolling.
                    </p>
                  </div>
                  <Switch
                    checked={config.snap_to_window}
                    onCheckedChange={(v) => setConfig({ ...config, snap_to_window: v })}
                  />
                </div>

                <Separator />

                {/* Snap Speed / Motion Responsiveness Setting */}
                <div
                  className={`space-y-3 transition-opacity ${
                    !config.smooth_scrolling ? "opacity-40 pointer-events-none" : "opacity-100"
                  }`}
                >
                  <div className="flex items-center justify-between">
                    <div>
                      <div className="flex items-center gap-2">
                        <Label className="text-sm font-medium">Snap & Motion Speed</Label>
                        <Badge variant="outline" className="text-[10px]">
                          {config.snap_speed <= 20
                            ? "Slow & Gentle"
                            : config.snap_speed <= 40
                            ? "Smooth & Balanced"
                            : config.snap_speed <= 65
                            ? "Responsive"
                            : "Snappy"}
                        </Badge>
                      </div>
                      <p className="text-xs text-muted-foreground">
                        Control how snappy or smooth window snapping, panning, and focus transitions feel.
                      </p>
                    </div>
                    <Badge variant="secondary" className="font-mono text-xs">
                      {config.snap_speed}
                    </Badge>
                  </div>
                  <div className="space-y-2">
                    <div className="flex justify-between text-xs text-muted-foreground">
                      <span>Slower / Gentle (10)</span>
                      <span>Default (35)</span>
                      <span>Snappy (100)</span>
                    </div>
                    <Slider
                      value={[config.snap_speed]}
                      onValueChange={(v) => setConfig({ ...config, snap_speed: v[0] })}
                      min={10}
                      max={100}
                      step={5}
                      className="py-1"
                    />
                  </div>
                </div>

                <Separator />

                {/* Scroll Speed Setting */}
                <div
                  className={`space-y-3 transition-opacity ${
                    config.snap_to_window ? "opacity-40 pointer-events-none" : "opacity-100"
                  }`}
                >
                  <div className="flex items-center justify-between">
                    <div>
                      <Label className="text-sm font-medium">Continuous Scroll Speed</Label>
                      <p className="text-xs text-muted-foreground">Pixel distance traveled per mouse scroll notch.</p>
                    </div>
                    <Badge variant="secondary" className="font-mono text-xs">
                      {config.scroll_speed} px/notch
                    </Badge>
                  </div>
                  <div className="space-y-2">
                    <div className="flex justify-between text-xs text-muted-foreground">
                      <span>Gentle (20px)</span>
                      <span>Balanced (100px)</span>
                      <span>Fast (500px)</span>
                    </div>
                    <Slider
                      value={[config.scroll_speed]}
                      onValueChange={(v) => setConfig({ ...config, scroll_speed: v[0] })}
                      min={10}
                      max={500}
                      step={10}
                      className="py-1"
                    />
                  </div>
                </div>
              </CardContent>
            </Card>
          </TabsContent>

          {/* TAB 4: SHORTCUTS & USER GUIDE */}
          <TabsContent value="shortcuts" className="space-y-6">
            <Card>
              <CardHeader className="pb-4">
                <div className="flex items-center gap-2">
                  <div className="p-1.5 rounded-md bg-primary/10 text-primary">
                    <Keyboard className="h-4 w-4" />
                  </div>
                  <div>
                    <CardTitle className="text-base">Window Manager Controls</CardTitle>
                    <CardDescription>Keyboard and mouse bindings for interacting with the strip.</CardDescription>
                  </div>
                </div>
              </CardHeader>
              <CardContent className="space-y-4">
                <div className="divide-y rounded-lg border">
                  <div className="flex items-center justify-between p-3.5 text-xs">
                    <div className="space-y-0.5">
                      <span className="font-semibold text-foreground">Pan Horizontal Strip</span>
                      <p className="text-muted-foreground">Scroll smoothly left and right across open windows</p>
                    </div>
                    <div className="flex items-center gap-1.5">
                      <kbd className="px-2 py-1 text-xs font-mono bg-muted rounded border shadow-sm">Alt</kbd>
                      <span className="text-muted-foreground">+</span>
                      <kbd className="px-2 py-1 text-xs font-mono bg-muted rounded border shadow-sm">Mouse Wheel</kbd>
                    </div>
                  </div>

                  <div className="flex items-center justify-between p-3.5 text-xs">
                    <div className="space-y-0.5">
                      <span className="font-semibold text-foreground">Manual Window Resize</span>
                      <p className="text-muted-foreground">Resize any tiled column using border drag</p>
                    </div>
                    <div className="flex items-center gap-1.5">
                      <kbd className="px-2 py-1 text-xs font-mono bg-muted rounded border shadow-sm">Window Border</kbd>
                      <span className="text-muted-foreground">+</span>
                      <kbd className="px-2 py-1 text-xs font-mono bg-muted rounded border shadow-sm">Drag</kbd>
                    </div>
                  </div>

                  <div className="flex items-center justify-between p-3.5 text-xs">
                    <div className="space-y-0.5">
                      <span className="font-semibold text-foreground">Save Configuration</span>
                      <p className="text-muted-foreground">Save updated settings to active session</p>
                    </div>
                    <div className="flex items-center gap-1.5">
                      <kbd className="px-2 py-1 text-xs font-mono bg-muted rounded border shadow-sm">Ctrl</kbd>
                      <span className="text-muted-foreground">+</span>
                      <kbd className="px-2 py-1 text-xs font-mono bg-muted rounded border shadow-sm">S</kbd>
                    </div>
                  </div>

                  <div className="flex items-center justify-between p-3.5 text-xs">
                    <div className="space-y-0.5">
                      <span className="font-semibold text-foreground">Toggle Debug Overlay</span>
                      <p className="text-muted-foreground">Show real-time FPS and offset physics monitor</p>
                    </div>
                    <div className="flex items-center gap-1.5">
                      <kbd className="px-2 py-1 text-xs font-mono bg-muted rounded border shadow-sm">Tray Icon Menu</kbd>
                    </div>
                  </div>
                </div>
              </CardContent>
            </Card>

            <Card className="border-primary/20 bg-primary/5">
              <CardContent className="p-4 flex items-start gap-3 text-xs">
                <div className="p-1 rounded bg-primary/10 text-primary shrink-0 mt-0.5">
                  <Info className="h-4 w-4" />
                </div>
                <div className="space-y-1">
                  <p className="font-semibold text-foreground">How Scrollable Tiling Works</p>
                  <p className="text-muted-foreground leading-relaxed">
                    Unlike traditional grid-based tiling window managers, this scrollable tiling window manager places your windows in an infinite horizontal strip. Windows maintain their desired width without getting cramped when you open many applications. Simply hold <strong className="text-foreground">Alt</strong> and scroll to pan between tasks.
                  </p>
                </div>
              </CardContent>
            </Card>
          </TabsContent>
        </Tabs>
      </main>

      {/* Floating Debug Overlay */}
      {debugEnabled && <DebugOverlay onClose={() => setDebugEnabled(false)} />}
    </div>
  );
}

export default App;
