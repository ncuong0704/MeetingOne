import type { Step } from 'react-joyride';

export interface TourDefinition {
  id: string;
  title: string;
  description: string;
  steps: Step[];
  requiresHomePage?: boolean;
  requiresExpandedSidebar?: boolean;
}
