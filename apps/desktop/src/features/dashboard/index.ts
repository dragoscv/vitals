export { DashboardScreen, type DashboardScreenProps } from './DashboardScreen';
export { registerDashboardStrings, DASHBOARD_NS } from './strings';
export { history, HistoryCollector, type MetricHistory } from './history';
export {
  createManualSystemSource,
  createTauriSystemSource,
  type SystemSnapshot,
  type SystemSource,
} from './useSystemSnapshot';
export { defaultLayout, widgetCatalogue, type DashboardLayout, type WidgetId } from './widgets';
