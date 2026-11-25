import React, { lazy, Suspense } from 'react';
import { BrowserRouter, Routes, Route, Link } from 'react-router-dom';

// Route-level dynamic imports with code splitting chunks
const OverviewPage = lazy(() => import(/* webpackChunkName: "portal-overview" */ './pages/Overview'));
const DocsPage = lazy(() => import(/* webpackChunkName: "portal-docs" */ './pages/Docs'));
const ApiReferencePage = lazy(() => import(/* webpackChunkName: "portal-api-ref" */ './pages/ApiReference'));
const ContractExplorerPage = lazy(() => import(/* webpackChunkName: "portal-explorer" */ './pages/ContractExplorer'));
const SettingsPage = lazy(() => import(/* webpackChunkName: "portal-settings" */ './pages/Settings'));
const HealthDashboardPage = lazy(() => import(/* webpackChunkName: "portal-health" */ '../app/dashboard/health/HealthDashboardPage'));

export const RouteLoadingFallback: React.FC = () => (
  <div style={{ display: 'flex', alignItems: 'center', justifyContent: 'center', minHeight: '300px' }}>
    <div className="portal-spinner" role="status" aria-label="Loading view">
      <span style={{ fontSize: '14px', color: '#64748b' }}>Loading portal module...</span>
    </div>
  </div>
);

export const DeveloperPortalRouter: React.FC = () => {
  return (
    <BrowserRouter>
      <nav style={{ padding: '16px', borderBottom: '1px solid #e2e8f0', display: 'flex', gap: '16px' }}>
        <Link to="/">Overview</Link>
        <Link to="/docs">Documentation</Link>
        <Link to="/api">API Reference</Link>
        <Link to="/explorer">Contracts</Link>
        <Link to="/settings">Settings</Link>
        <Link to="/dashboard/health">Portfolio Health</Link>
      </nav>
      <Suspense fallback={<RouteLoadingFallback />}>
        <Routes>
          <Route path="/" element={<OverviewPage />} />
          <Route path="/docs" element={<DocsPage />} />
          <Route path="/api" element={<ApiReferencePage />} />
          <Route path="/explorer" element={<ContractExplorerPage />} />
          <Route path="/settings" element={<SettingsPage />} />
          <Route path="/dashboard/health" element={<HealthDashboardPage />} />
        </Routes>
      </Suspense>
    </BrowserRouter>
  );
};
