import React, { useEffect, useState } from 'react';
import {
  useAccountingStore,
  RecognitionMethod,
  RevenueAnalytics,
  ScheduleEntry,
} from '../stores/accountingStore';

// ============================================================================
// Types
// ============================================================================

interface RevenueReportScreenProps {
  merchantId: string;
}

// ============================================================================
// Component
// ============================================================================

export const RevenueReportScreen: React.FC<RevenueReportScreenProps> = ({ merchantId }) => {
  const {
    subscriptions,
    revenueSchedules,
    recognitions,
    analytics,
    isLoading,
    error,
    setAnalytics,
    setLoading,
    setError,
  } = useAccountingStore();

  const [selectedPeriod, setSelectedPeriod] = useState<'month' | 'quarter' | 'year'>('month');
  const [selectedSubscriptionId, setSelectedSubscriptionId] = useState<number | null>(null);

  // Filter subscriptions for the merchant
  const merchantSubscriptions = Array.from(subscriptions.values()).filter(
    (sub) => sub.merchantId === merchantId
  );

  // Calculate period dates
  const getPeriodDates = () => {
    const now = Date.now();
    const msPerDay = 24 * 60 * 60 * 1000;
    
    let startTime: number;
    switch (selectedPeriod) {
      case 'month':
        startTime = now - 30 * msPerDay;
        break;
      case 'quarter':
        startTime = now - 90 * msPerDay;
        break;
      case 'year':
        startTime = now - 365 * msPerDay;
        break;
    }
    
    return { startTime: Math.floor(startTime / 1000), endTime: Math.floor(now / 1000) };
  };

  // Load analytics
  useEffect(() => {
    const loadAnalytics = async () => {
      setLoading(true);
      setError(null);

      try {
        const { startTime, endTime } = getPeriodDates();
        
        // In production, this would call the contract
        // For now, calculate from local state
        const totalRevenue = merchantSubscriptions.reduce(
          (sum, sub) => sum + BigInt(sub.totalAmount),
          BigInt(0)
        );
        const recognizedRevenue = merchantSubscriptions.reduce(
          (sum, sub) => sum + BigInt(sub.recognizedAmount),
          BigInt(0)
        );
        const deferredRevenue = merchantSubscriptions.reduce(
          (sum, sub) => sum + BigInt(sub.deferredAmount),
          BigInt(0)
        );

        const mockAnalytics: RevenueAnalytics = {
          merchantId,
          periodStart: startTime,
          periodEnd: endTime,
          totalRevenue: totalRevenue.toString(),
          recognizedRevenue: recognizedRevenue.toString(),
          deferredRevenue: deferredRevenue.toString(),
          subscriptionCount: merchantSubscriptions.length,
          averageSubscriptionValue:
            merchantSubscriptions.length > 0
              ? (totalRevenue / BigInt(merchantSubscriptions.length)).toString()
              : '0',
        };

        setAnalytics(mockAnalytics);
      } catch (err) {
        setError(err instanceof Error ? err.message : 'Failed to load analytics');
      } finally {
        setLoading(false);
      }
    };

    loadAnalytics();
  }, [merchantId, selectedPeriod]);

  // Format currency
  const formatCurrency = (stroops: string) => {
    const xlm = Number(stroops) / 10_000_000;
    return new Intl.NumberFormat('en-US', {
      style: 'currency',
      currency: 'USD',
      minimumFractionDigits: 2,
    }).format(xlm);
  };

  // Format date
  const formatDate = (timestamp: number) => {
    return new Date(timestamp * 1000).toLocaleDateString('en-US', {
      year: 'numeric',
      month: 'short',
      day: 'numeric',
    });
  };

  // Get recognition method label
  const getMethodLabel = (method: RecognitionMethod) => {
    switch (method) {
      case RecognitionMethod.StraightLine:
        return 'Straight-Line';
      case RecognitionMethod.UsageBased:
        return 'Usage-Based';
      case RecognitionMethod.MilestoneBased:
        return 'Milestone-Based';
      default:
        return 'Unknown';
    }
  };

  // Render loading state
  if (isLoading) {
    return (
      <div className="revenue-report-screen loading">
        <div className="spinner" />
        <p>Loading revenue data...</p>
      </div>
    );
  }

  // Render error state
  if (error) {
    return (
      <div className="revenue-report-screen error">
        <div className="error-message">
          <h3>Error</h3>
          <p>{error}</p>
        </div>
      </div>
    );
  }

  return (
    <div className="revenue-report-screen">
      {/* Header */}
      <div className="header">
        <h1>Revenue Recognition Report</h1>
        <div className="period-selector">
          <button
            className={selectedPeriod === 'month' ? 'active' : ''}
            onClick={() => setSelectedPeriod('month')}
          >
            Month
          </button>
          <button
            className={selectedPeriod === 'quarter' ? 'active' : ''}
            onClick={() => setSelectedPeriod('quarter')}
          >
            Quarter
          </button>
          <button
            className={selectedPeriod === 'year' ? 'active' : ''}
            onClick={() => setSelectedPeriod('year')}
          >
            Year
          </button>
        </div>
      </div>

      {/* Analytics Summary */}
      {analytics && (
        <div className="analytics-summary">
          <div className="metric-card">
            <h3>Total Revenue</h3>
            <p className="amount">{formatCurrency(analytics.totalRevenue)}</p>
          </div>
          <div className="metric-card">
            <h3>Recognized Revenue</h3>
            <p className="amount recognized">{formatCurrency(analytics.recognizedRevenue)}</p>
          </div>
          <div className="metric-card">
            <h3>Deferred Revenue</h3>
            <p className="amount deferred">{formatCurrency(analytics.deferredRevenue)}</p>
          </div>
          <div className="metric-card">
            <h3>Active Subscriptions</h3>
            <p className="count">{analytics.subscriptionCount}</p>
          </div>
          <div className="metric-card">
            <h3>Avg Subscription Value</h3>
            <p className="amount">{formatCurrency(analytics.averageSubscriptionValue)}</p>
          </div>
        </div>
      )}

      {/* Subscriptions List */}
      <div className="subscriptions-section">
        <h2>Subscriptions</h2>
        <div className="subscriptions-list">
          {merchantSubscriptions.length === 0 ? (
            <p className="empty-state">No subscriptions found</p>
          ) : (
            <table>
              <thead>
                <tr>
                  <th>ID</th>
                  <th>Total Amount</th>
                  <th>Recognized</th>
                  <th>Deferred</th>
                  <th>Status</th>
                  <th>Start Date</th>
                  <th>Actions</th>
                </tr>
              </thead>
              <tbody>
                {merchantSubscriptions.map((sub) => (
                  <tr key={sub.subscriptionId}>
                    <td>{sub.subscriptionId}</td>
                    <td>{formatCurrency(sub.totalAmount)}</td>
                    <td className="recognized">{formatCurrency(sub.recognizedAmount)}</td>
                    <td className="deferred">{formatCurrency(sub.deferredAmount)}</td>
                    <td>
                      <span className={`status ${sub.isActive ? 'active' : 'inactive'}`}>
                        {sub.isCancelled ? 'Cancelled' : sub.isActive ? 'Active' : 'Inactive'}
                      </span>
                    </td>
                    <td>{formatDate(sub.startTime)}</td>
                    <td>
                      <button
                        className="btn-view"
                        onClick={() => setSelectedSubscriptionId(sub.subscriptionId)}
                      >
                        View Schedule
                      </button>
                    </td>
                  </tr>
                ))}
              </tbody>
            </table>
          )}
        </div>
      </div>

      {/* Revenue Schedule Detail */}
      {selectedSubscriptionId && (
        <div className="schedule-detail">
          <div className="schedule-header">
            <h2>Revenue Schedule - Subscription #{selectedSubscriptionId}</h2>
            <button className="btn-close" onClick={() => setSelectedSubscriptionId(null)}>
              ×
            </button>
          </div>

          {(() => {
            const schedule = revenueSchedules.get(selectedSubscriptionId);
            if (!schedule) {
              return <p>No schedule found</p>;
            }

            return (
              <div className="schedule-content">
                <div className="schedule-summary">
                  <div className="summary-item">
                    <label>Recognition Method:</label>
                    <span>{getMethodLabel(schedule.method)}</span>
                  </div>
                  <div className="summary-item">
                    <label>Total Amount:</label>
                    <span>{formatCurrency(schedule.totalAmount)}</span>
                  </div>
                  <div className="summary-item">
                    <label>Total Recognized:</label>
                    <span className="recognized">{formatCurrency(schedule.totalRecognized)}</span>
                  </div>
                  <div className="summary-item">
                    <label>Total Deferred:</label>
                    <span className="deferred">{formatCurrency(schedule.totalDeferred)}</span>
                  </div>
                </div>

                <table className="schedule-table">
                  <thead>
                    <tr>
                      <th>Period</th>
                      <th>Scheduled Amount</th>
                      <th>Recognized Amount</th>
                      <th>Status</th>
                    </tr>
                  </thead>
                  <tbody>
                    {schedule.entries.map((entry: ScheduleEntry, index: number) => (
                      <tr key={index}>
                        <td>
                          {formatDate(entry.periodStart)} - {formatDate(entry.periodEnd)}
                        </td>
                        <td>{formatCurrency(entry.scheduledAmount)}</td>
                        <td className="recognized">{formatCurrency(entry.recognizedAmount)}</td>
                        <td>
                          <span className={`status ${entry.isRecognized ? 'recognized' : 'pending'}`}>
                            {entry.isRecognized ? 'Recognized' : 'Pending'}
                          </span>
                        </td>
                      </tr>
                    ))}
                  </tbody>
                </table>
              </div>
            );
          })()}
        </div>
      )}

      {/* Recent Recognitions */}
      <div className="recognitions-section">
        <h2>Recent Revenue Recognitions</h2>
        <div className="recognitions-list">
          {recognitions.length === 0 ? (
            <p className="empty-state">No recognitions yet</p>
          ) : (
            <table>
              <thead>
                <tr>
                  <th>Date</th>
                  <th>Subscription ID</th>
                  <th>Recognized Amount</th>
                  <th>Deferred Balance</th>
                  <th>Period</th>
                </tr>
              </thead>
              <tbody>
                {recognitions
                  .filter((r) => r.merchantId === merchantId)
                  .sort((a, b) => b.recognitionDate - a.recognitionDate)
                  .slice(0, 10)
                  .map((recognition, index) => (
                    <tr key={index}>
                      <td>{formatDate(recognition.recognitionDate)}</td>
                      <td>{recognition.subscriptionId}</td>
                      <td className="recognized">{formatCurrency(recognition.recognizedAmount)}</td>
                      <td className="deferred">{formatCurrency(recognition.deferredAmount)}</td>
                      <td>
                        {formatDate(recognition.periodStart)} - {formatDate(recognition.periodEnd)}
                      </td>
                    </tr>
                  ))}
              </tbody>
            </table>
          )}
        </div>
      </div>
    </div>
  );
};

export default RevenueReportScreen;
