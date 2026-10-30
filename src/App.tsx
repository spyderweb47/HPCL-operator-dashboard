import { HashRouter, Navigate, Route, Routes } from "react-router";
import { Layout } from "@/components/Layout";
import { DayEndPage } from "@/pages/DayEndPage";
import { HistoryPage } from "@/pages/HistoryPage";
import { RegisterPage } from "@/pages/RegisterPage";
import { SettingsPage } from "@/pages/SettingsPage";

// Hash routing: the app is served from local files, so there is no server to resolve deep links.
export function App() {
  return (
    <HashRouter>
      <Routes>
        <Route element={<Layout />}>
          <Route index element={<RegisterPage />} />
          <Route path="day-end" element={<DayEndPage />} />
          <Route path="history" element={<HistoryPage />} />
          <Route path="settings" element={<SettingsPage />} />
          <Route path="*" element={<Navigate to="/" replace />} />
        </Route>
      </Routes>
    </HashRouter>
  );
}
