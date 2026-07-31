'use client';

import { Joyride, STATUS, EVENTS, ACTIONS, type EventData } from 'react-joyride';
import { useUserGuide } from '@/contexts/UserGuideContext';
import { dispatchTemplateTourActionAsync } from '@/components/UserGuide/templateTourNavigation';

const joyrideLocale = {
  back: 'Quay lại',
  close: 'Đóng',
  last: 'Hoàn tất',
  next: 'Tiếp theo',
  skip: 'Bỏ qua',
};

const joyrideStyles = {
  options: {
    arrowColor: '#ffffff',
    backgroundColor: '#ffffff',
    overlayColor: 'rgba(0, 0, 0, 0.5)',
    primaryColor: '#16478e',
    textColor: '#1f2937',
    zIndex: 10000,
  },
  buttonNext: {
    backgroundColor: '#16478e',
    borderRadius: '0.5rem',
    fontSize: '0.875rem',
    padding: '0.5rem 1rem',
  },
  buttonBack: {
    color: '#6b7280',
    fontSize: '0.875rem',
    marginRight: '0.5rem',
  },
  buttonSkip: {
    color: '#9ca3af',
    fontSize: '0.875rem',
  },
  tooltip: {
    borderRadius: '0.75rem',
    padding: '1rem',
  },
  tooltipTitle: {
    fontSize: '1rem',
    fontWeight: 600,
    marginBottom: '0.5rem',
    textAlign: 'left' as const,
  },
  tooltipContent: {
    fontSize: '0.875rem',
    lineHeight: '1.5',
    textAlign: 'left' as const,
  },
};

export function UserGuideJoyride() {
  const { run, steps, activeTourId, stopTour } = useUserGuide();

  const handleEvent = (data: EventData) => {
    if (
      data.type === EVENTS.STEP_AFTER &&
      data.action === ACTIONS.NEXT &&
      activeTourId === 'create-template' &&
      data.index !== undefined
    ) {
      const step = steps[data.index];
      const templateActionOnNext = (step?.data as { templateActionOnNext?: Parameters<typeof dispatchTemplateTourActionAsync>[0] })
        ?.templateActionOnNext;
      if (templateActionOnNext) {
        void dispatchTemplateTourActionAsync(templateActionOnNext);
      }
    }

    if (
      data.type === 'tour:end' ||
      (data.type === 'tour:status' &&
        (data.status === STATUS.FINISHED || data.status === STATUS.SKIPPED))
    ) {
      stopTour();
    }
  };

  if (!run || steps.length === 0) return null;

  return (
    <Joyride
      run={run}
      steps={steps}
      continuous
      scrollToFirstStep
      locale={joyrideLocale}
      styles={joyrideStyles}
      options={{
        showProgress: true,
        buttons: ['back', 'close', 'primary', 'skip'],
        primaryColor: '#16478e',
        textColor: '#1f2937',
        backgroundColor: '#ffffff',
        overlayColor: 'rgba(0, 0, 0, 0.5)',
        zIndex: 10000,
      }}
      floatingOptions={{
        shiftOptions: { padding: 16 },
        flipOptions: { padding: 16 },
      }}
      onEvent={handleEvent}
    />
  );
}
