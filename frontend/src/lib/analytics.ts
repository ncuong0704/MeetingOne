/**
 * Bản nội bộ: không gửi telemetry / PostHog.
 * Giữ API và kiểu để các component hiện tại không cần sửa logic.
 */

export interface AnalyticsProperties {
  [key: string]: string;
}

export interface DeviceInfo {
  platform: string;
  os_version: string;
  architecture: string;
}

export interface UserSession {
  session_id: string;
  user_id: string;
  start_time: string;
  last_heartbeat: string;
  is_active: boolean;
}

export default class Analytics {
  private static initialized = false;
  private static currentUserId: string | null = null;
  private static sessionStartTime: number | null = null;
  private static meetingsInSession = 0;
  private static deviceInfo: DeviceInfo | null = null;

  static async init(): Promise<void> {
    this.initialized = true;
  }

  static async disable(): Promise<void> {
    this.initialized = false;
    this.currentUserId = null;
  }

  static async isEnabled(): Promise<boolean> {
    return false;
  }

  static async track(_eventName: string, _properties?: AnalyticsProperties): Promise<void> {}

  static async identify(userId: string, _properties?: AnalyticsProperties): Promise<void> {
    this.currentUserId = userId;
  }

  static async startSession(userId: string): Promise<string | null> {
    this.currentUserId = userId;
    return null;
  }

  static async endSession(): Promise<void> {}

  static async trackDailyActiveUser(): Promise<void> {}

  static async trackUserFirstLaunch(): Promise<void> {}

  static async isSessionActive(): Promise<boolean> {
    return false;
  }

  static async getPersistentUserId(): Promise<string> {
    return 'local';
  }

  static async checkAndTrackFirstLaunch(): Promise<void> {}

  static async checkAndTrackDailyUsage(): Promise<void> {}

  static getCurrentUserId(): string | null {
    return this.currentUserId;
  }

  static async getPlatform(): Promise<string> {
    const ua = navigator.userAgent.toLowerCase();
    if (ua.includes('mac')) return 'macOS';
    if (ua.includes('win')) return 'Windows';
    if (ua.includes('linux')) return 'Linux';
    return 'unknown';
  }

  static async getOSVersion(): Promise<string> {
    try {
      const platform = await this.getPlatform();
      return `${platform} (${navigator.userAgent})`;
    } catch {
      return 'unknown';
    }
  }

  static async getDeviceInfo(): Promise<DeviceInfo> {
    if (this.deviceInfo) return this.deviceInfo;

    const platform = await this.getPlatform();
    const ua = navigator.userAgent.toLowerCase();
    let architecture = 'unknown';
    if (ua.includes('arm') || ua.includes('aarch64')) architecture = 'aarch64';
    else if (ua.includes('x86_64') || ua.includes('x64')) architecture = 'x86_64';
    else if (ua.includes('x86')) architecture = 'x86';

    this.deviceInfo = {
      platform,
      os_version: await this.getOSVersion(),
      architecture,
    };
    return this.deviceInfo;
  }

  static async calculateDaysSince(_dateKey: string): Promise<number | null> {
    return null;
  }

  static async updateMeetingCount(): Promise<void> {}

  static async getMeetingsCountToday(): Promise<number> {
    return 0;
  }

  static async hasUsedFeatureBefore(_featureName: string): Promise<boolean> {
    return false;
  }

  static async markFeatureUsed(_featureName: string): Promise<void> {}

  static async trackSessionStarted(_sessionId: string): Promise<void> {}

  static async trackSessionEnded(_sessionId: string): Promise<void> {}

  static async trackMeetingCompleted(
    _meetingId: string,
    _metrics: {
      duration_seconds: number;
      transcript_segments: number;
      transcript_word_count: number;
      words_per_minute: number;
      meetings_today: number;
    }
  ): Promise<void> {}

  static async trackFeatureUsedEnhanced(
    _featureName: string,
    _properties?: Record<string, unknown>
  ): Promise<void> {}

  static async trackCopy(
    _copyType: 'transcript' | 'summary',
    _properties?: Record<string, unknown>
  ): Promise<void> {}

  static async trackMeetingStarted(_meetingId: string, _meetingTitle: string): Promise<void> {}

  static async trackRecordingStarted(_meetingId: string): Promise<void> {}

  static async trackRecordingStopped(_meetingId: string, _durationSeconds?: number): Promise<void> {}

  static async trackMeetingDeleted(_meetingId: string): Promise<void> {}

  static async trackSettingsChanged(_settingType: string, _newValue: string): Promise<void> {}

  static async trackFeatureUsed(_featureName: string): Promise<void> {}

  static async trackPageView(_pageName: string): Promise<void> {}

  static async trackButtonClick(_buttonName: string, _location?: string): Promise<void> {}

  static async trackError(_errorType: string, _errorMessage: string): Promise<void> {}

  static async trackAppStarted(): Promise<void> {}

  static async cleanup(): Promise<void> {}

  static reset(): void {
    this.initialized = false;
    this.currentUserId = null;
  }

  static async waitForInitialization(): Promise<boolean> {
    return false;
  }

  static async trackBackendConnection(_success: boolean, _error?: string): Promise<void> {}

  static async trackTranscriptionError(_errorMessage: string): Promise<void> {}

  static async trackTranscriptionSuccess(_duration?: number): Promise<void> {}

  static async trackSummaryGenerationStarted(
    _modelProvider: string,
    _modelName: string,
    _transcriptLength: number,
    _timeSinceRecordingMinutes?: number
  ): Promise<void> {}

  static async trackSummaryGenerationCompleted(
    _modelProvider: string,
    _modelName: string,
    _success: boolean,
    _durationSeconds?: number,
    _errorMessage?: string
  ): Promise<void> {}

  static async trackSummaryRegenerated(_modelProvider: string, _modelName: string): Promise<void> {}

  static async trackModelChanged(
    _oldProvider: string,
    _oldModel: string,
    _newProvider: string,
    _newModel: string
  ): Promise<void> {}

  static async trackCustomPromptUsed(_promptLength: number): Promise<void> {}
}
