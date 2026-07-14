import { createHashRouter, Navigate } from "react-router-dom";
import { MainLayout } from "@/components/layout/MainLayout";
import { LibraryPage } from "@/pages/LibraryPage";
import { WorkflowsPage } from "@/pages/WorkflowsPage";
import { RunnerPage } from "@/pages/RunnerPage";
import { HistoryPage } from "@/pages/HistoryPage";
import { SchedulesPage } from "@/pages/SchedulesPage";
import { SettingsPage } from "@/pages/SettingsPage";
import { PluginsPage } from "@/pages/PluginsPage";

export const router = createHashRouter([
  {
    path: "/",
    element: <MainLayout />,
    children: [
      { index: true, element: <Navigate to="/library" replace /> },
      { path: "library", element: <LibraryPage /> },
      { path: "library/:id", element: <LibraryPage /> },
      { path: "workflows", element: <WorkflowsPage /> },
      { path: "workflows/:id", element: <WorkflowsPage /> },
      { path: "runner", element: <RunnerPage /> },
      { path: "runner/:executionId", element: <RunnerPage /> },
      { path: "history", element: <HistoryPage /> },
      { path: "schedules", element: <SchedulesPage /> },
      { path: "settings", element: <SettingsPage /> },
      { path: "plugins", element: <PluginsPage /> },
    ],
  },
]);
