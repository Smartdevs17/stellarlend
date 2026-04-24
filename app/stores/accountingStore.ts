import { create } from 'zustand';
import { persist } from 'zustand/middleware';

// ============================================================================
// Types
// ============================================================================

export enum RecognitionMethod {
  StraightLine = 0,
  UsageBased = 1,
  MilestoneBased = 2,
}

export interface RevenueRecognitionRule {
  subscriptionId: number;
  method: RecognitionMethod;
  recognitionPeriod: number;
  merchantId: string;
  totalAmount: string;
  startTime: number;
  endTime: number;
  createdAt: number;
}

export interface Recognition {
  subscriptionId: number;
  merchantId: string;
  recognizedAmount: string;
  deferredAmount: string;
  recognitionDate: number;
  periodStart: number;
  periodEnd: number;
}

export interface ScheduleEntry {
  periodStart: number;
  periodEnd: number;
  scheduledAmount: string;
  recognizedAmount: string;
  isRecognized: boolean;
}

export interface RevenueSchedule {
  subscriptionId: number;
  merchantId: string;
  totalAmount: string;
  totalRecognized: string;
  totalDeferred: string;
  entries: ScheduleEntry[];
  method: RecognitionMethod;
}

export interface RevenueAnalytics {
  merchantId: string;
  periodStart: number;
  periodEnd: number;
  totalRevenue: string;
  recognizedRevenue: string;
  deferredRevenue: string;
  subscriptionCount: number;
  averageSubscriptionValue: string;
}

export interface SubscriptionState {
  subscriptionId: number;
  merchantId: string;
  totalAmount: string;
  recognizedAmount: string;
  deferredAmount: string;
  startTime: number;
  endTime: number;
  isActive: boolean;
  isCancelled: boolean;
  cancellationTime?: number;
  lastRecognitionTime: number;
}

// ============================================================================
// Store State
// ============================================================================

interface AccountingState {
  // Data
  subscriptions: Map<number, SubscriptionState>;
  recognitionRules: Map<number, RevenueRecognitionRule>;
  revenueSchedules: Map<number, RevenueSchedule>;
  recognitions: Recognition[];
  analytics: RevenueAnalytics | null;

  // Loading states
  isLoading: boolean;
  error: string | null;

  // Actions
  setSubscriptions: (subscriptions: SubscriptionState[]) => void;
  addSubscription: (subscription: SubscriptionState) => void;
  updateSubscription: (subscriptionId: number, updates: Partial<SubscriptionState>) => void;
  
  setRecognitionRule: (rule: RevenueRecognitionRule) => void;
  getRecognitionRule: (subscriptionId: number) => RevenueRecognitionRule | undefined;
  
  setRevenueSchedule: (schedule: RevenueSchedule) => void;
  getRevenueSchedule: (subscriptionId: number) => RevenueSchedule | undefined;
  
  addRecognition: (recognition: Recognition) => void;
  getRecognitions: (subscriptionId?: number) => Recognition[];
  
  setAnalytics: (analytics: RevenueAnalytics) => void;
  
  setLoading: (isLoading: boolean) => void;
  setError: (error: string | null) => void;
  
  reset: () => void;
}

// ============================================================================
// Store Implementation
// ============================================================================

const initialState = {
  subscriptions: new Map(),
  recognitionRules: new Map(),
  revenueSchedules: new Map(),
  recognitions: [],
  analytics: null,
  isLoading: false,
  error: null,
};

export const useAccountingStore = create<AccountingState>()(
  persist(
    (set, get) => ({
      ...initialState,

      setSubscriptions: (subscriptions) =>
        set({
          subscriptions: new Map(
            subscriptions.map((sub) => [sub.subscriptionId, sub])
          ),
        }),

      addSubscription: (subscription) =>
        set((state) => {
          const newSubscriptions = new Map(state.subscriptions);
          newSubscriptions.set(subscription.subscriptionId, subscription);
          return { subscriptions: newSubscriptions };
        }),

      updateSubscription: (subscriptionId, updates) =>
        set((state) => {
          const subscription = state.subscriptions.get(subscriptionId);
          if (!subscription) return state;

          const newSubscriptions = new Map(state.subscriptions);
          newSubscriptions.set(subscriptionId, { ...subscription, ...updates });
          return { subscriptions: newSubscriptions };
        }),

      setRecognitionRule: (rule) =>
        set((state) => {
          const newRules = new Map(state.recognitionRules);
          newRules.set(rule.subscriptionId, rule);
          return { recognitionRules: newRules };
        }),

      getRecognitionRule: (subscriptionId) => {
        return get().recognitionRules.get(subscriptionId);
      },

      setRevenueSchedule: (schedule) =>
        set((state) => {
          const newSchedules = new Map(state.revenueSchedules);
          newSchedules.set(schedule.subscriptionId, schedule);
          return { revenueSchedules: newSchedules };
        }),

      getRevenueSchedule: (subscriptionId) => {
        return get().revenueSchedules.get(subscriptionId);
      },

      addRecognition: (recognition) =>
        set((state) => ({
          recognitions: [...state.recognitions, recognition],
        })),

      getRecognitions: (subscriptionId) => {
        const recognitions = get().recognitions;
        if (subscriptionId === undefined) {
          return recognitions;
        }
        return recognitions.filter((r) => r.subscriptionId === subscriptionId);
      },

      setAnalytics: (analytics) => set({ analytics }),

      setLoading: (isLoading) => set({ isLoading }),

      setError: (error) => set({ error }),

      reset: () => set(initialState),
    }),
    {
      name: 'accounting-storage',
      partialize: (state) => ({
        subscriptions: Array.from(state.subscriptions.entries()),
        recognitionRules: Array.from(state.recognitionRules.entries()),
        revenueSchedules: Array.from(state.revenueSchedules.entries()),
        recognitions: state.recognitions,
      }),
      onRehydrateStorage: () => (state) => {
        if (state) {
          // Convert arrays back to Maps
          state.subscriptions = new Map(state.subscriptions as any);
          state.recognitionRules = new Map(state.recognitionRules as any);
          state.revenueSchedules = new Map(state.revenueSchedules as any);
        }
      },
    }
  )
);

// ============================================================================
// Selectors
// ============================================================================

export const selectSubscriptionById = (subscriptionId: number) => (state: AccountingState) =>
  state.subscriptions.get(subscriptionId);

export const selectActiveSubscriptions = (state: AccountingState) =>
  Array.from(state.subscriptions.values()).filter((sub) => sub.isActive);

export const selectTotalDeferredRevenue = (merchantId: string) => (state: AccountingState) =>
  Array.from(state.subscriptions.values())
    .filter((sub) => sub.merchantId === merchantId)
    .reduce((total, sub) => total + BigInt(sub.deferredAmount), BigInt(0))
    .toString();

export const selectTotalRecognizedRevenue = (merchantId: string) => (state: AccountingState) =>
  Array.from(state.subscriptions.values())
    .filter((sub) => sub.merchantId === merchantId)
    .reduce((total, sub) => total + BigInt(sub.recognizedAmount), BigInt(0))
    .toString();
