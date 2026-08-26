import { lazy, Suspense } from "react";
import {
  BrowserRouter,
  Navigate,
  Route,
  Routes,
  useLocation,
} from "react-router";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { useTranslation } from "react-i18next";
import { AppShell } from "./components/AppShell";
import { ThemeProvider } from "./components/ThemeProvider";
import { AuthBootstrap } from "./components/AuthBootstrap";
import { SettingsBootstrap } from "./components/SettingsBootstrap";
import { TauriGuardBanner } from "./components/TauriGuardBanner";
import { DesktopChrome } from "./components/DesktopChrome";
import { HotkeyProvider } from "./components/HotkeyProvider";
import { DeepLinkBootstrap } from "./components/DeepLinkAndUpdaterBootstrap";
import { StreamingBootstrap } from "./components/StreamingBootstrap";
import { OnboardingWizard } from "./components/OnboardingWizard";
import { LaunchErrorBanner } from "./components/LaunchErrorBanner";
import { UpdateBanner } from "./components/UpdateBanner";
import { RaidBanner } from "./components/RaidBanner";
import { DebugDiagnosticsBootstrap } from "./components/DebugDiagnosticsBootstrap";
import {
  ChannelPointsPollOverlay,
  isPollOverlayWindow,
} from "./components/ChannelPointsPollOverlay";
import {
  ChannelPointsHud,
  isPointsHudOverlayWindow,
} from "./components/ChannelPointsHud";
import { ChannelPointsHudSync } from "./components/ChannelPointsHudSync";
import { settingsTabFromPath } from "./lib/settings/tabs";
import { AppErrorBoundary, SentryBootstrap } from "./lib/sentry";
import "./styles/global.css";

const FollowedPage = lazy(() =>
  import("./pages/BrowsePages").then((module) => ({ default: module.FollowedPage })),
);
const StreamsPage = lazy(() =>
  import("./pages/BrowsePages").then((module) => ({ default: module.StreamsPage })),
);
const WatchingPage = lazy(() =>
  import("./pages/BrowsePages").then((module) => ({ default: module.WatchingPage })),
);
const AboutPage = lazy(() =>
  import("./pages/BrowsePages").then((module) => ({ default: module.AboutPage })),
);
const ChannelPage = lazy(() =>
  import("./pages/BrowseExtraPages").then((module) => ({ default: module.ChannelPage })),
);
const GameStreamsPage = lazy(() =>
  import("./pages/BrowseExtraPages").then((module) => ({ default: module.GameStreamsPage })),
);
const GamesPage = lazy(() =>
  import("./pages/BrowseExtraPages").then((module) => ({ default: module.GamesPage })),
);
const SearchPage = lazy(() =>
  import("./pages/BrowseExtraPages").then((module) => ({ default: module.SearchPage })),
);
const TeamPage = lazy(() =>
  import("./pages/BrowseExtraPages").then((module) => ({ default: module.TeamPage })),
);
const TeamsSearchPage = lazy(() =>
  import("./pages/BrowseExtraPages").then((module) => ({ default: module.TeamsSearchPage })),
);
const SettingsPage = lazy(() =>
  import("./pages/SettingsPage").then((module) => ({ default: module.SettingsPage })),
);
const MultistreamPage = lazy(() =>
  import("./pages/MultistreamPage").then((module) => ({ default: module.MultistreamPage })),
);
const DebugOutputSettings = lazy(() =>
  import("./components/DebugOutputSettings").then((module) => ({
    default: module.DebugOutputSettings,
  })),
);

const queryClient = new QueryClient({
  defaultOptions: {
    queries: {
      retry: 1,
      staleTime: 30_000,
      refetchOnWindowFocus: false,
    },
  },
});

function SettingsRoute() {
  const location = useLocation();
  const showDebugFilters = settingsTabFromPath(location.pathname) === "general";

  return (
    <>
      <SettingsPage />
      {showDebugFilters ? (
        <section className="settings" aria-label="Debug output filters">
          <div className="settings__layout">
            <div className="settings__nav" aria-hidden="true" />
            <div className="settings__main">
              <div className="settings__group">
                <DebugOutputSettings />
              </div>
            </div>
          </div>
        </section>
      ) : null}
    </>
  );
}

function AppRoutes() {
  const { t } = useTranslation("errors");
  return (
    // One failing page must not white-screen the whole app; reporting remains
    // opt-in and loads the Sentry SDK only when telemetry is actually used.
    <AppErrorBoundary
      fallback={
        <p className="muted" role="alert" style={{ padding: "2rem" }}>
          {t("generic")}
        </p>
      }
    >
      <Suspense fallback={null}>
        <Routes>
          <Route path="/" element={<FollowedPage />} />
          <Route path="/streams" element={<StreamsPage />} />
          <Route path="/games" element={<GamesPage />} />
          <Route path="/games/:gameId" element={<GameStreamsPage />} />
          <Route path="/search" element={<SearchPage />} />
          <Route path="/teams" element={<TeamsSearchPage />} />
          <Route path="/channel/:login" element={<ChannelPage />} />
          <Route path="/team/:teamName" element={<TeamPage />} />
          <Route path="/watching" element={<WatchingPage />} />
          <Route path="/multistream" element={<MultistreamPage />} />
          <Route path="/settings/*" element={<SettingsRoute />} />
          <Route path="/about" element={<AboutPage />} />
          <Route path="*" element={<Navigate to="/" replace />} />
        </Routes>
      </Suspense>
    </AppErrorBoundary>
  );
}

function isRaidOverlay() {
  return new URLSearchParams(window.location.search).get("overlay") === "raid";
}

export default function App() {
  if (isRaidOverlay()) {
    return (
      <ThemeProvider>
        <SettingsBootstrap>
          <RaidBanner />
        </SettingsBootstrap>
      </ThemeProvider>
    );
  }

  if (isPollOverlayWindow()) {
    return (
      <ThemeProvider>
        <SettingsBootstrap>
          <ChannelPointsPollOverlay />
        </SettingsBootstrap>
      </ThemeProvider>
    );
  }

  if (isPointsHudOverlayWindow()) {
    return (
      <ThemeProvider>
        <SettingsBootstrap>
          <ChannelPointsHud />
        </SettingsBootstrap>
      </ThemeProvider>
    );
  }

  return (
    <QueryClientProvider client={queryClient}>
      <ThemeProvider>
        <SettingsBootstrap>
          <DebugDiagnosticsBootstrap>
            <SentryBootstrap>
              <AuthBootstrap>
                <BrowserRouter>
                  <HotkeyProvider>
                    <DeepLinkBootstrap>
                      <StreamingBootstrap>
                        <OnboardingWizard />
                        <AppShell>
                          <DesktopChrome />
                          <TauriGuardBanner />
                          <LaunchErrorBanner />
                          <UpdateBanner />
                          <RaidBanner />
                          <ChannelPointsPollOverlay />
                          <AppErrorBoundary fallback={<span hidden />}>
                            <ChannelPointsHudSync />
                          </AppErrorBoundary>
                          <AppRoutes />
                        </AppShell>
                      </StreamingBootstrap>
                    </DeepLinkBootstrap>
                  </HotkeyProvider>
                </BrowserRouter>
              </AuthBootstrap>
            </SentryBootstrap>
          </DebugDiagnosticsBootstrap>
        </SettingsBootstrap>
      </ThemeProvider>
    </QueryClientProvider>
  );
}
