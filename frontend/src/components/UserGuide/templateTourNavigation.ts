export const TEMPLATE_TOUR_EVENT = 'user-guide:template-tour';

export const BUILTIN_ACT_TEMPLATE_ID = 'theo_mau_act_no_table';

export type TemplateTourAction =
  | { type: 'showList' }
  | { type: 'startNew' }
  | { type: 'addSection' }
  | { type: 'ensureMinSections'; count: number }
  | { type: 'openTemplate'; templateId: string }
  | { type: 'cloneTemplate'; templateId: string }
  | { type: 'closeEditor' }
  | { type: 'saveTemplate' };

export interface TemplateTourEventDetail {
  action: TemplateTourAction;
  resolve?: () => void;
}

export function dispatchTemplateTourAction(action: TemplateTourAction) {
  void dispatchTemplateTourActionAsync(action);
}

export function dispatchTemplateTourActionAsync(action: TemplateTourAction): Promise<void> {
  return new Promise((resolve) => {
    window.dispatchEvent(
      new CustomEvent<TemplateTourEventDetail>(TEMPLATE_TOUR_EVENT, {
        detail: { action, resolve },
      }),
    );
  });
}
